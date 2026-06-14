use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::clients::{PanSouClient, QuarkShareProbe};
use crate::error::Result;

/// 搜索路由状态
pub struct SearchState {
    pub client: Arc<PanSouClient>,
    pub quark_probe: Arc<QuarkShareProbe>,
}

/// 搜索请求
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub keyword: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub check_links: bool,
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
    let mut results = state
        .client
        .search_quark(&req.keyword, req.limit)
        .await?;

    // 如果需要检测链接有效性
    if req.check_links {
        let mut valid_results = Vec::new();

        for result in results {
            // 探测链接
            let probe_result = state.quark_probe.probe(
                &result.url,
                &result.password,
                1, // 只检测有效性，不需要文件列表
            ).await;

            // 只保留有效链接
            if probe_result.ok {
                valid_results.push(result);
            }
        }

        results = valid_results;
    }

    Ok(Json(Response::ok(results)))
}

/// 创建搜索路由
pub fn routes(client: Arc<PanSouClient>, quark_probe: Arc<QuarkShareProbe>) -> Router {
    let state = Arc::new(SearchState { client, quark_probe });

    Router::new()
        .route("/api/search", post(search))
        .with_state(state)
}
