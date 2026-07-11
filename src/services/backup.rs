use base64::{engine::general_purpose::STANDARD, Engine as _};
use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::error::{AppError, Result};
use crate::jobs::Job;
use crate::models::{AutomationEvent, Notification, Settings, Subscription};
use crate::store::schema::{decode_store_json, StoreKind, CURRENT_SCHEMA_VERSION};
use crate::utils::metrics::Metrics;
use crate::utils::{set_file_mode, unix_now, write_file_atomic};

const BACKUP_FORMAT_VERSION: u32 = 1;
const DEFAULT_RETENTION: usize = 7;
const MAX_FILES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupArchive {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub schema_version: u32,
    pub created_at: i64,
    pub files: Vec<BackupFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupPreview {
    pub valid: bool,
    pub format_version: u32,
    pub app_version: String,
    pub schema_version: u32,
    pub created_at: i64,
    pub file_count: usize,
    pub total_bytes: u64,
    pub files: Vec<BackupFilePreview>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupFilePreview {
    pub path: String,
    pub size: u64,
    pub schema_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredBackup {
    pub name: String,
    pub size: u64,
    pub modified_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreResult {
    pub restored_files: usize,
    pub snapshot: String,
    pub restart_required: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct BackupPolicy {
    pub interval: Duration,
    pub retention: usize,
    pub max_archive_bytes: u64,
    pub max_storage_bytes: u64,
}

impl BackupPolicy {
    pub fn from_env() -> Self {
        Self {
            interval: Duration::from_secs(env_u64("BACKUP_INTERVAL_HOURS", 24) * 3600),
            retention: env_u64("BACKUP_RETENTION", DEFAULT_RETENTION as u64).clamp(1, 100) as usize,
            max_archive_bytes: env_u64("BACKUP_MAX_ARCHIVE_MB", 256) * 1024 * 1024,
            max_storage_bytes: env_u64("BACKUP_MAX_STORAGE_MB", 512) * 1024 * 1024,
        }
    }
}

pub struct BackupService {
    data_dir: PathBuf,
    backup_dir: PathBuf,
    policy: BackupPolicy,
    operation_lock: Mutex<()>,
    metrics: Arc<Metrics>,
}

impl BackupService {
    pub fn new(data_dir: impl Into<PathBuf>, metrics: Arc<Metrics>) -> Self {
        Self::with_policy(data_dir, metrics, BackupPolicy::from_env())
    }

    pub fn with_policy(
        data_dir: impl Into<PathBuf>,
        metrics: Arc<Metrics>,
        policy: BackupPolicy,
    ) -> Self {
        let data_dir = data_dir.into();
        let backup_dir = data_dir.join("backups");
        Self {
            data_dir,
            backup_dir,
            policy,
            operation_lock: Mutex::new(()),
            metrics,
        }
    }

    pub fn start(self: Arc<Self>) {
        if self.policy.interval.is_zero() {
            return;
        }
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.policy.interval);
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(error) = self.create_stored_backup("scheduled").await {
                    tracing::error!("定时备份失败: {}", error);
                }
            }
        });
    }

    pub async fn export_archive(&self) -> Result<BackupArchive> {
        let data_dir = self.data_dir.clone();
        let max_bytes = self.policy.max_archive_bytes;
        tokio::task::spawn_blocking(move || build_archive(&data_dir, max_bytes))
            .await
            .map_err(|error| AppError::Internal(format!("备份任务异常退出: {error}")))?
    }

    pub async fn create_stored_backup(&self, label: &str) -> Result<StoredBackup> {
        let _guard = self.operation_lock.lock().await;
        let archive = self.export_archive().await?;
        let bytes = serde_json::to_vec_pretty(&archive)
            .map_err(|error| AppError::Internal(format!("序列化备份失败: {error}")))?;
        let safe_label = sanitize_label(label);
        let unique = uuid::Uuid::new_v4().simple().to_string();
        let name = format!(
            "backup-{}-{}-{}.json",
            archive.created_at,
            safe_label,
            &unique[..8]
        );
        let path = self.backup_dir.join(&name);
        let projected = backup_storage_size(&self.backup_dir)?
            .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if projected > self.policy.max_storage_bytes {
            self.prune_locked(self.policy.retention.saturating_sub(1))?;
            let projected = backup_storage_size(&self.backup_dir)?
                .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
            if projected > self.policy.max_storage_bytes {
                self.metrics.increment_backup_failure();
                return Err(AppError::RateLimited(format!(
                    "备份存储预算不足：上限 {} MiB",
                    self.policy.max_storage_bytes / 1024 / 1024
                )));
            }
        }
        write_file_atomic(&path, &bytes, 0o600)?;
        set_file_mode(&path, 0o600)?;
        self.prune_locked(self.policy.retention)?;
        self.metrics.increment_backup_success();
        stored_backup_from_path(&path)
    }

    pub async fn list_stored_backups(&self) -> Result<Vec<StoredBackup>> {
        let backup_dir = self.backup_dir.clone();
        tokio::task::spawn_blocking(move || list_backups(&backup_dir))
            .await
            .map_err(|error| AppError::Internal(format!("列出备份任务异常退出: {error}")))?
    }

    pub async fn preview(&self, archive: &BackupArchive) -> Result<BackupPreview> {
        validate_archive(archive, self.policy.max_archive_bytes)
    }

    pub async fn restore(
        &self,
        archive: &BackupArchive,
        confirmation: &str,
    ) -> Result<RestoreResult> {
        if confirmation != "RESTORE DATA" {
            return Err(AppError::Validation(
                "恢复确认文本必须为 RESTORE DATA".to_string(),
            ));
        }
        let _guard = self.operation_lock.lock().await;
        validate_archive(archive, self.policy.max_archive_bytes)?;

        // Snapshot is created before any business file is replaced.
        let current = self.export_archive().await?;
        let unique = uuid::Uuid::new_v4().simple().to_string();
        let snapshot_name = format!("backup-{}-pre-restore-{}.json", unix_now(), &unique[..8]);
        let snapshot_path = self.backup_dir.join(&snapshot_name);
        let snapshot_bytes = serde_json::to_vec_pretty(&current)
            .map_err(|error| AppError::Internal(format!("序列化恢复前快照失败: {error}")))?;
        write_file_atomic(&snapshot_path, &snapshot_bytes, 0o600)?;

        let data_dir = self.data_dir.clone();
        let files = decode_archive_files(archive)?;
        let restored_files = tokio::task::spawn_blocking(move || restore_files(&data_dir, files))
            .await
            .map_err(|error| AppError::Internal(format!("恢复任务异常退出: {error}")))??;

        let restart_plan = serde_json::json!({
            "reason": "data_restore",
            "created_at": unix_now(),
            "snapshot": snapshot_name,
        });
        write_file_atomic(
            &self.data_dir.join("restart-required.json"),
            serde_json::to_vec_pretty(&restart_plan)?.as_slice(),
            0o600,
        )?;
        self.metrics.increment_restore_success();
        Ok(RestoreResult {
            restored_files,
            snapshot: snapshot_name,
            restart_required: true,
            message: "数据已原子恢复；当前进程仍持有旧内存快照，请安全重启服务".to_string(),
        })
    }

    fn prune_locked(&self, keep: usize) -> Result<()> {
        let backups = list_backups(&self.backup_dir)?;
        for backup in backups.into_iter().skip(keep) {
            std::fs::remove_file(self.backup_dir.join(backup.name))
                .map_err(|error| AppError::Database(format!("删除过期备份失败: {error}")))?;
        }
        Ok(())
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn policy(&self) -> &BackupPolicy {
        &self.policy
    }
}

fn build_archive(data_dir: &Path, max_bytes: u64) -> Result<BackupArchive> {
    std::fs::create_dir_all(data_dir)
        .map_err(|error| AppError::Database(format!("创建数据目录失败: {error}")))?;
    let mut paths = Vec::new();
    collect_files(data_dir, data_dir, &mut paths)?;
    paths.sort();
    if paths.len() > MAX_FILES {
        return Err(AppError::Validation(format!(
            "数据文件过多，最多允许 {MAX_FILES} 个"
        )));
    }
    let mut total = 0u64;
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let relative = path.strip_prefix(data_dir).map_err(|_| {
            AppError::Validation(format!("文件不在 DATA_DIR 内: {}", path.display()))
        })?;
        let relative = safe_relative_path(relative)?;
        let bytes = std::fs::read(&path).map_err(|error| {
            AppError::Database(format!("读取 {} 失败: {error}", path.display()))
        })?;
        total = total.saturating_add(bytes.len() as u64);
        if total > max_bytes {
            return Err(AppError::RateLimited(format!(
                "备份内容超过 {} MiB 上限",
                max_bytes / 1024 / 1024
            )));
        }
        files.push(BackupFile {
            path: relative,
            size: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
            content_base64: STANDARD.encode(bytes),
        });
    }
    Ok(BackupArchive {
        format: "my-media-sub-backup".to_string(),
        format_version: BACKUP_FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: CURRENT_SCHEMA_VERSION,
        created_at: unix_now(),
        files,
    })
}

fn collect_files(root: &Path, current: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(current).map_err(|error| {
        AppError::Database(format!("读取目录 {} 失败: {error}", current.display()))
    })? {
        let entry =
            entry.map_err(|error| AppError::Database(format!("读取目录项失败: {error}")))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| AppError::Database(format!("读取文件类型失败: {error}")))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if path == root.join("backups") {
                continue;
            }
            collect_files(root, &path, result)?;
        } else if file_type.is_file() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".tmp") && name != "restart-required.json" {
                result.push(path);
            }
        }
    }
    Ok(())
}

