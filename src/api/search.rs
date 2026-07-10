use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde::Deserialize;
use std::sync::Arc;

use super::response::ApiResponse as Response;
use crate::clients::{PanSouClient, QuarkShareProbe};
use crate::error::Result;
use crate::services::source_quality::{score_source, SourceQualityFile, SourceQualityInput};
use crate::store::SettingsStore;

/// 搜索路由状态
pub struct SearchState {
    pub settings_store: Arc<SettingsStore>,
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

/// 搜索资源
async fn search(
    State(state): State<Arc<SearchState>>,
    Json(req): Json<SearchRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let pansou_api_url = settings.pansou_api_url.trim().to_string();
    let pansou_api_url = if pansou_api_url.is_empty() {
        None
    } else {
        Some(pansou_api_url)
    };
    let pansou_client = PanSouClient::new(pansou_api_url);
    let mut results = pansou_client
        .search(&req.keyword, &settings.cloud_types, req.limit)
        .await?;

    // 如果需要检测链接有效性或嗅探文件列表
    if req.check_links || req.probe_files {
        let quark_probe = QuarkShareProbe::new(settings.quark_cookie);
        let mut processed_results = Vec::new();

        for mut result in results {
            // 探测链接（max_files 控制获取多少文件）
            let max_files = if req.probe_files { req.max_files } else { 1 };
            let probe_result = quark_probe
                .probe(&result.url, &result.password, max_files)
                .await;

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

    let now_ms = chrono::Utc::now().timestamp_millis();
    for result in &mut results {
        let probe = result.probe_info.as_ref();
        result.quality = score_source(
            &SourceQualityInput {
                title: result.note.clone(),
                datetime: result.datetime.clone(),
                validity: result.is_valid,
                probe_ok: probe.map(|probe| probe.ok),
                probe_file_count: probe.map(|probe| probe.file_count).unwrap_or_default(),
                probe_episode_count: probe.map(|probe| probe.episode_count).unwrap_or_default(),
                files: probe
                    .map(|probe| {
                        probe
                            .files
                            .iter()
                            .map(|file| SourceQualityFile {
                                name: file.name.clone(),
                                is_dir: file.is_dir,
                                size: file.size,
                                updated_at: file.updated_at.clone(),
                                category: file.category.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            now_ms,
        );
    }

    Ok(Json(Response::ok(results)))
}

/// 创建搜索路由
pub fn routes(settings_store: Arc<SettingsStore>) -> Router {
    let state = Arc::new(SearchState { settings_store });

    Router::new()
        .route("/api/search", post(search))
        .with_state(state)
}
