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
use crate::services::MetadataService;
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
}

async fn search_metadata(
    State(state): State<Arc<MetadataState>>,
    Query(query): Query<MetadataSearchQuery>,
) -> Result<Json<Response<Vec<MediaMetadata>>>> {
    let results = state
        .metadata_service
        .search(
            &state.settings_store,
            &query.query,
            query.media_type.as_deref(),
        )
        .await?;

    Ok(Json(Response::ok(results)))
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
        .with_state(state)
}
