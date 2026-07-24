use std::collections::HashMap;
use std::path::PathBuf;

use tokio::sync::{broadcast, Mutex, RwLock};

use crate::error::{AppError, Result};
use crate::store::schema::{
    backup_store_before_migration, decode_store_json, write_versioned_json_atomic_async, StoreKind,
    StoreSchemaError,
};
use crate::utils::{quarantine_corrupt_file, set_file_mode};

use super::model::{now, Job, JobStatus};

const MAX_JOBS: usize = 500;
const MAX_ARCHIVED_JOBS: usize = 5_000;

pub struct JobStore {
    path: PathBuf,
    archive_path: PathBuf,
    jobs: RwLock<Vec<Job>>,
    id_index: RwLock<HashMap<String, usize>>,
    save_lock: Mutex<()>,
    events: broadcast::Sender<Job>,
}

impl JobStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let archive_path = path.with_file_name(format!(
            "{}.archive.json",
            path.file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("jobs")
        ));
        Self {
            path,
            archive_path,
            jobs: RwLock::new(Vec::new()),
            id_index: RwLock::new(HashMap::new()),
            save_lock: Mutex::new(()),
            events: broadcast::channel(200).0,
        }
    }

    pub async fn load(&self) -> Result<()> {
        if !self.path.exists() {
            self.replace_memory(Vec::new()).await;
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| AppError::Database(format!("读取任务文件失败: {}", e)))?;
        set_file_mode(&self.path, 0o600)?;
        match decode_store_json::<Vec<Job>>(&content, StoreKind::Jobs) {
            Ok(decoded) => {
                backup_store_before_migration(&self.path, &content, decoded.source_version)?;
                let mut parsed = decoded.data;
                let original_len = parsed.len();
                truncate_jobs(&mut parsed);
                if decoded.needs_write || parsed.len() != original_len {
                    write_versioned_json_atomic_async(&self.path, &parsed, 0o600).await?;
                }
                self.replace_memory(parsed).await;
            }
            Err(StoreSchemaError::UnsupportedVersion { found, current }) => {
                return Err(AppError::Database(format!(
                    "任务存储 schema 版本 {} 高于当前支持版本 {}，请升级程序后重试",
                    found, current
                )));
            }
            Err(error) => {
                tracing::warn!("解析任务 JSON 失败，已隔离损坏文件并使用空任务: {}", error);
                quarantine_corrupt_file(&self.path);
                self.replace_memory(Vec::new()).await;
            }
        }
        Ok(())
    }

    pub async fn add(&self, job: Job) -> Result<Job> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let jobs = self.jobs.read().await;
            let mut snapshot = jobs.clone();
            snapshot.push(job.clone());
            truncate_jobs(&mut snapshot);
            snapshot
        };
        self.save_snapshot(&snapshot).await?;
        self.replace_memory(snapshot).await;
        self.emit(job.clone());
        Ok(job)
    }

    /// 添加任务；若存在相同幂等键的排队或运行任务，则返回已有任务。
    pub async fn add_idempotent(&self, job: Job) -> Result<(Job, bool)> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let jobs = self.jobs.read().await;
            if let Some(key) = job.idempotency_key.as_deref() {
                if let Some(existing) = jobs.iter().rev().find(|existing| {
                    existing.idempotency_key.as_deref() == Some(key)
                        && matches!(
                            existing.status,
                            super::model::JobStatus::Queued | super::model::JobStatus::Running
                        )
                }) {
                    return Ok((existing.clone(), false));
                }
            }
            let mut snapshot = jobs.clone();
            snapshot.push(job.clone());
            truncate_jobs(&mut snapshot);
            snapshot
        };
        self.save_snapshot(&snapshot).await?;
        self.replace_memory(snapshot).await;
        self.emit(job.clone());
        Ok((job, true))
    }

    pub async fn get(&self, id: &str) -> Option<Job> {
        let jobs = self.jobs.read().await;
        let indexes = self.id_index.read().await;
        let index = indexes.get(id).copied()?;
        jobs.get(index).cloned()
    }

    pub async fn list(&self) -> Vec<Job> {
        let jobs = self.jobs.read().await;
        jobs.iter().rev().cloned().collect()
    }

    pub async fn list_paginated(&self, offset: usize, limit: usize) -> Vec<Job> {
        let jobs = self.jobs.read().await;
        jobs.iter()
            .rev()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    pub async fn update<F>(&self, id: &str, updater: F) -> Result<Option<Job>>
    where
        F: FnOnce(&mut Job),
    {
        self.try_update(id, |job| {
            updater(job);
            Ok(())
        })
        .await
    }

    pub async fn try_update<F>(&self, id: &str, updater: F) -> Result<Option<Job>>
    where
        F: FnOnce(&mut Job) -> Result<()>,
    {
        let _save_guard = self.save_lock.lock().await;
        let updated = {
            let jobs = self.jobs.read().await;
            let mut snapshot = jobs.clone();
            if let Some(job) = snapshot.iter_mut().find(|job| job.id == id) {
                updater(job)?;
                job.updated_at = now();
                Some((job.clone(), snapshot))
            } else {
                None
            }
        };

        if let Some((_, snapshot)) = &updated {
            self.save_snapshot(snapshot).await?;
            self.replace_memory(snapshot.clone()).await;
        }

        if let Some((job, _)) = &updated {
            self.emit(job.clone());
        }

        Ok(updated.map(|(job, _)| job))
    }

    /// 仅在调用方谓词接受当前状态时更新。
    ///
    /// 返回 `None` 表示任务不存在或状态已不满足条件；拒绝更新时不会落盘、更新时间
    /// 或发送事件，适合 Worker 原子认领排队任务。
    pub async fn update_if<F>(&self, id: &str, updater: F) -> Result<Option<Job>>
    where
        F: FnOnce(&mut Job) -> bool,
    {
        let _save_guard = self.save_lock.lock().await;
        let updated = {
            let jobs = self.jobs.read().await;
            let mut snapshot = jobs.clone();
            let Some(job) = snapshot.iter_mut().find(|job| job.id == id) else {
                return Ok(None);
            };
            if !updater(job) {
                return Ok(None);
            }
            job.updated_at = now();
            (job.clone(), snapshot)
        };

        self.save_snapshot(&updated.1).await?;
        self.replace_memory(updated.1).await;
        self.emit(updated.0.clone());
        Ok(Some(updated.0))
    }

    pub async fn compact(&self) -> Result<usize> {
        let _guard = self.save_lock.lock().await;
        let mut snapshot = self.jobs.read().await.clone();
        let before = snapshot.len();
        truncate_jobs(&mut snapshot);
        self.save_snapshot(&snapshot).await?;
        let removed = before.saturating_sub(snapshot.len());
        self.replace_memory(snapshot).await;
        Ok(removed)
    }

    pub async fn archive_completed(&self, retain: usize) -> Result<usize> {
        let _guard = self.save_lock.lock().await;
        let current = self.jobs.read().await.clone();
        let terminal_count = current.iter().filter(|job| is_terminal(job)).count();
        let move_count = terminal_count.saturating_sub(retain);
        if move_count == 0 {
            return Ok(0);
        }
        let move_ids = current
            .iter()
            .filter(|job| is_terminal(job))
            .take(move_count)
            .map(|job| job.id.clone())
            .collect::<std::collections::HashSet<_>>();
        let (mut archived_now, active): (Vec<_>, Vec<_>) = current
            .into_iter()
            .partition(|job| move_ids.contains(&job.id));
        let mut archive = self.read_archive().await?;
        archive.append(&mut archived_now);
        let archive_retention = configured_archive_retention();
        if archive.len() > archive_retention {
            archive.drain(0..archive.len() - archive_retention);
        }
        write_versioned_json_atomic_async(&self.archive_path, &archive, 0o600).await?;
        self.save_snapshot(&active).await?;
        self.replace_memory(active).await;
        Ok(move_count)
    }

    pub async fn list_archived(&self, offset: usize, limit: usize) -> Result<Vec<Job>> {
        let archive = self.read_archive().await?;
        Ok(archive.into_iter().rev().skip(offset).take(limit).collect())
    }

    pub async fn terminal_count(&self) -> usize {
        self.jobs
            .read()
            .await
            .iter()
            .filter(|job| is_terminal(job))
            .count()
    }

    pub async fn prune_archive(&self, retain: usize) -> Result<usize> {
        let _guard = self.save_lock.lock().await;
        let mut archive = self.read_archive().await?;
        let before = archive.len();
        if archive.len() > retain {
            archive.drain(0..archive.len() - retain);
            write_versioned_json_atomic_async(&self.archive_path, &archive, 0o600).await?;
        }
        Ok(before.saturating_sub(archive.len()))
    }

    pub async fn archived_count(&self) -> Result<usize> {
        Ok(self.read_archive().await?.len())
    }

    /// 在阻塞线程池读取并解析归档文件，避免同步 IO 卡住 tokio worker。
    async fn read_archive(&self) -> Result<Vec<Job>> {
        let path = self.archive_path.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<Job>> {
            if !path.exists() {
                return Ok(Vec::new());
            }
            let content = std::fs::read_to_string(&path)
                .map_err(|error| AppError::Database(format!("读取任务归档失败: {error}")))?;
            set_file_mode(&path, 0o600)?;
            decode_store_json::<Vec<Job>>(&content, StoreKind::Jobs)
                .map(|decoded| decoded.data)
                .map_err(|error| AppError::Database(format!("解析任务归档失败: {error}")))
        })
        .await
        .map_err(|error| AppError::Database(format!("读取任务归档线程失败: {error}")))?
    }

    async fn replace_memory(&self, jobs: Vec<Job>) {
        let index = build_job_index(&jobs);
        let mut current_jobs = self.jobs.write().await;
        let mut current_index = self.id_index.write().await;
        *current_jobs = jobs;
        *current_index = index;
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Job> {
        self.events.subscribe()
    }

    fn emit(&self, job: Job) {
        let _ = self.events.send(job);
    }

    async fn save_snapshot(&self, jobs: &[Job]) -> Result<()> {
        write_versioned_json_atomic_async(&self.path, &jobs, 0o600).await
    }
}

