use axum::{
    extract::{Query, State},
    Json,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    clients::QuarkClient,
    AppState,
};

/// 探测分享链接请求
#[derive(Debug, Deserialize)]
pub struct ProbeShareRequest {
    /// 分享链接
    pub url: String,
    /// 提取码
    #[serde(default)]
    pub passcode: String,
    /// 最大文件数
    #[serde(default = "default_max_files")]
    pub max_files: usize,
}

fn default_max_files() -> usize {
    300
}

/// 探测分享链接
pub async fn probe_share(
    State(state): State<AppState>,
    Query(req): Query<ProbeShareRequest>,
) -> Result<impl IntoResponse> {
    let cookie = state.config.quark.cookie.clone();
    let mut client = QuarkClient::new(cookie);
    
    let info = client.probe(&req.url, &req.passcode, req.max_files).await;
    
    Ok(Json(info))
}
