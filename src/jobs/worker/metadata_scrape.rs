use super::*;

impl JobWorker {
    pub(super) async fn run_metadata_scrape(
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

    pub(super) async fn scrape_subscription_metadata(
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

    pub(super) async fn apply_subscription_metadata(
        &self,
        subscription_id: &str,
        metadata: MediaMetadata,
    ) -> Result<()> {
        self.subscription_store
            .update(subscription_id, |sub| {
                sub.metadata = Some(merge_refreshed_metadata(sub.metadata.as_ref(), metadata));
                if let Some(count) = episode_count_for_season(sub.metadata.as_ref(), sub.season) {
                    sub.total_episode_number = Some(count);
                } else if sub.total_episode_number.is_none() {
                    sub.total_episode_number = sub.rules.finish_after_episode;
                }
                reconcile_completed_subscription_status(sub);
                sub.updated_at = now();
            })
            .await?
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
        Ok(())
    }

    pub(super) async fn update_metadata_progress(
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

    pub(super) async fn finish_metadata_scrape(
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
}