fn is_terminal(job: &Job) -> bool {
    matches!(
        job.status,
        JobStatus::Succeeded | JobStatus::Failed | JobStatus::Canceled
    )
}

fn build_job_index(jobs: &[Job]) -> HashMap<String, usize> {
    jobs.iter()
        .enumerate()
        .map(|(index, job)| (job.id.clone(), index))
        .collect()
}

/// 超出容量时仅淘汰终态任务（成功/失败/取消），从最旧开始；绝不淘汰排队或
/// 运行中的任务。若终态任务不足以降到容量以内，则允许暂时超出容量。
fn truncate_jobs(jobs: &mut Vec<Job>) {
    if jobs.len() <= MAX_JOBS {
        return;
    }
    let mut excess = jobs.len() - MAX_JOBS;
    jobs.retain(|job| {
        if excess > 0 && is_terminal(job) {
            excess -= 1;
            false
        } else {
            true
        }
    });
}

fn configured_archive_retention() -> usize {
    std::env::var("RETENTION_ARCHIVED_JOBS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(MAX_ARCHIVED_JOBS)
        .min(MAX_ARCHIVED_JOBS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobKind, JobPriority, JobStatus};
    use crate::store::schema::migration_backup_path;
    use serde_json::json;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "my-media-sub-{}-{}.json",
            name,
            uuid::Uuid::new_v4()
        ))
    }

    fn make_job(id: &str) -> Job {
        Job {
            id: id.to_string(),
            kind: JobKind::ManualTransfer,
            request_id: None,
            correlation_id: None,
            subscription_id: None,
            priority: JobPriority::Normal,
            attempt: 1,
            next_attempt_at: None,
            error_class: None,
            status: JobStatus::Queued,
            progress: 0,
            title: "测试任务".to_string(),
            message: "queued".to_string(),
            idempotency_key: None,
            payload: json!({"url": "https://pan.quark.cn/s/test"}),
            result: None,
            error: None,
            created_at: 1,
            updated_at: 1,
            started_at: None,
            finished_at: None,
        }
    }

    #[cfg(unix)]
    fn assert_private_file_mode(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;

        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(not(unix))]
    fn assert_private_file_mode(_path: &std::path::Path) {}

    fn quarantine_path(path: &std::path::Path) -> Option<PathBuf> {
        let file_name = path.file_name()?.to_string_lossy();
        std::fs::read_dir(path.parent()?)
            .ok()?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|candidate| {
                candidate.file_name().is_some_and(|name| {
                    name.to_string_lossy()
                        .starts_with(&format!("{}.corrupt-", file_name))
                })
            })
    }

    #[tokio::test]
    async fn load_migrates_legacy_jobs_to_envelope() {
        let tmp = temp_path("jobs-legacy");
        let original = serde_json::to_vec_pretty(&vec![make_job("legacy")]).unwrap();
        std::fs::write(&tmp, &original).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        let store = JobStore::new(&tmp);
        store.load().await.unwrap();

        assert_eq!(store.list().await[0].id, "legacy");
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&tmp).unwrap()).unwrap();
        assert_eq!(persisted["schema_version"], 1);
        assert_eq!(persisted["data"][0]["id"], "legacy");
        assert_private_file_mode(&tmp);

        let backup = migration_backup_path(&tmp, 0);
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        assert_private_file_mode(&backup);

        let _ = std::fs::remove_file(tmp);
        let _ = std::fs::remove_file(backup);
    }

    #[tokio::test]
    async fn future_job_schema_is_preserved() {
        let tmp = temp_path("jobs-future");
        let original = json!({"schema_version": 99, "data": []}).to_string();
        std::fs::write(&tmp, &original).unwrap();

        let store = JobStore::new(&tmp);
        let error = store.load().await.unwrap_err();

        assert!(matches!(error, AppError::Database(_)));
        assert_eq!(std::fs::read_to_string(&tmp).unwrap(), original);
        assert!(quarantine_path(&tmp).is_none());
        assert_private_file_mode(&tmp);

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn load_quarantines_corrupt_job_file() {
        let tmp = temp_path("jobs-corrupt");
        std::fs::write(&tmp, b"{not-valid-json").unwrap();

        let store = JobStore::new(&tmp);
        store.load().await.unwrap();

        assert!(store.list().await.is_empty());
        assert!(!tmp.exists());
        let quarantined = quarantine_path(&tmp).expect("corrupt job file was not quarantined");
        let _ = std::fs::remove_file(quarantined);
    }

    #[tokio::test]
    async fn add_keeps_jobs_unchanged_when_save_fails() {
        let blocker = temp_path("jobs-save-blocker");
        std::fs::write(&blocker, b"not-a-directory").unwrap();
        let path = blocker.join("jobs.json");
        let store = JobStore::new(&path);
        store.load().await.unwrap();

        let result = store.add(make_job("should-not-stick")).await;

        assert!(matches!(result, Err(AppError::Database(_))));
        assert!(store.list().await.is_empty());

        let _ = std::fs::remove_file(blocker);
    }

    #[tokio::test]
    async fn add_idempotent_returns_existing_active_job() {
        let tmp = temp_path("jobs-idempotent");
        let store = JobStore::new(&tmp);
        store.load().await.unwrap();

        let mut first = make_job("first");
        first.idempotency_key = Some("same-work".to_string());
        let (created, was_created) = store.add_idempotent(first).await.unwrap();
        assert!(was_created);
        assert_eq!(created.id, "first");

        let mut duplicate = make_job("duplicate");
        duplicate.idempotency_key = Some("same-work".to_string());
        let (existing, was_created) = store.add_idempotent(duplicate).await.unwrap();

        assert!(!was_created);
        assert_eq!(existing.id, "first");
        assert_eq!(store.list().await.len(), 1);

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn update_if_rejects_without_mutating_or_emitting() {
        let tmp = temp_path("jobs-conditional-update");
        let store = JobStore::new(&tmp);
        store.load().await.unwrap();
        store.add(make_job("conditional")).await.unwrap();
        let mut events = store.subscribe();

        let updated = store
            .update_if("conditional", |job| {
                job.message = "should-not-stick".to_string();
                false
            })
            .await
            .unwrap();

        assert!(updated.is_none());
        assert_eq!(store.get("conditional").await.unwrap().message, "queued");
        assert!(events.try_recv().is_err());

        let _ = std::fs::remove_file(tmp);
    }

    #[test]
    fn truncate_evicts_only_terminal_jobs_oldest_first() {
        let mut jobs = Vec::new();
        for index in 0..(MAX_JOBS + 10) {
            let mut job = make_job(&format!("job-{index}"));
            job.status = if index % 2 == 0 {
                JobStatus::Succeeded
            } else {
                JobStatus::Queued
            };
            jobs.push(job);
        }

        truncate_jobs(&mut jobs);

        assert_eq!(jobs.len(), MAX_JOBS);
        // 被淘汰的是最旧的 10 个终态任务（偶数下标 0..=18）。
        assert!(jobs
            .iter()
            .all(|job| job.id != "job-0" && job.id != "job-18"));
        assert!(jobs.iter().any(|job| job.id == "job-20"));
        // 所有排队任务原样保留。
        assert_eq!(
            jobs.iter()
                .filter(|job| job.status == JobStatus::Queued)
                .count(),
            (MAX_JOBS + 10) / 2
        );
    }

    #[test]
    fn truncate_never_evicts_live_jobs_even_over_cap() {
        let mut jobs = Vec::new();
        for index in 0..(MAX_JOBS + 5) {
            let mut job = make_job(&format!("live-{index}"));
            job.status = if index % 2 == 0 {
                JobStatus::Queued
            } else {
                JobStatus::Running
            };
            jobs.push(job);
        }

        truncate_jobs(&mut jobs);

        // 没有终态任务可淘汰时允许超出容量。
        assert_eq!(jobs.len(), MAX_JOBS + 5);
    }

    #[tokio::test]
    async fn archive_completed_moves_only_old_terminal_jobs() {
        let tmp = temp_path("jobs-archive");
        let store = JobStore::new(&tmp);
        store.load().await.unwrap();
        for index in 0..4 {
            let mut job = make_job(&format!("done-{index}"));
            job.status = JobStatus::Succeeded;
            job.created_at = index;
            store.add(job).await.unwrap();
        }
        store.add(make_job("queued-kept")).await.unwrap();

        assert_eq!(store.archive_completed(2).await.unwrap(), 2);
        let active = store.list().await;
        assert_eq!(active.len(), 3);
        assert!(active.iter().any(|job| job.id == "queued-kept"));
        let archived = store.list_archived(0, 10).await.unwrap();
        assert_eq!(archived.len(), 2);
        assert_eq!(store.archived_count().await.unwrap(), 2);

        let archive_path = tmp.with_file_name(format!(
            "{}.archive.json",
            tmp.file_stem().unwrap().to_string_lossy()
        ));
        let _ = std::fs::remove_file(tmp);
        let _ = std::fs::remove_file(archive_path);
    }
}
