use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::clients::{QuarkSaveClient, QuarkShareProbe};
use crate::error::{AppError, Result};
use crate::models::{episode_count_for_season, MediaMetadata, Subscription};
use crate::services::notification::add_notification;
use crate::services::push::{
    record_push_message_report, PushEvent, PushLevel, PushRetryPolicy, PushService,
};
use crate::services::{MetadataService, SubscriptionTransferService};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

use super::model::{
    now, Job, JobKind, JobStatus, ManualTransferPayload, MetadataScrapePayload,
    PushDispatchPayload, SubscriptionTransferPayload,
};
use super::store::JobStore;

pub(crate) struct JobWorker {
    pub(crate) store: Arc<JobStore>,
    pub(crate) sender: mpsc::Sender<String>,
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
                if self.is_canceled(&job_id).await {
                    info!("任务 {} 已取消", job_id);
                } else {
                    error!("任务 {} 执行失败: {}", job_id, e);
                }
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
            JobKind::PushDispatch => {
                let payload: PushDispatchPayload = serde_json::from_value(job.payload)?;
                self.run_push_dispatch(job_id, payload).await
            }
        }
    }

    async fn run_push_dispatch(&self, job_id: &str, payload: PushDispatchPayload) -> Result<()> {
        self.update_running(job_id, 10, "正在准备推送").await?;

        let Some(event) = PushEvent::from_str(&payload.event) else {
            let message = format!("未知推送事件: {}", payload.event);
            self.fail_push_dispatch(job_id, message, None).await?;
            return Ok(());
        };
        let Some(level) = PushLevel::from_str(&payload.level) else {
            let message = format!("未知推送级别: {}", payload.level);
            self.fail_push_dispatch(job_id, message, None).await?;
            return Ok(());
        };

        let settings = self.settings_store.get().await;
        let push_service = PushService::new(settings);

        if !push_service.event_enabled(event) {
            self.skip_push_dispatch(job_id, &payload, "推送事件开关未启用，已跳过")
                .await?;
            return Ok(());
        }

        if push_service.enabled_channels().is_empty() {
            self.skip_push_dispatch(job_id, &payload, "未配置推送渠道，已跳过")
                .await?;
            return Ok(());
        }

        self.update_running(job_id, 35, "正在发送推送").await?;
        let report = push_service
            .send_event_with_retry_detailed(
                event,
                &payload.title,
                &payload.message,
                level,
                PushRetryPolicy::background_default(),
            )
            .await;

        record_push_message_report(
            &self.notification_store,
            event.as_str(),
            &payload.title,
            &payload.message,
            level,
            &report,
        )
        .await;

        let success_count = report.results.values().filter(|&&ok| ok).count();
        let failed_count = report.results.len().saturating_sub(success_count);
        let result = json!({
            "source_event": event.as_str(),
            "push_title": payload.title,
            "push_level": level.as_str(),
            "results": &report.results,
            "errors": &report.errors,
            "attempts": &report.attempts,
            "success_count": success_count,
            "failed_count": failed_count,
        });

        let message = if failed_count > 0 {
            format!(
                "推送派发失败：成功 {} 个，失败 {} 个",
                success_count, failed_count
            )
        } else {
            format!("推送派发完成：成功 {} 个渠道", success_count)
        };

        if failed_count > 0 {
            self.fail_push_dispatch(job_id, message, Some(result))
                .await?;
        } else {
            self.complete_if_active(job_id, |job| {
                job.status = JobStatus::Succeeded;
                job.progress = 100;
                job.message = message;
                job.result = Some(result);
                job.finished_at = Some(now());
            })
            .await?;
        }

        Ok(())
    }

    async fn enqueue_push_dispatch(&self, payload: PushDispatchPayload) -> Result<Job> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now();
        let job = Job {
            id: id.clone(),
            kind: JobKind::PushDispatch,
            status: JobStatus::Queued,
            progress: 0,
            title: "推送派发".to_string(),
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
        if let Err(e) = self.sender.try_send(id.clone()) {
            self.store
                .update(&id, |job| {
                    job.status = JobStatus::Failed;
                    job.progress = 100;
                    job.message = "任务队列不可用".to_string();
                    job.error = Some(format!("推送任务入队失败: {}", e));
                    job.finished_at = Some(now());
                })
                .await?;
            return Err(AppError::Internal(format!("推送任务入队失败: {}", e)));
        }

        Ok(job)
    }

    async fn skip_push_dispatch(
        &self,
        job_id: &str,
        payload: &PushDispatchPayload,
        message: &str,
    ) -> Result<()> {
        self.complete_if_active(job_id, |job| {
            job.status = JobStatus::Succeeded;
            job.progress = 100;
            job.message = message.to_string();
            job.result = Some(json!({
                "source_event": payload.event,
                "push_title": payload.title,
                "push_level": payload.level,
                "skipped": true,
            }));
            job.finished_at = Some(now());
        })
        .await?;
        Ok(())
    }

    async fn fail_push_dispatch(
        &self,
        job_id: &str,
        message: String,
        result: Option<serde_json::Value>,
    ) -> Result<()> {
        self.complete_if_active(job_id, |job| {
            job.status = JobStatus::Failed;
            job.progress = 100;
            job.message = message.clone();
            job.error = Some(message);
            job.result = result;
            job.finished_at = Some(now());
        })
        .await?;
        Ok(())
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
            if self.is_canceled(job_id).await {
                return Ok(());
            }

            if sub.metadata.is_some() && !payload.overwrite {
                skipped += 1;
                self.update_metadata_progress(job_id, index + 1, total, "已有元数据，已跳过")
                    .await?;
                continue;
            }

            let scrape_result = self.scrape_subscription_metadata(sub).await;
            if self.is_canceled(job_id).await {
                return Ok(());
            }

            match scrape_result {
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
        if self
            .finish_metadata_scrape(job_id, scraped, skipped, failed, &message)
            .await?
        {
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
        }

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
                if let Some(count) = episode_count_for_season(sub.metadata.as_ref(), sub.season) {
                    sub.total_episode_number = Some(count);
                } else if sub.total_episode_number.is_none() {
                    sub.total_episode_number = sub.rules.finish_after_episode;
                }
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
    ) -> Result<bool> {
        self.complete_if_active(job_id, |job| {
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
        .await
    }

    async fn run_subscription_transfer(
        &self,
        job_id: &str,
        payload: SubscriptionTransferPayload,
    ) -> Result<()> {
        self.update_running(job_id, 10, "正在准备订阅转存").await?;

        if payload.file_names.is_empty() {
            self.complete_if_active(job_id, |job| {
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
                let subscription_id = result.subscription_id.clone();
                let transferred_count = result.transferred_count;
                let skipped = result.skipped;
                let reason = result.reason.clone();
                let push_title = result.push_title.clone();
                let push_message = result.push_message.clone();
                let completed = self
                    .complete_if_active(job_id, |job| {
                        job.status = JobStatus::Succeeded;
                        job.progress = 100;
                        job.message = reason;
                        job.result = Some(json!({
                            "subscription_id": subscription_id,
                            "transferred_count": transferred_count,
                            "skipped": skipped,
                        }));
                        job.finished_at = Some(now());
                    })
                    .await?;
                if completed && !skipped {
                    if let (Some(title), Some(message)) = (push_title, push_message) {
                        if let Err(e) = self
                            .enqueue_push_dispatch(PushDispatchPayload {
                                event: PushEvent::TransferSaved.as_str().to_string(),
                                title,
                                message,
                                level: PushLevel::Success.as_str().to_string(),
                            })
                            .await
                        {
                            warn!("创建转存完成推送任务失败: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                let message = format!("订阅自动转存失败: {}", e);
                if !self
                    .complete_if_active(job_id, |job| {
                        job.status = JobStatus::Failed;
                        job.progress = 100;
                        job.message = message.clone();
                        job.error = Some(message.clone());
                        job.finished_at = Some(now());
                    })
                    .await?
                {
                    return Ok(());
                }
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

    async fn is_canceled(&self, job_id: &str) -> bool {
        self.store
            .get(job_id)
            .await
            .is_some_and(|job| job.status == JobStatus::Canceled)
    }

    async fn complete_if_active(
        &self,
        job_id: &str,
        updater: impl FnOnce(&mut Job),
    ) -> Result<bool> {
        let mut completed = false;
        self.store
            .try_update(job_id, |job| {
                if job.status == JobStatus::Canceled {
                    return Ok(());
                }
                updater(job);
                completed = true;
                Ok(())
            })
            .await?;
        Ok(completed)
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
        if !self
            .complete_if_active(job_id, |job| {
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
            .await?
        {
            return Ok(());
        }

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
        if !self
            .complete_if_active(job_id, |job| {
                job.status = JobStatus::Failed;
                job.progress = 100;
                job.message = message.clone();
                job.error = Some(message.clone());
                job.finished_at = Some(now());
            })
            .await?
        {
            return Ok(());
        }

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
