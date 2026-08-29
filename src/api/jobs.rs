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

/// SSE 转发缓冲：足够吸收任务事件突发，又不至于在客户端读得慢时无限堆积。
const SSE_CHANNEL_CAPACITY: usize = 64;

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
        // 与 archive 接口一致：显式 limit 钳制上限，避免异常请求拉取过量数据。
        Some(limit) => {
            state
                .store
                .list_paginated(query.offset.unwrap_or(0), limit.min(500))
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
    let mut updates = BroadcastStream::new(state.store.subscribe());

    // 事件经由 channel 转发，而不是直接把广播流交给 Sse：这样关闭开始时可以主动
    // 结束这条流。SSE 永远不会自己结束，而 axum 的优雅关闭要等所有在途连接收尾，
    // 否则在线升级重启会被浏览器里开着的这条连接永久卡住。
    let (sender, receiver) = tokio::sync::mpsc::channel(SSE_CHANNEL_CAPACITY);
    tokio::spawn(async move {
        if sender
            .send(Ok(Event::default().event("snapshot").data(snapshot_data)))
            .await
            .is_err()
        {
            return;
        }
        loop {
            tokio::select! {
                biased;
                _ = crate::shutdown::wait() => break,
                event = updates.next() => match event {
                    // 落后的订阅者会收到 Lagged，跳过继续跟进最新事件。
                    Some(Err(_)) => continue,
                    Some(Ok(job)) => {
                        let Ok(data) = serde_json::to_string(&job) else { continue };
                        if sender.send(Ok(Event::default().event("job").data(data))).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },
            }
        }
    });

    Ok(
        Sse::new(tokio_stream::wrappers::ReceiverStream::new(receiver))
            .keep_alive(KeepAlive::default()),
    )
}

/// 清空已结束的任务记录。推送派发这类高频任务会迅速堆满活动中心，
/// 需要一个手动清理入口；排队中与运行中的任务不受影响。
async fn clear_finished_jobs(
    State(state): State<Arc<JobState>>,
) -> Result<Json<Response<serde_json::Value>>> {
    let removed = state.store.clear_finished().await?;
    Ok(Json(Response::ok(serde_json::json!({
        "removed": removed,
    }))))
}

pub fn routes(store: Arc<JobStore>, queue: Arc<JobQueue>) -> Router {
    let state = Arc::new(JobState { store, queue });

    Router::new()
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/archive", get(list_archived_jobs))
        .route("/api/jobs/clear", post(clear_finished_jobs))
        .route("/api/jobs/events", get(job_events))
        .route("/api/jobs/{id}", get(get_job))
        .route("/api/jobs/{id}/cancel", post(cancel_job))
        .route("/api/jobs/{id}/retry", post(retry_job))
        .route("/api/jobs/{id}/priority", post(set_job_priority))
        .with_state(state)
}
