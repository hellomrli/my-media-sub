use axum::{extract::State, routing::get, Json, Router};
use std::sync::Arc;

use super::response::ApiResponse as Response;
use crate::utils::metrics::{Metrics, MetricsSnapshot};

async fn get_metrics(State(metrics): State<Arc<Metrics>>) -> Json<Response<MetricsSnapshot>> {
    Json(Response::success(metrics.snapshot()))
}

pub fn routes(metrics: Arc<Metrics>) -> Router {
    Router::new()
        .route("/api/metrics", get(get_metrics))
        .with_state(metrics)
}
