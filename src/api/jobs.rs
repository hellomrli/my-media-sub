use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::jobs::{Job, JobStore};

pub struct JobState {
    pub store: Arc<JobStore>,
}

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

async fn list_jobs(State(state): State<Arc<JobState>>) -> Result<Json<Response<Vec<Job>>>> {
    Ok(Json(Response::ok(state.store.list().await)))
}

async fn get_job(
    State(state): State<Arc<JobState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    match state.store.get(&id).await {
        Some(job) => Ok(Json(Response::ok(job))),
        None => Err(AppError::NotFound("任务不存在".to_string())),
    }
}

pub fn routes(store: Arc<JobStore>) -> Router {
    let state = Arc::new(JobState { store });

    Router::new()
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/{id}", get(get_job))
        .with_state(state)
}
