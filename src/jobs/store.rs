use std::collections::HashMap;
use std::path::PathBuf;

use tokio::sync::{broadcast, Mutex, RwLock};

use crate::error::{AppError, Result};
use crate::store::schema::{
    backup_store_before_migration, decode_store_json, write_versioned_json_atomic_async, StoreKind,
    StoreSchemaError,
};
use crate::utils::{quarantine_corrupt_file, set_file_mode};

use super::model::{now, Job};

const MAX_JOBS: usize = 500;

pub struct JobStore {
    path: PathBuf,
    jobs: RwLock<Vec<Job>>,
    id_index: RwLock<HashMap<String, usize>>,
    save_lock: Mutex<()>,
    events: broadcast::Sender<Job>,
}

impl JobStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
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

fn build_job_index(jobs: &[Job]) -> HashMap<String, usize> {
    jobs.iter()
        .enumerate()
        .map(|(index, job)| (job.id.clone(), index))
        .collect()
}

fn truncate_jobs(jobs: &mut Vec<Job>) {
    if jobs.len() > MAX_JOBS {
        let remove_count = jobs.len() - MAX_JOBS;
        jobs.drain(0..remove_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobKind, JobStatus};
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
}
