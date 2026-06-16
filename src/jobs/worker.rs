use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::clients::{QuarkSaveClient, QuarkShareProbe};
use crate::error::{AppError, Result};
use crate::models::{MediaMetadata, Subscription};
use crate::services::notification::add_notification;
use crate::services::{MetadataService, SubscriptionTransferService};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

use super::model::{
    now, JobKind, JobStatus, ManualTransferPayload, MetadataScrapePayload,
    SubscriptionTransferPayload,
};
use super::store::JobStore;

pub(crate) struct JobWorker {
    pub(crate) store: Arc<JobStore>,
    pub(crate) settings_store: Arc<SettingsStore>,
    pub(crate) subscription_store: Arc<SubscriptionStore>,
    pub(crate) notification_store: Arc<NotificationStore>,
    pub(crate) metadata_service: Arc<MetadataService>,
    pub(crate) transfer_service: Arc<SubscriptionTransferService>,
    pub(crate) receiver: mpsc::Receiver<String>,
}

impl JobWorker {
    pub(crate) async fn run(mut self) {
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

        if job.status != JobStatus::Queued {
            info!("跳过非排队任务 {}: {:?}", job_id, job.status);
            return Ok(());
        }

        match job.kind {
            JobKind::ManualTransfer => {
                let payload: ManualTransferPayload = serde_json::from_value(job.payload)?;
                self.run_manual_transfer(job_id, payload).await
            }
            JobKind::SubscriptionTransfer => {
                let payload: SubscriptionTransferPayload = serde_json::from_value(job.payload)?;
                self.run_subscription_transfer(job_id, payload).await
            }
            JobKind::MetadataScrape => {
                let payload: MetadataScrapePayload = serde_json::from_value(job.payload)?;
                self.run_metadata_scrape(job_id, payload).await
            }
        }
    }

    async fn run_metadata_scrape(
        &self,
        job_id: &str,
        payload: MetadataScrapePayload,
    ) -> Result<()> {
        self.update_running(job_id, 5, "正在准备元数据刮削").await?;

        let subscriptions = if let Some(id) = payload.subscription_id.as_deref() {
            match self.subscription_store.get(id).await {
                Some(sub) => vec![sub],
                None => {
                    self.store
                        .update(job_id, |job| {
                            job.status = JobStatus::Failed;
                            job.progress = 100;
                            job.message = "订阅不存在".to_string();
                            job.error = Some("订阅不存在".to_string());
                            job.finished_at = Some(now());
                        })
                        .await?;
                    return Ok(());
                }
            }
        } else {
            self.subscription_store.list().await
        };

        let total = subscriptions.len();
        if total == 0 {
            self.finish_metadata_scrape(job_id, 0, 0, 0, "没有可刮削的订阅")
                .await?;
            return Ok(());
        }

        let mut scraped = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;

        for (index, sub) in subscriptions.iter().enumerate() {
            if sub.metadata.is_some() && !payload.overwrite {
                skipped += 1;
                self.update_metadata_progress(job_id, index + 1, total, "已有元数据，已跳过")
                    .await?;
                continue;
            }

            match self.scrape_subscription_metadata(sub).await {
                Ok(Some(metadata)) => {
                    self.apply_subscription_metadata(&sub.id, metadata).await?;
                    scraped += 1;
                    self.update_metadata_progress(job_id, index + 1, total, "已匹配并写入元数据")
                        .await?;
                }
                Ok(None) => {
                    failed += 1;
                    self.update_metadata_progress(job_id, index + 1, total, "未找到匹配元数据")
                        .await?;
                }
                Err(e) => {
                    failed += 1;
                    warn!("订阅 {} 元数据刮削失败: {}", sub.title, e);
                    self.update_metadata_progress(job_id, index + 1, total, "刮削失败")
                        .await?;
                }
            }
        }

        let message = format!(
            "元数据刮削完成：写入 {} 个，跳过 {} 个，未匹配/失败 {} 个",
            scraped, skipped, failed
        );
        self.finish_metadata_scrape(job_id, scraped, skipped, failed, &message)
            .await?;

        self.add_transfer_notification(
            if failed > 0 && scraped == 0 {
                "warning"
            } else {
                "success"
            },
            "metadata_scrape_completed",
            "订阅元数据刮削完成",
            &message,
            HashMap::from([
                ("mode".to_string(), json!("metadata")),
                ("job_id".to_string(), json!(job_id)),
                (
                    "subscription_id".to_string(),
                    json!(payload.subscription_id.unwrap_or_default()),
                ),
                ("scraped_count".to_string(), json!(scraped)),
                ("skipped_count".to_string(), json!(skipped)),
                ("failed_count".to_string(), json!(failed)),
            ]),
        )
        .await;

        Ok(())
    }

    async fn scrape_subscription_metadata(
        &self,
        sub: &Subscription,
    ) -> Result<Option<MediaMetadata>> {
        let candidates = self
            .metadata_service
            .search(
                &self.settings_store,
                &sub.title,
                Some(sub.media_type.as_str()),
            )
            .await?;
        Ok(MetadataService::choose_best_match(
            &sub.title,
            &sub.media_type,
            &candidates,
        ))
    }

    async fn apply_subscription_metadata(
        &self,
        subscription_id: &str,
        metadata: MediaMetadata,
    ) -> Result<()> {
        self.subscription_store
            .update(subscription_id, |sub| {
                sub.metadata = Some(metadata);
                sub.updated_at = now();
            })
            .await?
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
        Ok(())
    }

    async fn update_metadata_progress(
        &self,
        job_id: &str,
        current: usize,
        total: usize,
        message: &str,
    ) -> Result<()> {
        let progress = 10 + ((current as f32 / total.max(1) as f32) * 80.0).round() as u8;
        self.update_running(
            job_id,
            progress.min(95),
            &format!("{} ({}/{})", message, current, total),
        )
        .await
    }

    async fn finish_metadata_scrape(
        &self,
        job_id: &str,
        scraped: usize,
        skipped: usize,
        failed: usize,
        message: &str,
    ) -> Result<()> {
        self.store
            .update(job_id, |job| {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
                job.message = message.to_string();
                job.result = Some(json!({
                    "scraped_count": scraped,
                    "skipped_count": skipped,
                    "failed_count": failed,
                }));
                job.finished_at = Some(now());
            })
            .await?;
        Ok(())
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
            .try_update(job_id, |job| {
                if job.status == JobStatus::Canceled {
                    return Err(AppError::Validation("任务已取消".to_string()));
                }
                job.status = JobStatus::Running;
                job.progress = progress;
                job.message = message.to_string();
                if job.started_at.is_none() {
                    job.started_at = Some(now());
                }
                Ok(())
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
        if let Err(e) =
            add_notification(&self.notification_store, level, event, title, message, meta).await
        {
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