fn validate_archive(archive: &BackupArchive, max_bytes: u64) -> Result<BackupPreview> {
    if archive.format != "my-media-sub-backup" || archive.format_version != BACKUP_FORMAT_VERSION {
        return Err(AppError::Validation("不支持的备份格式或版本".to_string()));
    }
    if archive.schema_version > CURRENT_SCHEMA_VERSION {
        return Err(AppError::Validation(format!(
            "备份 schema {} 高于当前支持版本 {}",
            archive.schema_version, CURRENT_SCHEMA_VERSION
        )));
    }
    if archive.files.len() > MAX_FILES {
        return Err(AppError::Validation("备份文件数量超过安全上限".to_string()));
    }
    let mut seen = HashSet::new();
    let mut total = 0u64;
    let mut previews = Vec::new();
    for file in &archive.files {
        let relative = safe_relative_path(Path::new(&file.path))?;
        if !seen.insert(relative.clone()) {
            return Err(AppError::Validation(format!(
                "备份包含重复路径: {relative}"
            )));
        }
        let bytes = STANDARD
            .decode(&file.content_base64)
            .map_err(|_| AppError::Validation(format!("备份文件 Base64 无效: {relative}")))?;
        if bytes.len() as u64 != file.size || sha256_hex(&bytes) != file.sha256 {
            return Err(AppError::Validation(format!(
                "备份文件校验失败: {relative}"
            )));
        }
        total = total.saturating_add(file.size);
        if total > max_bytes {
            return Err(AppError::RateLimited(
                "备份内容超过恢复安全上限".to_string(),
            ));
        }
        let schema_version = validate_store_file(&relative, &bytes)?;
        previews.push(BackupFilePreview {
            path: relative,
            size: file.size,
            schema_version,
        });
    }
    let mut warnings = Vec::new();
    if !seen.contains("settings.json") {
        warnings.push("备份不包含 settings.json，将保留当前设置".to_string());
    }
    Ok(BackupPreview {
        valid: true,
        format_version: archive.format_version,
        app_version: archive.app_version.clone(),
        schema_version: archive.schema_version,
        created_at: archive.created_at,
        file_count: archive.files.len(),
        total_bytes: total,
        files: previews,
        warnings,
    })
}

