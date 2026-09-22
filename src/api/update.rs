use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use ring::digest;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use tokio::io::AsyncWriteExt;

use super::response::ApiResponse as Response;
use crate::clients::http_pool;
use crate::error::{AppError, Result};
use crate::restart::RestartPlan;
use crate::utils::constant_time_eq;
use crate::utils::format_bytes;

const GITHUB_REPO: &str = "hellomrli/my-media-sub";
const REQUIRED_STATIC_ASSETS: &[&str] = &[
    "index.html",
    "manifest.webmanifest",
    "service-worker.js",
    "openapi.json",
];
static UPDATE_PROGRESS: LazyLock<Mutex<UpdateProgressResponse>> =
    LazyLock::new(|| Mutex::new(UpdateProgressResponse::idle()));
static PENDING_RESTART: LazyLock<Mutex<Option<RestartPlan>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateAsset {
    pub name: String,
    pub size: u64,
    pub download_url: String,
}

impl From<GithubAsset> for UpdateAsset {
    fn from(asset: GithubAsset) -> Self {
        Self {
            name: asset.name,
            size: asset.size,
            download_url: asset.browser_download_url,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UpdateCheckResponse {
    pub repository: String,
    pub current_version: String,
    pub latest_version: String,
    pub latest_tag: String,
    pub update_available: bool,
    pub release_name: String,
    pub release_url: String,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub checked_at: String,
    pub runtime: String,
    /// Whether this process can atomically replace its executable and WebUI.
    /// Standard Docker images opt in by running from a writable persistent
    /// runtime volume instead of directly from the immutable image layer.
    pub online_update_supported: bool,
    pub linux_x86_64_asset: Option<UpdateAsset>,
}

#[derive(Debug, Serialize)]
pub struct UpdateReleaseResponse {
    pub tag: String,
    pub version: String,
    pub name: String,
    pub release_url: String,
    pub published_at: Option<String>,
    pub asset: Option<UpdateAsset>,
    pub is_current: bool,
    pub is_newer: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateApplyRequest {
    pub tag: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateApplyResponse {
    pub success: bool,
    pub previous_version: String,
    pub new_version: String,
    pub binary_path: String,
    pub backup_path: String,
    pub restart_required: bool,
    pub auto_restart_scheduled: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateRestartResponse {
    pub success: bool,
    pub restart_scheduled: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateProgressResponse {
    pub running: bool,
    pub percent: u8,
    pub stage: String,
    pub message: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub updated_at: String,
}

impl UpdateProgressResponse {
    fn idle() -> Self {
        Self {
            running: false,
            percent: 0,
            stage: "idle".to_string(),
            message: "等待升级".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            error: None,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

async fn check_update() -> Result<impl IntoResponse> {
    let release = fetch_latest_release().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let latest_version = normalize_version(&release.tag_name);
    let update_available = is_newer_version(&latest_version, &current_version);
    let linux_x86_64_asset = find_asset(&release.assets, "linux-x86_64.tar.gz").map(Into::into);
    let runtime = detect_runtime();

    let response = UpdateCheckResponse {
        repository: GITHUB_REPO.to_string(),
        current_version,
        latest_version,
        latest_tag: release.tag_name.clone(),
        update_available,
        release_name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        release_url: release.html_url,
        release_notes: release.body.unwrap_or_default(),
        published_at: release.published_at,
        checked_at: Utc::now().to_rfc3339(),
        online_update_supported: online_update_supported(&runtime),
        runtime,
        linux_x86_64_asset,
    };

    Ok(Json(Response::ok(response)))
}

async fn list_releases() -> Result<impl IntoResponse> {
    let releases = fetch_releases().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let response = releases
        .into_iter()
        .map(|release| release_to_response(release, &current_version))
        .collect::<Vec<_>>();

    Ok(Json(Response::ok(response)))
}

async fn apply_update(request: Option<Json<UpdateApplyRequest>>) -> Result<impl IntoResponse> {
    let runtime = detect_runtime();
    if !online_update_supported(&runtime) {
        return Err(AppError::Validation(online_update_unavailable_message(
            &runtime,
        )));
    }
    ensure_no_pending_restart()?;

    let target_tag = request.and_then(|Json(req)| req.tag).and_then(|tag| {
        let tag = tag.trim().to_string();
        (!tag.is_empty()).then_some(tag)
    });
    let message = target_tag
        .as_deref()
        .map(|tag| format!("正在准备切换到 {}", tag))
        .unwrap_or_else(|| "正在检查最新版本".to_string());

    try_begin_update_progress(message)?;
    // panic 兜底：apply_update_inner 若异常中止，drop guard 会把 running
    // 复位，否则升级进度互斥锁永久卡死，后续所有升级都被拒绝。
    let _progress_reset_guard = UpdateProgressResetGuard;
    match apply_update_inner(target_tag).await {
        Ok(response) => Ok(Json(Response::ok(response))),
        Err(error) => {
            fail_update_progress(error.to_string());
            Err(error)
        }
    }
}

async fn update_progress() -> Result<impl IntoResponse> {
    Ok(Json(Response::ok(current_update_progress())))
}

async fn restart_update() -> Result<impl IntoResponse> {
    let plan = PENDING_RESTART
        .lock()
        .map_err(|_| AppError::Internal("读取重启计划失败".to_string()))?
        .take()
        .ok_or_else(|| AppError::Validation("当前没有待重启的升级任务".to_string()))?;

    if let Err(message) = crate::restart::request(plan.clone()) {
        store_pending_restart(plan)?;
        return Err(AppError::Validation(message));
    }
    finish_update_progress("服务正在重启，请稍后刷新页面", "restarting");

    Ok(Json(Response::ok(UpdateRestartResponse {
        success: true,
        restart_scheduled: true,
        message: "服务正在重启，请稍后刷新页面".to_string(),
    })))
}

async fn apply_update_inner(target_tag: Option<String>) -> Result<UpdateApplyResponse> {
    let release = match target_tag {
        Some(ref tag) => fetch_release_by_tag(tag).await?,
        None => fetch_latest_release().await?,
    };
    set_update_progress(5, "checking", "正在校验版本信息");
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let target_version = normalize_version(&release.tag_name);
    if target_version == current_version {
        return Err(AppError::Validation(format!(
            "当前已经是 {}",
            release.tag_name
        )));
    }
    if target_tag.is_none() && !is_newer_version(&target_version, &current_version) {
        return Err(AppError::Validation("当前已是最新版本".to_string()));
    }

    let asset = find_asset(&release.assets, "linux-x86_64.tar.gz")
        .ok_or_else(|| AppError::NotFound("Release 中未找到 Linux x86_64 二进制包".to_string()))?;
    let checksum_asset = find_asset(&release.assets, "linux-x86_64.tar.gz.sha256")
        .ok_or_else(|| AppError::NotFound("Release 中未找到 SHA256 校验文件".to_string()))?;
    // 签名文件（minisign 分离签名）。配置了公钥时它是**必需**的：
    // 只有 SHA-256 的话，能篡改 Release 的攻击者可以同时替换载荷与校验和，
    // 而载荷会被解包并覆盖运行中的二进制。
    let signature_asset = find_asset(&release.assets, "linux-x86_64.tar.gz.minisig");
    if signature::signature_verification_enabled() && signature_asset.is_none() {
        return Err(AppError::Validation(
            "已配置 SELF_UPDATE_PUBLIC_KEY，但该 Release 缺少 .minisig 签名文件；             拒绝在无法验证真实性时安装升级包"
                .to_string(),
        ));
    }
    if !signature::signature_verification_enabled() {
        tracing::warn!(
            "自更新仅校验 SHA-256：校验和与载荷来自同一个 Release，因此无法证明真实性。             建议设置 SELF_UPDATE_PUBLIC_KEY（最好在编译期固化）以启用分离签名校验，             详见 docs/docker-online-update.md"
        );
    }
    let current_exe = std::env::current_exe()
        .map_err(|e| AppError::Internal(format!("无法定位当前二进制: {}", e)))?;
    let target_static_dir = crate::utils::static_dir();
    let restart_plan = RestartPlan::for_executable(&current_exe);
    let backup_path = backup_path(&current_exe);
    let work_dir = std::env::temp_dir().join(format!(
        "my-media-sub-update-{}-{}",
        target_version,
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&work_dir)
        .await
        .map_err(|e| AppError::Internal(format!("创建升级临时目录失败: {}", e)))?;

    let install_result = download_and_install_release(
        &asset,
        &checksum_asset,
        signature_asset.as_ref(),
        &work_dir,
        &current_exe,
        &target_static_dir,
        &backup_path,
    )
    .await;
    if install_result.is_ok() {
        set_update_progress(97, "cleanup", "正在清理升级临时文件");
    }
    let _ = tokio::fs::remove_dir_all(&work_dir).await;
    install_result?;

    record_runtime_version(&current_exe, &target_version).await;
    prune_update_backups(&current_exe, &target_static_dir).await;
    store_pending_restart(restart_plan)?;
    finish_update_progress("升级完成，请点击按钮重启服务并刷新页面", "restart_required");

    Ok(UpdateApplyResponse {
        success: true,
        previous_version: current_version,
        new_version: target_version,
        binary_path: current_exe.display().to_string(),
        backup_path: backup_path.display().to_string(),
        restart_required: true,
        auto_restart_scheduled: false,
        message: format!("已切换到 {}，请重启服务后生效", release.tag_name),
    })
}

async fn download_and_install_release(
    asset: &GithubAsset,
    checksum_asset: &GithubAsset,
    signature_asset: Option<&GithubAsset>,
    work_dir: &Path,
    current_exe: &Path,
    target_static_dir: &Path,
    backup_path: &Path,
) -> Result<()> {
    let archive_path = work_dir.join(&asset.name);
    set_update_progress(8, "checksum", "正在下载校验文件");
    let checksum_content = download_asset_bytes(&checksum_asset.browser_download_url).await?;
    // 签名文件在下载载荷**之前**取，这样签名缺失/非法时不必浪费一次大文件下载。
    let signature_content = match signature_asset {
        Some(asset) => Some(download_asset_bytes(&asset.browser_download_url).await?),
        None => None,
    };
    download_asset(&asset.browser_download_url, &archive_path, asset.size).await?;
    set_update_progress(69, "checksum", "正在校验升级包 SHA256");
    verify_sha256(&archive_path, &asset.name, &checksum_content).await?;
    // 完整性之后再校验真实性。顺序是刻意的：先确保字节完整（能给出「下载损坏」
    // 这类可操作的错误），再确认它确实出自发布方。
    if let Some(signature_bytes) = signature_content {
        set_update_progress(69, "signature", "正在校验升级包签名");
        let signature_text = String::from_utf8_lossy(&signature_bytes);
        let archive_bytes = tokio::fs::read(&archive_path)
            .await
            .map_err(|error| AppError::Internal(format!("读取升级包以校验签名失败: {error}")))?;
        signature::verify_minisign(&archive_bytes, &signature_text)
            .map_err(|error| AppError::Validation(format!("升级包签名校验未通过: {error}")))?;
        tracing::info!("自更新产物签名校验通过（minisign / Ed25519）");
    }
    set_update_progress(70, "extracting", "正在解压升级包");
    extract_archive(&archive_path, work_dir).await?;
    set_update_progress(82, "locating", "正在检查升级包内容");
    let new_binary = find_binary(work_dir)
        .ok_or_else(|| AppError::Internal("升级包中未找到 my-media-sub 二进制".to_string()))?;
    let new_static_dir = find_static_dir(work_dir)
        .ok_or_else(|| AppError::Internal("升级包中未找到完整 static 目录".to_string()))?;
    set_update_progress(86, "assets", "正在暂存二进制和 WebUI 静态资源");
    replace_update_payload(
        &new_binary,
        &new_static_dir,
        current_exe,
        target_static_dir,
        backup_path,
    )
    .await?;
    Ok(())
}

/// 升级包体积硬上限（期望大小的 3 倍，兼顾压缩包与异常大文件的容差）。
const MAX_UPDATE_PACKAGE_BYTES: u64 = 3;

fn backup_path(current_exe: &Path) -> PathBuf {
    let file_name = current_exe
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("my-media-sub");
    current_exe.with_file_name(format!(
        "{}.bak-{}-{}",
        file_name,
        Utc::now().format("%Y%m%d%H%M%S"),
        uuid::Uuid::new_v4()
    ))
}

async fn replace_update_payload(
    new_binary: &Path,
    new_static_dir: &Path,
    current_exe: &Path,
    target_static_dir: &Path,
    backup_path: &Path,
) -> Result<()> {
    let new_binary = new_binary.to_path_buf();
    let new_static_dir = new_static_dir.to_path_buf();
    let current_exe = current_exe.to_path_buf();
    let target_static_dir = target_static_dir.to_path_buf();
    let backup_path = backup_path.to_path_buf();
    set_update_progress(90, "replacing", "正在提交二进制和 WebUI 静态资源升级事务");
    tokio::task::spawn_blocking(move || {
        replace_update_payload_blocking(
            &new_binary,
            &new_static_dir,
            &current_exe,
            &target_static_dir,
            &backup_path,
        )
    })
    .await
    .map_err(|e| AppError::Internal(format!("安装升级文件任务失败: {}", e)))?
}

fn replace_update_payload_blocking(
    new_binary: &Path,
    new_static_dir: &Path,
    current_exe: &Path,
    target_static_dir: &Path,
    backup_path: &Path,
) -> Result<()> {
    if !current_exe.is_file() {
        return Err(AppError::Internal(format!(
            "当前二进制不存在: {}",
            current_exe.display()
        )));
    }
    if !new_binary.is_file()
        || std::fs::symlink_metadata(new_binary)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(true)
    {
        return Err(AppError::Internal(
            "升级包中的 my-media-sub 不是普通文件".to_string(),
        ));
    }
    if !static_payload_is_complete(new_static_dir) {
        return Err(AppError::Internal(
            "升级包中的 static 目录不完整".to_string(),
        ));
    }

    let binary_parent = current_exe
        .parent()
        .ok_or_else(|| AppError::Internal("无法定位二进制所在目录".to_string()))?;
    let static_parent = target_static_dir
        .parent()
        .ok_or_else(|| AppError::Internal("无法定位静态资源所在目录".to_string()))?;
    std::fs::create_dir_all(static_parent)
        .map_err(|e| AppError::Internal(format!("创建静态资源父目录失败: {}", e)))?;

    let token = uuid::Uuid::new_v4();
    let binary_name = current_exe
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("my-media-sub");
    let static_name = target_static_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("static");
    let binary_stage = binary_parent.join(format!(".{binary_name}.new-{token}"));
    let static_stage = static_parent.join(format!(".{static_name}.new-{token}"));
    let static_backup = static_parent.join(format!(
        "{static_name}.bak-{}-{token}",
        Utc::now().format("%Y%m%d%H%M%S")
    ));

    let install_result = (|| -> Result<()> {
        std::fs::copy(current_exe, backup_path)
            .map_err(|e| AppError::Internal(format!("备份当前二进制失败: {}", e)))?;
        let current_metadata = std::fs::metadata(current_exe)
            .map_err(|e| AppError::Internal(format!("读取当前二进制权限失败: {}", e)))?;
        std::fs::copy(new_binary, &binary_stage)
            .map_err(|e| AppError::Internal(format!("暂存新二进制失败: {}", e)))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &binary_stage,
                std::fs::Permissions::from_mode(current_metadata.permissions().mode()),
            )
            .map_err(|e| AppError::Internal(format!("设置新二进制权限失败: {}", e)))?;
        }

        std::fs::File::open(&binary_stage)
            .and_then(|file| file.sync_all())
            .map_err(|e| AppError::Internal(format!("同步新二进制失败: {}", e)))?;
        copy_dir_all(new_static_dir, &static_stage)?;

        let had_static = target_static_dir.exists();
        if had_static {
            std::fs::rename(target_static_dir, &static_backup)
                .map_err(|e| AppError::Internal(format!("备份静态资源失败: {}", e)))?;
        }
        if let Err(error) = std::fs::rename(&static_stage, target_static_dir) {
            let rollback_error = if had_static {
                std::fs::rename(&static_backup, target_static_dir).err()
            } else {
                None
            };
            return Err(AppError::Internal(match rollback_error {
                Some(rollback_error) => {
                    format!("切换静态资源失败: {error}；恢复旧静态资源也失败: {rollback_error}")
                }
                None => format!("切换静态资源失败: {error}"),
            }));
        }

        if let Err(error) = std::fs::rename(&binary_stage, current_exe) {
            let _ = std::fs::remove_dir_all(target_static_dir);
            let rollback_error = if had_static {
                std::fs::rename(&static_backup, target_static_dir).err()
            } else {
                None
            };
            return Err(AppError::Internal(match rollback_error {
                Some(rollback_error) => {
                    format!("替换当前二进制失败: {error}；恢复旧静态资源也失败: {rollback_error}")
                }
                None => format!("替换当前二进制失败: {error}"),
            }));
        }

        if let Err(error) = sync_directory(binary_parent) {
            tracing::warn!("{}", error);
        }
        if static_parent != binary_parent {
            if let Err(error) = sync_directory(static_parent) {
                tracing::warn!("{}", error);
            }
        }
        Ok(())
    })();

    let _ = std::fs::remove_file(&binary_stage);
    let _ = std::fs::remove_dir_all(&static_stage);
    if install_result.is_err() {
        let _ = std::fs::remove_file(backup_path);
    }
    install_result
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| AppError::Internal(format!("同步升级目录失败: {}", e)))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn find_binary(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = std::fs::read_dir(path).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == "my-media-sub")
                .unwrap_or(false)
            {
                return Some(path);
            }
        }
    }
    None
}

fn find_static_dir(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = std::fs::read_dir(path).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == "static")
                .unwrap_or(false)
                && static_payload_is_complete(&path)
            {
                return Some(path);
            }
            stack.push(path);
        }
    }
    None
}

fn static_payload_is_complete(path: &Path) -> bool {
    path.is_dir()
        && REQUIRED_STATIC_ASSETS
            .iter()
            .all(|asset| path.join(asset).is_file())
}

fn copy_dir_all(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)
        .map_err(|e| AppError::Internal(format!("创建静态资源目录失败: {}", e)))?;

    for entry in std::fs::read_dir(source)
        .map_err(|e| AppError::Internal(format!("读取静态资源目录失败: {}", e)))?
    {
        let entry = entry.map_err(|e| AppError::Internal(format!("读取静态资源项失败: {}", e)))?;
        let file_type = entry
            .file_type()
            .map_err(|e| AppError::Internal(format!("读取静态资源类型失败: {}", e)))?;
        if file_type.is_symlink() {
            return Err(AppError::Validation(
                "升级包中的 static 目录不能包含符号链接".to_string(),
            ));
        }
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&source_path, &target_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&source_path, &target_path)
                .map_err(|e| AppError::Internal(format!("复制静态资源失败: {}", e)))?;
        } else {
            return Err(AppError::Validation(
                "升级包中的 static 目录包含不支持的文件类型".to_string(),
            ));
        }
    }

