use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, info, warn};

use crate::clients::{QuarkSaveClient, QuarkShareProbe};
use crate::error::{AppError, Result};
use crate::services::SubscriptionTransferService;
use crate::store::{NotificationStore, SettingsStore};

const MAX_JOBS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ManualTransfer,
    SubscriptionTransfer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub progress: u8,
    pub title: String,
    pub message: String,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualTransferPayload {
    pub url: String,
    #[serde(default)]
    pub passcode: String,
    #[serde(default)]
    pub target_fid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionTransferPayload {
    pub subscription_id: String,
    pub file_names: Vec<String>,
}

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
        let mut jobs = self.jobs.write().await;
        let updated = if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
            updater(job);
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

pub struct JobQueue {
    store: Arc<JobStore>,
    sender: mpsc::Sender<String>,
}

impl JobQueue {
    pub fn new(
        store: Arc<JobStore>,
        settings_store: Arc<SettingsStore>,
        notification_store: Arc<NotificationStore>,
        transfer_service: Arc<SubscriptionTransferService>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(100);
        let worker = JobWorker {
            store: store.clone(),
            settings_store,
            notification_store,
            transfer_service,
            receiver,
        };
        tokio::spawn(worker.run());

        Self { store, sender }
    }

    pub async fn submit_manual_transfer(&self, payload: ManualTransferPayload) -> Result<Job> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now();
        let job = Job {
            id: id.clone(),
            kind: JobKind::ManualTransfer,
            status: JobStatus::Queued,
            progress: 0,
            title: "手动转存".to_string(),
            message: "等待后台任务执行".to_string(),
            payload: serde_json::to_value(payload)?,
            result: None,
            error: None,
            created_at,
            updated_at: created_at,
            started_at: None,
            finished_at: None,
        };

        let job = self.store.add(job).await?;
        if self.sender.send(id.clone()).await.is_err() {
            self.store
                .update(&id, |job| {
                    job.status = JobStatus::Failed;
                    job.progress = 100;
                    job.message = "任务队列不可用".to_string();
                    job.error = Some("任务队列不可用".to_string());
                    job.finished_at = Some(now());
                })
                .await?;
            return Err(AppError::Internal("任务队列不可用".to_string()));
        }

        Ok(job)
    }

    pub async fn submit_subscription_transfer(
        &self,
        payload: SubscriptionTransferPayload,
    ) -> Result<Job> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now();
        let job = Job {
            id: id.clone(),
            kind: JobKind::SubscriptionTransfer,
            status: JobStatus::Queued,
            progress: 0,
            title: "订阅自动转存".to_string(),
            message: "等待后台任务执行".to_string(),
            payload: serde_json::to_value(payload)?,
            result: None,
            error: None,
            created_at,
            updated_at: created_at,
            started_at: None,
            finished_at: None,
        };

        let job = self.store.add(job).await?;
        if self.sender.send(id.clone()).await.is_err() {
            self.store
                .update(&id, |job| {
                    job.status = JobStatus::Failed;
                    job.progress = 100;
                    job.message = "任务队列不可用".to_string();
                    job.error = Some("任务队列不可用".to_string());
                    job.finished_at = Some(now());
                })
                .await?;
            return Err(AppError::Internal("任务队列不可用".to_string()));
        }

        Ok(job)
    }
}

struct JobWorker {
    store: Arc<JobStore>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    transfer_service: Arc<SubscriptionTransferService>,
    receiver: mpsc::Receiver<String>,
}

impl JobWorker {
    async fn run(mut self) {
        info!("后台任务 worker 已启动");
        while let Some(job_id) = self.receiver.recv().await {
            if let Err(e) = self.run_job(&job_id).await {
                error!("任务 {} 执行失败: {}", job_id, e);
            }
        }
        warn!("后台任务 worker 已停止");
    }

    async fn run_job(&self, job_id: &str) -> Result<()> {
        let Some(job) = self.store.get(job_id).await else {
            warn!("任务不存在: {}", job_id);
            return Ok(());
        };

        match job.kind {
            JobKind::ManualTransfer => {
                let payload: ManualTransferPayload = serde_json::from_value(job.payload)?;
                self.run_manual_transfer(job_id, payload).await
            }
            JobKind::SubscriptionTransfer => {
                let payload: SubscriptionTransferPayload = serde_json::from_value(job.payload)?;
                self.run_subscription_transfer(job_id, payload).await
            }
        }
    }

