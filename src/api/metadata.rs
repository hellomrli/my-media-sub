use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::response::ApiResponse as Response;
use crate::error::Result;
use crate::models::MediaMetadata;
use crate::services::{MetadataService, TmdbTestResult};
use crate::store::SettingsStore;

pub struct MetadataState {
    pub settings_store: Arc<SettingsStore>,
    pub metadata_service: Arc<MetadataService>,
}

#[derive(Debug, Deserialize)]
struct MetadataSearchQuery {
    query: String,
    #[serde(default)]
    media_type: Option<String>,
    /// 发行年份提示（可选）。未提供时从 `query` 里自动提取（`庆余年 (2024)`）。
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    source_title: Option<String>,
}

#[derive(Debug, Serialize)]
struct MetadataSearchResponse {
    #[serde(flatten)]
    response: Response<Vec<MediaMetadata>>,
    attempted_queries: Vec<String>,
    matched_query: Option<String>,
}

async fn search_metadata(
    State(state): State<Arc<MetadataState>>,
    Query(query): Query<MetadataSearchQuery>,
) -> Result<Json<MetadataSearchResponse>> {
    let detailed = crate::services::title_normalize::normalize_title_detailed(&query.query);
    let search_query = if detailed.normalized.is_empty() {
        query.query.as_str()
    } else {
        detailed.normalized.as_str()
    };
    let year = query
        .year
        .filter(|year| (1900..=2100).contains(year))
        .or(detailed.year);
    let outcome = state
        .metadata_service
        .search_with_title_fallback(
            &state.settings_store,
            &query.query,
            query.source_title.as_deref(),
            query.media_type.as_deref(),
        )
        .await?;
    let results = crate::services::metadata::MetadataService::rank_candidates(
        outcome.matched_query.as_deref().unwrap_or(search_query),
        query.media_type.as_deref(),
        year,
        outcome.results,
    );

    let response = if outcome.attempted_queries.len() > 1 {
        let message = match &outcome.matched_query {
            Some(matched) => format!(
                "已调整搜索剧名为「{matched}」并匹配到元数据（尝试 {} 个名称）",
                outcome.attempted_queries.len()
            ),
            None => format!(
                "已尝试 {} 个剧名，仍未匹配到元数据：{}",
                outcome.attempted_queries.len(),
                outcome.attempted_queries.join(" / ")
            ),
        };
        Response::with_message(results, message)
    } else {
        Response::ok(results)
    };
    Ok(Json(MetadataSearchResponse {
        response,
        attempted_queries: outcome.attempted_queries,
        matched_query: outcome.matched_query,
    }))
}

async fn test_tmdb(State(state): State<Arc<MetadataState>>) -> Json<Response<TmdbTestResult>> {
    let result = state.metadata_service.test_api(&state.settings_store).await;
    Json(Response::ok(result))
}

pub fn routes(
    settings_store: Arc<SettingsStore>,
    metadata_service: Arc<MetadataService>,
) -> Router {
    let state = Arc::new(MetadataState {
        settings_store,
        metadata_service,
    });

    Router::new()
        .route("/api/metadata/search", get(search_metadata))
        .route("/api/metadata/test", get(test_tmdb))
        .with_state(state)
}
