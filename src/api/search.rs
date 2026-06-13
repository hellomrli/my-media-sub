use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::clients::PanSouClient;
use crate::error::Result;

/// 搜索路由状态
pub struct SearchState {
    pub client: Arc<PanSouClient>,
}

/// 搜索请求
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub keyword: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

/// 通用响应
#[derive(Serialize)]
struct Response<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
}

impl<T> Response<T> {
    fn ok(data: T) -> Self {
        Self { data: Some(data) }
    }
}

/// 搜索资源
async fn search(
    State(state): State<Arc<SearchState>>,
    Json(req): Json<SearchRequest>,
) -> Result<impl IntoResponse> {
    let results = state
        .client
        .search_quark(&req.keyword, req.limit)
        .await?;

    Ok(Json(Response::ok(results)))
}

/// 创建搜索路由
pub fn routes(client: Arc<PanSouClient>) -> Router {
    let state = Arc::new(SearchState { client });

    Router::new()
        .route("/api/search", post(search))
        .with_state(state)
}