fn validate_store_file(path: &str, bytes: &[u8]) -> Result<Option<u32>> {
    let content = std::str::from_utf8(bytes)
        .map_err(|_| AppError::Validation(format!("JSON 文件不是 UTF-8: {path}")))?;
    let kind = match path {
        "settings.json" => decode_store_json::<Settings>(content, StoreKind::Settings).map(|_| ()),
        "subscriptions.json" => {
            decode_store_json::<Vec<Subscription>>(content, StoreKind::Subscriptions).map(|_| ())
        }
        "notifications.json" => {
            decode_store_json::<Vec<Notification>>(content, StoreKind::Notifications).map(|_| ())
        }
        "jobs.json" => decode_store_json::<Vec<Job>>(content, StoreKind::Jobs).map(|_| ()),
        "automation_events.json" => {
            decode_store_json::<Vec<AutomationEvent>>(content, StoreKind::AutomationEvents)
                .map(|_| ())
        }
        _ => return Ok(None),
    };
    kind.map_err(|error| AppError::Validation(format!("{path} schema 校验失败: {error}")))?;
    let value: serde_json::Value = serde_json::from_str(content)?;
    Ok(value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .or(Some(0)))
}

fn decode_archive_files(archive: &BackupArchive) -> Result<Vec<(String, Vec<u8>)>> {
    archive
        .files
        .iter()
        .map(|file| {
            let path = safe_relative_path(Path::new(&file.path))?;
            let bytes = STANDARD
                .decode(&file.content_base64)
                .map_err(|_| AppError::Validation(format!("备份文件 Base64 无效: {path}")))?;
            Ok((path, bytes))
        })
        .collect()
}

