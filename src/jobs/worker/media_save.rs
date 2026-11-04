use super::*;
use crate::clients::Aria2Client;
use crate::models::{MediaSaveOptions, Settings};
use crate::providers::{CloudDriveProvider, ProviderProbeResult};
use crate::services::media_save::{resolve_saved_files, select_save_files};

impl JobWorker {
    pub(super) async fn run_media_save(
        &self,
        job_id: &str,
        req: &ManualTransferPayload,
        options: &MediaSaveOptions,
        settings: &Settings,
        provider: Arc<dyn CloudDriveProvider>,
        probe: ProviderProbeResult,
    ) -> Result<()> {
        if probe.state == "partial" && options.file_ids.is_empty() {
            return Err(AppError::Validation(
                "分享探测不完整，请选择具体电影文件或缩小分享范围".to_string(),
            ));
        }
        let selected = select_save_files(&probe.files, options)?;
        self.update_running(job_id, 30, "已确认所选画质，正在准备保存目录")
            .await?;
        let target_fid = if !options.target_dir.trim().is_empty() {
            provider.ensure(&options.target_dir).await?
        } else if !req.target_fid.trim().is_empty() {
            req.target_fid.clone()
        } else {
            "0".to_string()
        };
        let existing = provider.list(&target_fid).await?;
        if selected
            .iter()
            .any(|file| existing.iter().any(|item| item.name == file.name))
        {
            return Err(AppError::Validation(
                "目标目录已有同名文件，为避免保存或下载错误版本，请选择其他目录".to_string(),
            ));
        }
        if self.is_canceled(job_id).await {
            return Ok(());
        }
        self.update_running(job_id, 45, "正在保存所选资源到网盘")
            .await?;
        self.run_with_heartbeat(
            job_id,
            provider.transfer(TransferRequest {
                share_url: req.url.clone(),
                passcode: req.passcode.clone(),
                target_id: target_fid.clone(),
                file_ids: selected.iter().map(|file| file.id.clone()).collect(),
            }),
        )
        .await?;

        // 云端转存已发生：立即记住结果。后续下载提交失败不能诱导用户重新转存。
        let mut result = json!({
            "file_count": probe.files.len(), "saved_count": selected.len(),
            "target_fid": target_fid, "target_dir": options.target_dir,
            "preferred_resolution": options.preferred_resolution,
            "files": [], "sync_downloads": [], "download_errors": []
        });
        self.store
            .update(job_id, |job| {
                job.result = Some(result.clone());
                job.progress = 70;
                job.message = "资源已保存到网盘，正在确认文件".to_string();
            })
            .await?;

        let mut saved_files = Vec::new();
        let mut errors = Vec::new();
        for attempt in 0..3 {
            match provider
                .list(&target_fid)
                .await
                .and_then(|items| resolve_saved_files(&selected, &items))
            {
                Ok(files) => {
                    saved_files = files;
                    break;
                }
                Err(error) if attempt == 2 => errors.push(error.to_string()),
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(600)).await,
            }
        }
        result["files"] = json!(saved_files
            .iter()
            .map(|file| json!({"fid": file.id, "name": file.name}))
            .collect::<Vec<_>>());
        self.store
            .update(job_id, |job| job.result = Some(result.clone()))
            .await?;
        let mut downloads = Vec::new();
        if options.sync_download_enabled && !saved_files.is_empty() {
            self.update_running(job_id, 80, "网盘保存完成，正在提交本地下载")
                .await?;
            let aria2 = Aria2Client::new(
                &settings.aria2_rpc_url,
                &settings.aria2_secret,
                &options.sync_download_dir,
            );
            for file in &saved_files {
                let download = async {
                    let infos = provider
                        .download_info(std::slice::from_ref(&file.id))
                        .await?;
                    let info = infos
                        .iter()
                        .find(|info| info.id == file.id)
                        .ok_or_else(|| {
                            AppError::Http("网盘未返回所选文件的下载地址".to_string())
                        })?;
                    let output_name = crate::filename::portable_filename(&file.name);
                    aria2
                        .add_uri(&info.download_url, Some(&output_name), &info.headers)
                        .await
                }
                .await;
                match download {
                    Ok(gid) => downloads.push(json!({"gid": gid, "fid": file.id, "file_name": file.name, "download_dir": options.sync_download_dir})),
                    Err(error) => errors.push(format!("{}：{}", file.name, error)),
                }
                result["sync_downloads"] = json!(downloads);
                result["download_errors"] = json!(errors);
                self.store
                    .update(job_id, |job| job.result = Some(result.clone()))
                    .await?;
            }
        }
        result["download_errors"] = json!(errors);
        let message = if !errors.is_empty() {
            format!("已保存 {} 个文件到网盘；本地下载或文件确认未全部完成，请在网盘页继续下载，不要重新转存", selected.len())
        } else if options.sync_download_enabled {
            format!(
                "已保存 {} 个文件到网盘，并提交 {} 个本地下载任务",
                selected.len(),
                downloads.len()
            )
        } else {
            format!("已保存 {} 个所选资源到网盘，不会持续追更", selected.len())
        };
        self.complete_if_active(job_id, |job| {
            job.status = JobStatus::Succeeded;
            job.progress = 100;
            job.message = message.clone();
            job.result = Some(result.clone());
            job.finished_at = Some(now());
        })
        .await?;
        self.add_transfer_notification(
            if errors.is_empty() {
                "success"
            } else {
                "warning"
            },
            "manual_transfer_succeeded",
            "媒体下载保存",
            &message,
            HashMap::from([
                ("mode".to_string(), json!("once")),
                ("job_id".to_string(), json!(job_id)),
                ("media_title".to_string(), json!(options.title)),
                ("target_dir".to_string(), json!(options.target_dir)),
                ("saved_count".to_string(), json!(selected.len())),
                ("sync_downloads".to_string(), json!(downloads)),
                ("download_errors".to_string(), json!(errors)),
            ]),
        )
        .await;
        Ok(())
    }
}
