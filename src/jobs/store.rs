use std::path::PathBuf;

use tokio::sync::{broadcast, RwLock};

use crate::error::{AppError, Result};

use super::model::{now, Job};

const MAX_JOBS: usize = 500;

pub struct JobStore {
    path: PathBuf,
    jobs: RwLock<Vec<Job>>,
    events: broadcast::Sender<Job>,
}

impl JobStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            jobs: RwLock::new(Vec::new()),
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
        *jobs = serde_json::from_str(&content)
            .map_err(|e| AppError::Database(format!("解析任务 JSON 失败: {}", e)))?;
        Ok(())
    }

    pub async fn add(&self, job: Job) -> Result<Job> {
        let mut jobs = self.jobs.write().await;
        jobs.push(job.clone());
        self.save_locked(&jobs).await?;
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
        let mut jobs = self.jobs.write().await;
        let updated = if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
            updater(job)?;
            job.updated_at = now();
            Some(job.clone())
        } else {
            None
        };

        if updated.is_some() {
            self.save_locked(&jobs).await?;
        }

        if let Some(job) = &updated {
            self.emit(job.clone());
        }

        Ok(updated)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Job> {
        self.events.subscribe()
    }

    fn emit(&self, job: Job) {
        let _ = self.events.send(job);
    }

    async fn save_locked(&self, jobs: &[Job]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Database(format!("创建任务目录失败: {}", e)))?;
        }

        let slice = if jobs.len() > MAX_JOBS {
            &jobs[jobs.len() - MAX_JOBS..]
        } else {
            jobs
        };
        let content = serde_json::to_string_pretty(slice)
            .map_err(|e| AppError::Database(format!("序列化任务失败: {}", e)))?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, content)
            .map_err(|e| AppError::Database(format!("写入任务临时文件失败: {}", e)))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| AppError::Database(format!("重命名任务临时文件失败: {}", e)))?;

        Ok(())
    }
}