fn restore_files(data_dir: &Path, files: Vec<(String, Vec<u8>)>) -> Result<usize> {
    let mut restored = 0;
    for (relative, bytes) in files {
        let path = data_dir.join(&relative);
        if !path.starts_with(data_dir) {
            return Err(AppError::Validation(format!("恢复路径越界: {relative}")));
        }
        reject_symlink_ancestors(data_dir, Path::new(&relative))?;
        write_file_atomic(&path, &bytes, 0o600)?;
        restored += 1;
    }
    Ok(restored)
}

fn reject_symlink_ancestors(data_dir: &Path, relative: &Path) -> Result<()> {
    let mut current = data_dir.to_path_buf();
    for component in relative.components() {
        if let Component::Normal(part) = component {
            current.push(part);
            if let Ok(metadata) = std::fs::symlink_metadata(&current) {
                if metadata.file_type().is_symlink() {
                    return Err(AppError::Validation(format!(
                        "恢复路径包含符号链接: {}",
                        current.display()
                    )));
                }
            }
        }
    }
    Ok(())
}

fn safe_relative_path(path: &Path) -> Result<String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(AppError::Validation(
            "备份路径必须是非空相对路径".to_string(),
        ));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| AppError::Validation("备份路径必须是 UTF-8".to_string()))?;
                if part.is_empty() || part.contains(['/', '\\']) {
                    return Err(AppError::Validation("备份路径包含非法字符".to_string()));
                }
                parts.push(part);
            }
            _ => return Err(AppError::Validation("备份路径包含路径穿越".to_string())),
        }
    }
    let relative = parts.join("/");
    if parts.first().is_some_and(|part| *part == "backups") || relative == "restart-required.json" {
        return Err(AppError::Validation(format!(
            "备份路径使用保留位置: {relative}"
        )));
    }
    Ok(relative)
}

