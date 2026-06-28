use std::path::PathBuf;

use tokio::sync::{broadcast, Mutex, RwLock};

use crate::error::{AppError, Result};
use crate::utils::{quarantine_corrupt_file, write_json_atomic_async};

use super::model::{now, Job};

const MAX_JOBS: usize = 500;

pub struct JobStore {
    path: PathBuf,
    jobs: RwLock<Vec<Job>>,
    save_lock: Mutex<()>,
    events: broadcast::Sender<Job>,
}

impl JobStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            jobs: RwLock::new(Vec::new()),
            save_lock: Mutex::new(()),
            events: broadcast::channel(200).0,
        }
    }

    pub async fn load(&self) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if !self.path.exists() {
            *jobs = Vec::new();
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| AppError::Database(format!("读取任务文件失败: {}", e)))?;
        match serde_json::from_str(&content) {
            Ok(mut parsed) => {
                truncate_jobs(&mut parsed);
                *jobs = parsed;
            }
            Err(e) => {
                tracing::warn!("解析任务 JSON 失败，已隔离损坏文件并使用空任务: {}", e);
                quarantine_corrupt_file(&self.path);
                *jobs = Vec::new();
            }
        }
        Ok(())
    }

    pub async fn add(&self, job: Job) -> Result<Job> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let mut jobs = self.jobs.write().await;
            jobs.push(job.clone());
            truncate_jobs(&mut jobs);
            jobs.clone()
        };
        self.save_snapshot(&snapshot).await?;
        self.emit(job.clone());
        Ok(job)
    }

    pub async fn get(&self, id: &str) -> Option<Job> {
        let jobs = self.jobs.read().await;
        jobs.iter().find(|job| job.id == id).cloned()
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
            let mut jobs = self.jobs.write().await;
            if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
                updater(job)?;
                job.updated_at = now();
                Some((job.clone(), jobs.clone()))
            } else {
                None
            }
        };

        if let Some((_, snapshot)) = &updated {
            self.save_snapshot(snapshot).await?;
        }

        if let Some((job, _)) = &updated {
            self.emit(job.clone());
        }

        Ok(updated.map(|(job, _)| job))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Job> {
        self.events.subscribe()
    }

    fn emit(&self, job: Job) {
        let _ = self.events.send(job);
    }

    async fn save_snapshot(&self, jobs: &[Job]) -> Result<()> {
        write_json_atomic_async(&self.path, &jobs, 0o600).await
    }
}

fn truncate_jobs(jobs: &mut Vec<Job>) {
    if jobs.len() > MAX_JOBS {
        let remove_count = jobs.len() - MAX_JOBS;
        jobs.drain(0..remove_count);
    }
}