    async fn run_subscription_transfer(
        &self,
        job_id: &str,
        payload: SubscriptionTransferPayload,
    ) -> Result<()> {
        self.update_running(job_id, 10, "正在准备订阅转存").await?;

        if payload.file_names.is_empty() {
            self.store
                .update(job_id, |job| {
                    job.status = JobStatus::Succeeded;
                    job.progress = 100;
                    job.message = "没有新文件需要转存".to_string();
                    job.result = Some(json!({
                        "subscription_id": payload.subscription_id,
                        "transferred_count": 0,
                        "skipped": true,
                    }));
                    job.finished_at = Some(now());
                })
                .await?;
            return Ok(());
        }

        self.update_running(job_id, 35, "正在执行订阅转存").await?;
        match self
            .transfer_service
            .auto_transfer_new_files(&payload.subscription_id, &payload.file_names)
            .await
        {
            Ok(result) => {
                let progress = if result.skipped { 100 } else { 95 };
                self.update_running(job_id, progress, &result.reason)
                    .await?;
                self.store
                    .update(job_id, |job| {
                        job.status = JobStatus::Succeeded;
                        job.progress = 100;
                        job.message = result.reason.clone();
                        job.result = Some(json!({
                            "subscription_id": result.subscription_id,
                            "transferred_count": result.transferred_count,
                            "skipped": result.skipped,
                        }));
                        job.finished_at = Some(now());
                    })
                    .await?;
            }
            Err(e) => {
                let message = format!("订阅自动转存失败: {}", e);
                self.store
                    .update(job_id, |job| {
                        job.status = JobStatus::Failed;
                        job.progress = 100;
                        job.message = message.clone();
                        job.error = Some(message.clone());
                        job.finished_at = Some(now());
                    })
                    .await?;
                self.add_transfer_notification(
                    "error",
                    "subscription_transfer_failed",
                    "订阅自动转存失败",
                    &message,
                    HashMap::from([
                        ("mode".to_string(), json!("auto")),
                        ("job_id".to_string(), json!(job_id)),
                        (
                            "subscription_id".to_string(),
                            json!(payload.subscription_id),
                        ),
                        ("file_count".to_string(), json!(payload.file_names.len())),
                    ]),
                )
                .await;
                warn!("{}", message);
            }
        }

        Ok(())
    }

