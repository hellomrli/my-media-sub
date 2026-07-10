use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::response::json_ok;
use crate::error::Result;
use crate::jobs::{JobQueue, ManualTransferPayload};

pub struct TransferState {
    pub job_queue: Arc<JobQueue>,
}

#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub url: String,
    #[serde(default)]
    pub passcode: String,
    #[serde(default)]
    pub target_fid: String,
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
    Json(req): Json<TransferRequest>,
) -> Result<impl IntoResponse> {
    let job = state
        .job_queue
        .submit_manual_transfer(ManualTransferPayload {
            url: req.url,
            passcode: req.passcode,
            target_fid: req.target_fid,
        })
        .await?;

    Ok(json_ok(TransferResponse {
        success: true,
        message: Some("转存任务已创建，正在后台执行".to_string()),
        job_id: Some(job.id),
        file_count: None,
        saved_count: None,
    }))
}

pub fn routes(job_queue: Arc<JobQueue>) -> Router {
    let state = Arc::new(TransferState { job_queue });

    Router::new()
        .route("/api/transfer", post(transfer_share))
        .with_state(state)
}