fn list_backups(backup_dir: &Path) -> Result<Vec<StoredBackup>> {
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }
    let mut backups = Vec::new();
    for entry in std::fs::read_dir(backup_dir)
        .map_err(|error| AppError::Database(format!("读取备份目录失败: {error}")))?
    {
        let entry =
            entry.map_err(|error| AppError::Database(format!("读取备份项失败: {error}")))?;
        if entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
            && entry.file_name().to_string_lossy().starts_with("backup-")
            && entry.file_name().to_string_lossy().ends_with(".json")
        {
            backups.push(stored_backup_from_path(&entry.path())?);
        }
    }
    backups.sort_by(|left, right| {
        right
            .modified_at
            .cmp(&left.modified_at)
            .then_with(|| right.name.cmp(&left.name))
    });
    Ok(backups)
}

fn stored_backup_from_path(path: &Path) -> Result<StoredBackup> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| AppError::Database(format!("读取备份元数据失败: {error}")))?;
    Ok(StoredBackup {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AppError::Database("备份文件名不是 UTF-8".to_string()))?
            .to_string(),
        size: metadata.len(),
        modified_at: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default(),
    })
}

fn backup_storage_size(backup_dir: &Path) -> Result<u64> {
    Ok(list_backups(backup_dir)?
        .iter()
        .map(|backup| backup.size)
        .sum())
}

fn sha256_hex(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sanitize_label(label: &str) -> String {
    let label: String = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .take(32)
        .collect();
    if label.is_empty() {
        "manual".to_string()
    } else {
        label
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::metrics::Metrics;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "my-media-sub-backup-{name}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn service(dir: &Path) -> BackupService {
        BackupService::with_policy(
            dir,
            Arc::new(Metrics::default()),
            BackupPolicy {
                interval: Duration::ZERO,
                retention: 2,
                max_archive_bytes: 1024 * 1024,
                max_storage_bytes: 4 * 1024 * 1024,
            },
        )
    }

    #[tokio::test]
    async fn archive_round_trip_previews_and_restores() {
        let dir = temp_dir("round-trip");
        std::fs::create_dir_all(&dir).unwrap();
        let settings = serde_json::json!({"schema_version":1,"data":Settings::default()});
        std::fs::write(
            dir.join("settings.json"),
            serde_json::to_vec(&settings).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), b"before").unwrap();
        let service = service(&dir);
        let archive = service.export_archive().await.unwrap();
        let preview = service.preview(&archive).await.unwrap();
        assert!(preview.valid);
        assert_eq!(preview.file_count, 2);

        std::fs::write(dir.join("notes.txt"), b"after").unwrap();
        let restored = service.restore(&archive, "RESTORE DATA").await.unwrap();
        assert!(restored.restart_required);
        assert_eq!(std::fs::read(dir.join("notes.txt")).unwrap(), b"before");
        assert!(dir.join("backups").join(restored.snapshot).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn rejects_path_traversal_and_tampered_content() {
        let dir = temp_dir("unsafe");
        let service = service(&dir);
        let mut archive = BackupArchive {
            format: "my-media-sub-backup".into(),
            format_version: 1,
            app_version: "test".into(),
            schema_version: 1,
            created_at: 1,
            files: vec![BackupFile {
                path: "../escape.json".into(),
                size: 2,
                sha256: sha256_hex(b"{}"),
                content_base64: STANDARD.encode(b"{}"),
            }],
        };
        assert!(service
            .preview(&archive)
            .await
            .unwrap_err()
            .to_string()
            .contains("路径穿越"));
        archive.files[0].path = "safe.json".into();
        archive.files[0].sha256 = "wrong".into();
        assert!(service
            .preview(&archive)
            .await
            .unwrap_err()
            .to_string()
            .contains("校验失败"));
    }

    #[tokio::test]
    async fn retention_keeps_latest_backups() {
        let dir = temp_dir("retention");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), b"data").unwrap();
        let service = service(&dir);
        for index in 0..3 {
            service
                .create_stored_backup(&format!("manual-{index}"))
                .await
                .unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(service.list_stored_backups().await.unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
