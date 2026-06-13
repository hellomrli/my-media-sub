use axum::{
    extract::{Query, State},
    Json,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    clients::{QuarkClient, QuarkSaveClient},
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

/// 转存分享文件请求
#[derive(Debug, Deserialize)]
pub struct SaveShareRequest {
    /// 分享链接
    pub url: String,
    /// 提取码
    #[serde(default)]
    pub passcode: String,
    /// 目标目录路径
    #[serde(default)]
    pub target_dir: String,
}

/// 转存分享文件
pub async fn save_share(
    State(state): State<AppState>,
    Json(req): Json<SaveShareRequest>,
) -> Result<impl IntoResponse> {
    let cookie = state.config.quark.cookie.clone();
    
    // 先探测获取文件列表和 token
    let mut probe_client = QuarkClient::new(cookie.clone());
    let pwd_id = QuarkClient::extract_pwd_id(&req.url)
        .ok_or_else(|| crate::error::AppError::Validation("无效的分享链接".to_string()))?;
    
    let stoken = probe_client.get_share_token(&pwd_id, &req.passcode).await?;
    let files = probe_client.list_files(&pwd_id, &stoken, "0").await?;
    
    // 准备转存
    let save_client = QuarkSaveClient::new(cookie);
    
    // 确保目标目录存在
    let target_fid = if !req.target_dir.is_empty() {
        save_client.ensure_dir_path(&req.target_dir).await?
    } else {
        "0".to_string()
    };
    
    // 转存所有顶层文件
    let fid_list: Vec<String> = files.iter()
        .map(|f| f.fid.clone())
        .collect();
    let fid_token_list: Vec<String> = files.iter()
        .map(|f| f.share_fid_token.clone())
        .collect();
    
    let result = save_client.save_share_files(
        &pwd_id,
        &stoken,
        fid_list,
        fid_token_list,
        &target_fid,
    ).await?;
    
    Ok(Json(result))
}
