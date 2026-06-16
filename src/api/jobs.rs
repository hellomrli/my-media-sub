use axum::{
    extract::{Path, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

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

async fn job_events(
    State(state): State<Arc<JobState>>,
) -> Result<Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>>> {
    let snapshot = state.store.list().await;
    let snapshot_data = serde_json::to_string(&snapshot).unwrap_or_else(|_| "[]".to_string());
    let snapshot_stream =
        tokio_stream::once(Ok(Event::default().event("snapshot").data(snapshot_data)));

    let updates = BroadcastStream::new(state.store.subscribe()).filter_map(|event| match event {
        Ok(job) => {
            let data = serde_json::to_string(&job).ok()?;
            Some(Ok(Event::default().event("job").data(data)))
        }
        Err(_) => None,
    });

    Ok(Sse::new(snapshot_stream.chain(updates)).keep_alive(KeepAlive::default()))
}

pub fn routes(store: Arc<JobStore>) -> Router {
    let state = Arc::new(JobState { store });

    Router::new()
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/events", get(job_events))
        .route("/api/jobs/{id}", get(get_job))
        .with_state(state)
}
