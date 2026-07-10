use super::*;

impl JobWorker {
    pub(super) async fn run_push_dispatch(
        &self,
        job_id: &str,
        payload: PushDispatchPayload,
    ) -> Result<()> {
        self.update_running(job_id, 10, "正在准备推送").await?;

        let Some(event) = PushEvent::from_name(&payload.event) else {
            let message = format!("未知推送事件: {}", payload.event);
            self.fail_push_dispatch(job_id, message, None).await?;
            return Ok(());
        };
        let Some(level) = PushLevel::from_name(&payload.level) else {
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

        record_push_message_report_for_notification(
            &self.notification_store,
            payload.notification_id.as_deref(),
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

    pub(super) async fn enqueue_push_dispatch(&self, payload: PushDispatchPayload) -> Result<Job> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = now();
        let payload_value = serde_json::to_value(payload)?;
        let job = Job {
            id: id.clone(),
            kind: JobKind::PushDispatch,
            status: JobStatus::Queued,
            progress: 0,
            title: "推送派发".to_string(),
            message: "等待后台任务执行".to_string(),
            idempotency_key: Some(job_idempotency_key(&JobKind::PushDispatch, &payload_value)),
            payload: payload_value,
            result: None,
            error: None,
            created_at,
            updated_at: created_at,
            started_at: None,
            finished_at: None,
        };

        let (job, created) = self.store.add_idempotent(job).await?;
        if !created {
            return Ok(job);
        }
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

    pub(super) async fn skip_push_dispatch(
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

    pub(super) async fn fail_push_dispatch(
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
}
