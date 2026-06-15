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
    /// 是否检测链接有效性
    #[serde(default)]
    pub check_links: bool,
    /// 是否嗅探文件列表
    #[serde(default)]
    pub probe_files: bool,
    /// 嗅探时获取的最大文件数
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    /// 是否过滤失效链接（需要 check_links 为 true）
    #[serde(default)]
    pub filter_bad: bool,
}

fn default_limit() -> usize {
    10
}

fn default_max_files() -> usize {
    50
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

    // 如果需要检测链接有效性或嗅探文件列表
    if req.check_links || req.probe_files {
        let mut processed_results = Vec::new();

        for mut result in results {
            // 探测链接（max_files 控制获取多少文件）
            let max_files = if req.probe_files { req.max_files } else { 1 };
            let probe_result = state.quark_probe.probe(
                &result.url,
                &result.password,
                max_files,
            ).await;

            // 如果需要过滤失效链接
            if req.filter_bad && !probe_result.ok {
                continue; // 跳过失效链接
            }

            // 添加探测信息到结果中
            if req.probe_files {
                result.probe_info = Some(probe_result);
            } else if req.check_links {
                // 只检测有效性，不获取文件列表
                result.is_valid = Some(probe_result.ok);
            }

            processed_results.push(result);
        }

        results = processed_results;
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
