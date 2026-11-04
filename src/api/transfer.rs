use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::response::json_ok;
use crate::error::{AppError, Result};
use crate::jobs::{JobQueue, ManualTransferPayload};
use crate::models::MediaSaveOptions;
use crate::store::SettingsStore;

pub struct TransferState {
    pub job_queue: Arc<JobQueue>,
    pub settings_store: Arc<SettingsStore>,
}

#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub url: String,
    #[serde(default)]
    pub passcode: String,
    #[serde(default)]
    pub target_fid: String,
    #[serde(default)]
    pub save_options: Option<MediaSaveOptions>,
}

#[derive(Serialize)]
pub struct TransferResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_count: Option<usize>,
}

async fn transfer_share(
    State(state): State<Arc<TransferState>>,
    Json(mut req): Json<TransferRequest>,
) -> Result<impl IntoResponse> {
    if let Some(options) = req.save_options.as_mut() {
        if options.title.trim().is_empty() {
            return Err(AppError::Validation("媒体名称不能为空".to_string()));
        }
        options.preferred_resolution =
            crate::services::media_selection::normalize_resolution(&options.preferred_resolution)?;
        if options.media_type == "movie" && options.file_ids.len() > 1 {
            return Err(AppError::Validation("电影请只选择一个正片版本".to_string()));
        }
        if options.file_ids.len() > crate::services::SHARE_PROBE_MAX_FILES
            || options.file_ids.iter().any(|id| id.trim().is_empty())
        {
            return Err(AppError::Validation("资源文件选择无效".to_string()));
        }
        let settings = state.settings_store.get().await;
        if settings.quark_cookie.trim().is_empty() {
            return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
        }
        if options.sync_download_enabled && settings.aria2_rpc_url.trim().is_empty() {
            return Err(AppError::Validation(
                "请先配置 Aria2，或选择仅保存到网盘".to_string(),
            ));
        }
    }
    let selected_save = req.save_options.is_some();
    let job = state
        .job_queue
        .submit_manual_transfer(ManualTransferPayload {
            url: req.url,
            passcode: req.passcode,
            target_fid: req.target_fid,
            offer_download: false,
            save_options: req.save_options,
        })
        .await?;

    Ok(json_ok(TransferResponse {
        success: true,
        message: Some(if selected_save {
            "下载保存任务已创建，只处理所选资源，不会创建追更订阅".to_string()
        } else {
            "转存任务已创建，正在后台执行".to_string()
        }),
        job_id: Some(job.id),
        file_count: None,
        saved_count: None,
    }))
}

pub fn routes(job_queue: Arc<JobQueue>, settings_store: Arc<SettingsStore>) -> Router {
    let state = Arc::new(TransferState {
        job_queue,
        settings_store,
    });

    Router::new()
        .route("/api/transfer", post(transfer_share))
        .with_state(state)
}
