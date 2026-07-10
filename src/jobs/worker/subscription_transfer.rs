use super::*;

impl JobWorker {
    pub(super) async fn run_subscription_transfer(
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
            .auto_transfer_new_files_with_options(
                &payload.subscription_id,
                &payload.file_names,
                payload.force_transfer,
            )
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
                let push_notification_id = result.push_notification_id.clone();
                let result_notification_id = push_notification_id.clone();
                let renamed_count = result.renamed_count;
                let strm_generated_count = result.strm_generated_count;
                let strm_error = result.strm_error.clone();
                let aria2_submitted_count = result.aria2_submitted_count;
                let aria2_error = result.aria2_error.clone();
                let completed = self
                    .complete_if_active(job_id, |job| {
                        job.status = JobStatus::Succeeded;
                        job.progress = 100;
                        job.message = reason;
                        job.result = Some(json!({
                            "subscription_id": subscription_id,
                            "transferred_count": transferred_count,
                            "skipped": skipped,
                            "renamed_count": renamed_count,
                            "strm_generated_count": strm_generated_count,
                            "strm_error": strm_error,
                            "aria2_submitted_count": aria2_submitted_count,
                            "aria2_error": aria2_error,
                            "notification_id": result_notification_id,
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
                                notification_id: push_notification_id,
                                correlation_id: payload.correlation_id.clone(),
                                subscription_id: Some(payload.subscription_id.clone()),
                                episode: None,
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
}
