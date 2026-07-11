use super::*;

impl JobWorker {
    pub(super) async fn run_manual_transfer(
        &self,
        job_id: &str,
        req: ManualTransferPayload,
    ) -> Result<()> {
        self.update_running(job_id, 5, "正在读取配置").await?;

        let settings = self.settings_store.get().await;
        let cookie = settings.quark_cookie.clone();

        if cookie.is_empty() {
            self.fail_manual_transfer(job_id, &req, None, None, "未配置夸克 Cookie".to_string())
                .await?;
            return Ok(());
        }

        self.update_running(job_id, 15, "正在探测分享链接").await?;
        let provider =
            CloudDriveProviderRegistry::new().resolve_with_quark_cookie("quark", cookie)?;
        let share_info = provider.probe(&req.url, &req.passcode, 200).await?;

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

        let target_fid = if req.target_fid.trim().is_empty() {
            "0".to_string()
        } else {
            req.target_fid.clone()
        };

        self.update_running(job_id, 45, "正在转存文件").await?;
        if self.is_canceled(job_id).await {
            info!("任务 {} 已在转存前取消", job_id);
            return Ok(());
        }
        match provider
            .transfer(TransferRequest {
                share_url: req.url.clone(),
                passcode: req.passcode.clone(),
                target_id: target_fid.clone(),
                file_ids: Vec::new(),
            })
            .await
        {
            Ok(outcome) => {
                let saved_count = outcome.transferred_files.len();
                self.succeed_manual_transfer(
                    job_id,
                    &req,
                    &target_fid,
                    share_info.files.len(),
                    saved_count,
                )
                .await?;
            }
            Err(e) => {
                self.fail_manual_transfer(
                    job_id,
                    &req,
                    Some(share_info.files.len()),
                    Some(target_fid),
                    format!("转存失败: {}", e),
                )
                .await?;
            }
        }

        Ok(())
    }

    pub(super) async fn succeed_manual_transfer(
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
            self.add_transfer_notification(
                "warning",
                "manual_transfer_completed_after_cancel",
                "转存已完成但任务已取消",
                &format!(
                    "任务取消时夸克转存已经完成，实际已转存 {} 个文件",
                    saved_count
                ),
                HashMap::from([
                    ("mode".to_string(), json!("manual")),
                    ("job_id".to_string(), json!(job_id)),
                    ("saved_count".to_string(), json!(saved_count)),
                    ("target_fid".to_string(), json!(target_fid)),
                    ("source_url".to_string(), json!(req.url.clone())),
                ]),
            )
            .await;
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

    pub(super) async fn fail_manual_transfer(
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
}
