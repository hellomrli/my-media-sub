use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
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
}

async fn search_metadata(
    State(state): State<Arc<MetadataState>>,
    Query(query): Query<MetadataSearchQuery>,
) -> Result<Json<Response<Vec<MediaMetadata>>>> {
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
    let results = state
        .metadata_service
        .search(
            &state.settings_store,
            search_query,
            query.media_type.as_deref(),
        )
        .await?;
    let results = crate::services::metadata::MetadataService::rank_candidates(
        search_query,
        query.media_type.as_deref(),
        year,
        results,
    );

    Ok(Json(Response::ok(results)))
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