    Ok(())
}

async fn record_runtime_version(current_exe: &Path, version: &str) {
    let Some(runtime_dir) = managed_runtime_dir() else {
        return;
    };
    if !path_is_within(current_exe, &runtime_dir) {
        return;
    }

    let marker = runtime_dir.join(".installed-version");
    let content = format!("{}\n", version);
    let result = tokio::task::spawn_blocking(move || {
        crate::utils::write_file_atomic(&marker, content.as_bytes(), 0o644)
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("记录运行时版本失败: {}", error),
        Err(error) => tracing::warn!("记录运行时版本任务失败: {}", error),
    }
}

async fn prune_update_backups(current_exe: &Path, target_static_dir: &Path) {
    let retention = std::env::var("SELF_UPDATE_BACKUP_RETENTION")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(3)
        .clamp(1, 20);
    let current_exe = current_exe.to_path_buf();
    let target_static_dir = target_static_dir.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        prune_sibling_backups(&current_exe, retention)?;
        prune_sibling_backups(&target_static_dir, retention)
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("清理旧升级备份失败: {}", error),
        Err(error) => tracing::warn!("清理旧升级备份任务失败: {}", error),
    }
}

fn prune_sibling_backups(target: &Path, retention: usize) -> Result<()> {
    let Some(parent) = target.parent() else {
        return Ok(());
    };
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let prefix = format!("{name}.bak-");
    let mut backups = std::fs::read_dir(parent)
        .map_err(|e| AppError::Internal(format!("读取升级备份目录失败: {}", e)))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    backups.sort_by(|left, right| right.file_name().cmp(&left.file_name()));

    for backup in backups.into_iter().skip(retention) {
        let metadata = std::fs::symlink_metadata(&backup)
            .map_err(|e| AppError::Internal(format!("读取升级备份失败: {}", e)))?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(&backup)
                .map_err(|e| AppError::Internal(format!("删除旧静态资源备份失败: {}", e)))?;
        } else {
            std::fs::remove_file(&backup)
                .map_err(|e| AppError::Internal(format!("删除旧二进制备份失败: {}", e)))?;
        }
    }
    Ok(())
}

