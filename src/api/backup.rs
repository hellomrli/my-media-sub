use axum::{
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use super::response::ApiResponse;
use crate::error::{AppError, Result};
use crate::services::backup::{BackupArchive, BackupService, RestoreResult};

#[derive(Deserialize)]
struct RestoreRequest {
    archive: BackupArchive,
    confirmation: String,
}

async fn export_backup(State(service): State<Arc<BackupService>>) -> Result<Response> {
    let archive = service.export_archive().await?;
    let bytes = serde_json::to_vec_pretty(&archive)?;
    let filename = format!("my-media-sub-backup-{}.json", archive.created_at);
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|error| AppError::Internal(error.to_string()))?,
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn list_backups(State(service): State<Arc<BackupService>>) -> Result<impl IntoResponse> {
    Ok(Json(ApiResponse::ok(service.list_stored_backups().await?)))
}

async fn create_backup(State(service): State<Arc<BackupService>>) -> Result<impl IntoResponse> {
    let backup = service.create_stored_backup("manual").await?;
    Ok((StatusCode::CREATED, Json(ApiResponse::ok(backup))))
}

async fn preview_backup(
    State(service): State<Arc<BackupService>>,
    Json(archive): Json<BackupArchive>,
) -> Result<impl IntoResponse> {
    Ok(Json(ApiResponse::ok(service.preview(&archive).await?)))
}

async fn restore_backup(
    State(service): State<Arc<BackupService>>,
    Json(request): Json<RestoreRequest>,
) -> Result<Json<ApiResponse<RestoreResult>>> {
    let result = service
        .restore(&request.archive, &request.confirmation)
        .await?;
    Ok(Json(ApiResponse::with_message(
        result,
        "恢复完成，必须重启服务后再继续操作",
    )))
}

pub fn routes(service: Arc<BackupService>) -> Router {
    Router::new()
        .route("/api/backups", get(list_backups).post(create_backup))
        .route("/api/backups/export", get(export_backup))
        .route("/api/backups/preview", post(preview_backup))
        .route("/api/backups/restore", post(restore_backup))
        .layer(DefaultBodyLimit::max(384 * 1024 * 1024))
        .with_state(service)
}