    async fn run_manual_transfer(&self, job_id: &str, req: ManualTransferPayload) -> Result<()> {
        self.update_running(job_id, 5, "正在读取配置").await?;

        let settings = self.settings_store.get().await;
        let cookie = settings.quark_cookie.clone();

        if cookie.is_empty() {
            self.fail_manual_transfer(job_id, &req, None, None, "未配置夸克 Cookie".to_string())
                .await?;
            return Ok(());
        }

        self.update_running(job_id, 15, "正在探测分享链接").await?;
        let quark_probe = QuarkShareProbe::new(cookie.clone());
        let share_info = quark_probe.probe(&req.url, &req.passcode, 200).await;

        if !share_info.ok {
            self.fail_manual_transfer(
                job_id,
                &req,
                None,
                None,
                format!("链接探测失败: {}", share_info.message),
            )
            .await?;
            return Ok(());
        }

        if share_info.files.is_empty() {
            self.fail_manual_transfer(
                job_id,
                &req,
                Some(0),
                None,
                "链接中没有可转存的文件".to_string(),
            )
            .await?;
            return Ok(());
        }

        let pwd_id = match QuarkShareProbe::extract_pwd_id(&req.url) {
            Some(id) => id,
            None => {
                self.fail_manual_transfer(
                    job_id,
                    &req,
                    Some(share_info.file_count),
                    None,
                    "无法提取分享链接 ID".to_string(),
                )
                .await?;
                return Ok(());
            }
        };

        let target_fid = if req.target_fid.trim().is_empty() {
            "0".to_string()
        } else {
            req.target_fid.clone()
        };

        self.update_running(job_id, 45, "正在转存文件").await?;
        let save_client = QuarkSaveClient::new(cookie);
        match save_with_probe(
            &save_client,
            &quark_probe,
            &pwd_id,
            &req.passcode,
            &target_fid,
        )
        .await
        {
            Ok(saved_count) => {
                self.succeed_manual_transfer(
                    job_id,
                    &req,
                    &target_fid,
                    share_info.file_count,
                    saved_count,
                )
                .await?;
            }
            Err(e) => {
                self.fail_manual_transfer(
                    job_id,
                    &req,
                    Some(share_info.file_count),
                    Some(target_fid),
                    format!("转存失败: {}", e),
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn update_running(&self, job_id: &str, progress: u8, message: &str) -> Result<()> {
        self.store
            .update(job_id, |job| {
                job.status = JobStatus::Running;
                job.progress = progress;
                job.message = message.to_string();
                if job.started_at.is_none() {
                    job.started_at = Some(now());
                }
            })
            .await?;
        Ok(())
    }

    async fn succeed_manual_transfer(
        &self,
        job_id: &str,
        req: &ManualTransferPayload,
        target_fid: &str,
        file_count: usize,
        saved_count: usize,
    ) -> Result<()> {
        let message = format!("成功转存 {} 个文件到网盘", saved_count);
        self.store
            .update(job_id, |job| {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
                job.message = message.clone();
                job.result = Some(json!({
                    "file_count": file_count,
                    "saved_count": saved_count,
                    "target_fid": target_fid,
                }));
                job.finished_at = Some(now());
            })
            .await?;

        self.add_transfer_notification(
            "success",
            "manual_transfer_succeeded",
            "手动转存完成",
            &message,
            HashMap::from([
                ("mode".to_string(), json!("manual")),
                ("job_id".to_string(), json!(job_id)),
                ("url".to_string(), json!(req.url)),
                ("target_fid".to_string(), json!(target_fid)),
                ("file_count".to_string(), json!(file_count)),
                ("saved_count".to_string(), json!(saved_count)),
            ]),
        )
        .await;

        Ok(())
    }

    async fn fail_manual_transfer(
        &self,
        job_id: &str,
        req: &ManualTransferPayload,
        file_count: Option<usize>,
        target_fid: Option<String>,
        message: String,
    ) -> Result<()> {
        self.store
            .update(job_id, |job| {
                job.status = JobStatus::Failed;
                job.progress = 100;
                job.message = message.clone();
                job.error = Some(message.clone());
                job.finished_at = Some(now());
            })
            .await?;

        let mut meta = HashMap::from([
            ("mode".to_string(), json!("manual")),
            ("job_id".to_string(), json!(job_id)),
            ("url".to_string(), json!(req.url)),
            (
                "target_fid".to_string(),
                json!(target_fid.unwrap_or_else(|| req.target_fid.clone())),
            ),
        ]);
        if let Some(file_count) = file_count {
            meta.insert("file_count".to_string(), json!(file_count));
        }

        self.add_transfer_notification(
            "error",
            "manual_transfer_failed",
            "手动转存失败",
            &message,
            meta,
        )
        .await;

        Ok(())
    }

    async fn add_transfer_notification(
        &self,
        level: &str,
        event: &str,
        title: &str,
        message: &str,
        meta: HashMap<String, serde_json::Value>,
    ) {
        let notification = crate::models::Notification {
            id: uuid::Uuid::new_v4().to_string(),
            level: level.to_string(),
            event: event.to_string(),
            title: title.to_string(),
            message: message.to_string(),
            meta,
            read: false,
            created_at: now(),
        };

        if let Err(e) = self.notification_store.add(notification).await {
            warn!("写入转存历史失败: {}", e);
        }
    }
}

async fn save_with_probe(
    save_client: &QuarkSaveClient,
    probe: &QuarkShareProbe,
    pwd_id: &str,
    passcode: &str,
    target_fid: &str,
) -> Result<usize> {
    let (stoken, err) = probe.get_share_token(pwd_id, passcode).await?;
    if let Some(err_msg) = err {
        return Err(AppError::Http(format!("获取分享 token 失败: {}", err_msg)));
    }

    let stoken = stoken.ok_or_else(|| AppError::Http("未能获取分享 token".to_string()))?;
    let (fresh_files, err) = probe.list_share_files(pwd_id, &stoken, "0").await?;
    if let Some(err_msg) = err {
        return Err(AppError::Http(format!("重新获取文件列表失败: {}", err_msg)));
    }

    let mut fid_list = Vec::new();
    let mut fid_token_list = Vec::new();

    for item in &fresh_files {
        let fid = item
            .get("fid")
            .or_else(|| item.get("file_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let share_fid_token = item
            .get("share_fid_token")
            .or_else(|| item.get("file_token"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !fid.is_empty() && !share_fid_token.is_empty() {
            fid_list.push(fid.to_string());
            fid_token_list.push(share_fid_token.to_string());
        }
    }

    if fid_list.is_empty() {
        return Err(AppError::Validation(
            "没有可转存的文件（缺少 fid 或 token）".to_string(),
        ));
    }

    save_client
        .save_share_files(pwd_id, &stoken, &fid_list, &fid_token_list, target_fid)
        .await?;

    Ok(fid_list.len())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_job_store_add_update_list() {
        let tmp =
            std::env::temp_dir().join(format!("my-media-sub-jobs-{}.json", uuid::Uuid::new_v4()));
        let store = JobStore::new(&tmp);
        store.load().await.unwrap();

        let job = Job {
            id: "job1".to_string(),
            kind: JobKind::ManualTransfer,
            status: JobStatus::Queued,
            progress: 0,
            title: "测试任务".to_string(),
            message: "queued".to_string(),
            payload: json!({"url": "https://pan.quark.cn/s/test"}),
            result: None,
            error: None,
            created_at: 1,
            updated_at: 1,
            started_at: None,
            finished_at: None,
        };

        store.add(job).await.unwrap();
        store
            .update("job1", |job| {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
            })
            .await
            .unwrap();

        let loaded = store.get("job1").await.unwrap();
        assert_eq!(loaded.status, JobStatus::Succeeded);
        assert_eq!(store.list().await.len(), 1);

        let _ = std::fs::remove_file(tmp);
    }
}