fn normalize_version(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string()
}

fn find_asset(assets: &[GithubAsset], suffix: &str) -> Option<GithubAsset> {
    assets
        .iter()
        .find(|asset| asset.name.ends_with(suffix))
        .cloned()
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    compare_versions(candidate, current) == Some(Ordering::Greater)
}

fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    if left_parts.is_empty() || right_parts.is_empty() {
        return None;
    }

    for index in 0..left_parts.len().max(right_parts.len()) {
        let left_value = left_parts.get(index).copied().unwrap_or(0);
        let right_value = right_parts.get(index).copied().unwrap_or(0);
        match left_value.cmp(&right_value) {
            Ordering::Equal => {}
            ordering => return Some(ordering),
        }
    }

    Some(Ordering::Equal)
}

fn version_parts(value: &str) -> Vec<u64> {
    normalize_version(value)
        .split(['.', '-', '+'])
        .filter_map(|part| {
            let digits: String = part.chars().take_while(|ch| ch.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse::<u64>().ok()
            }
        })
        .collect()
}

pub fn routes() -> Router {
    Router::new()
        .route("/api/update/check", get(check_update))
        .route("/api/update/releases", get(list_releases))
        .route("/api/update/progress", get(update_progress))
        .route("/api/update/apply", post(apply_update))
        .route("/api/update/restart", post(restart_update))
}

mod github;
mod package;
mod progress;
mod runtime;
mod signature;

// 四个子模块承载原本挤在同一个文件里的职责：进度状态机、发布包下载与校验、
// GitHub 客户端、运行时/部署形态探测。它们都是 `pub(super)`，只在本模块内使用；
// 模块内部的 `use super::*;` 让拆分后的函数继续看到原有的导入。
use github::*;
use package::*;
use progress::*;
use runtime::*;

#[cfg(test)]
mod tests;
