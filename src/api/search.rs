use axum::{
    extract::{Query, State},
    Json,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::{
    error::Result,
    clients::PanSouClient,
    AppState,
};

/// 搜索请求
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    /// 关键词
    pub keyword: String,
    /// 网盘类型
    #[serde(default = "default_cloud_type")]
    pub cloud_type: String,
    /// 返回数量
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_cloud_type() -> String {
    "quark".to_string()
}

fn default_limit() -> usize {
    50
}

/// 搜索资源
pub async fn search(
    State(_state): State<AppState>,
    Query(req): Query<SearchRequest>,
) -> Result<impl IntoResponse> {
    let client = PanSouClient::new(None);
    let results = client.search(&req.keyword, &req.cloud_type, req.limit).await?;
    
    Ok(Json(results))
}
