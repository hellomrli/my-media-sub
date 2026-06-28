use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use std::sync::Arc;

use crate::utils::metrics::{Metrics, MetricsSnapshot};

#[derive(Serialize)]
struct Response<T> {
    ok: bool,
    data: T,
}

impl<T> Response<T> {
    fn success(data: T) -> Self {
        Self { ok: true, data }
    }
}

async fn get_metrics(State(metrics): State<Arc<Metrics>>) -> Json<Response<MetricsSnapshot>> {
    Json(Response::success(metrics.snapshot()))
}

pub fn routes(metrics: Arc<Metrics>) -> Router {
    Router::new()
        .route("/api/metrics", get(get_metrics))
        .with_state(metrics)
}
