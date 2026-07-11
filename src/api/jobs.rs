use axum::{
    extract::{Path, Query, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use super::response::ApiResponse as Response;
use crate::error::{AppError, Result};
use crate::jobs::{Job, JobPriority, JobQueue, JobStore};

pub struct JobState {
    pub store: Arc<JobStore>,
    pub queue: Arc<JobQueue>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ListQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
struct SetPriorityRequest {
    priority: JobPriority,
}

async fn list_jobs(
    State(state): State<Arc<JobState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Response<Vec<Job>>>> {
    let jobs = match query.limit {
        Some(limit) => {
            state
                .store
                .list_paginated(query.offset.unwrap_or(0), limit)
                .await
        }
        None => state.store.list().await,
    };
    Ok(Json(Response::ok(jobs)))
}

async fn list_archived_jobs(
    State(state): State<Arc<JobState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Response<Vec<Job>>>> {
    Ok(Json(Response::ok(
        state
            .store
            .list_archived(
                query.offset.unwrap_or(0),
                query.limit.unwrap_or(100).min(500),
            )
            .await?,
    )))
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

async fn cancel_job(
    State(state): State<Arc<JobState>>,
    Path(id): Path<String>,
) -> Result<Json<Response<Job>>> {
    Ok(Json(Response::ok(state.queue.cancel(&id).await?)))
}

async fn retry_job(
    State(state): State<Arc<JobState>>,
    Path(id): Path<String>,
) -> Result<Json<Response<Job>>> {
    Ok(Json(Response::ok(state.queue.retry(&id).await?)))
}

async fn set_job_priority(
    State(state): State<Arc<JobState>>,
    Path(id): Path<String>,
    Json(request): Json<SetPriorityRequest>,
) -> Result<Json<Response<Job>>> {
    Ok(Json(Response::ok(
        state.queue.set_priority(&id, request.priority).await?,
    )))
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

pub fn routes(store: Arc<JobStore>, queue: Arc<JobQueue>) -> Router {
    let state = Arc::new(JobState { store, queue });

    Router::new()
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/archive", get(list_archived_jobs))
        .route("/api/jobs/events", get(job_events))
        .route("/api/jobs/{id}", get(get_job))
        .route("/api/jobs/{id}/cancel", post(cancel_job))
        .route("/api/jobs/{id}/retry", post(retry_job))
        .route("/api/jobs/{id}/priority", post(set_job_priority))
        .with_state(state)
}
