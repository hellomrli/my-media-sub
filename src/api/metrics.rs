use axum::{
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response as HttpResponse},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::response::ApiResponse as Response;
use crate::error::{AppError, Result};
use crate::utils::metrics::{Metrics, MetricsSnapshot};

async fn get_metrics(State(metrics): State<Arc<Metrics>>) -> Json<Response<MetricsSnapshot>> {
    Json(Response::success(metrics.snapshot()))
}

async fn prometheus_metrics(State(metrics): State<Arc<Metrics>>) -> HttpResponse {
    let mut response = metrics.prometheus().into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
}

#[derive(Serialize)]
struct LogFilterResponse {
    filter: String,
    runtime_reload_available: bool,
}

#[derive(Deserialize)]
struct UpdateLogFilterRequest {
    filter: String,
}

async fn get_log_filter() -> Json<Response<LogFilterResponse>> {
    Json(Response::success(LogFilterResponse {
        filter: crate::observability::log_filter(),
        runtime_reload_available: crate::observability::runtime_reload_available(),
    }))
}

async fn update_log_filter(
    Json(request): Json<UpdateLogFilterRequest>,
) -> Result<Json<Response<LogFilterResponse>>> {
    let filter =
        crate::observability::set_log_filter(&request.filter).map_err(AppError::Validation)?;
    tracing::info!(log_filter = %filter, "runtime log filter updated");
    Ok(Json(Response::success(LogFilterResponse {
        filter,
        runtime_reload_available: crate::observability::runtime_reload_available(),
    })))
}

pub fn routes(metrics: Arc<Metrics>) -> Router {
    Router::new()
        .route("/api/metrics", get(get_metrics))
        .route("/metrics", get(prometheus_metrics))
        .route(
            "/api/observability/log-filter",
            get(get_log_filter).put(update_log_filter),
        )
        .with_state(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_filter_rejects_invalid_directives() {
        assert!(crate::observability::set_log_filter("[").is_err());
    }

    #[test]
    fn prometheus_output_has_core_metric_families() {
        let metrics = Metrics::default();
        metrics.observe_http_request("GET", 200, std::time::Duration::from_millis(25));
        metrics.observe_external_dependency("quark", std::time::Duration::from_millis(40), true);
        let output = metrics.prometheus();
        assert!(
            output.contains("my_media_sub_http_requests_total{method=\"GET\",status=\"200\"} 1")
        );
        assert!(output.contains("my_media_sub_external_requests_total{service=\"quark\"} 1"));
    }
}
