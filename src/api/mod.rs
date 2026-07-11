pub mod automation;
pub mod backup;
pub mod calendar;
pub mod diagnostics;
pub mod drive;
pub mod jobs;
pub mod metadata;
pub mod metrics;
pub mod notifications;
pub mod push;
pub mod response;
pub mod search;
pub mod settings;
pub mod storage;
pub mod strm;
pub mod subscription_source;
pub mod subscriptions;
pub mod transfer;
pub mod update;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, Method, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{any, get},
    Router,
};
use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower_http::services::ServeDir;

use crate::app::AppContext;
use crate::error::json_error_response;
use crate::store::SettingsStore;
use crate::utils::constant_time_eq;

/// 健康检查响应
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

/// 健康检查
async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[derive(Clone)]
struct AuthState {
    settings_store: Arc<SettingsStore>,
    failures: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
}

impl AuthState {
    fn new(settings_store: Arc<SettingsStore>) -> Self {
        Self {
            settings_store,
            failures: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn is_blocked(&self, key: &str, now: Instant) -> bool {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        prune_auth_failures(&mut failures, now);
        failures
            .get(key)
            .is_some_and(|attempts| attempts.len() >= 5)
    }

    fn record_failure(&self, key: String, now: Instant) {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        prune_auth_failures(&mut failures, now);
        if failures.len() >= 10_000 && !failures.contains_key(&key) {
            return;
        }
        failures.entry(key).or_default().push_back(now);
    }

    fn clear(&self, key: &str) {
        self.failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(key);
    }
}

fn prune_auth_failures(failures: &mut HashMap<String, VecDeque<Instant>>, now: Instant) {
    let cutoff = now.checked_sub(Duration::from_secs(60)).unwrap_or(now);
    failures.retain(|_, attempts| {
        while attempts.front().is_some_and(|attempt| *attempt < cutoff) {
            attempts.pop_front();
        }
        !attempts.is_empty()
    });
}

fn auth_rate_key(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .unwrap_or("direct-client")
        .to_string()
}

async fn basic_auth(State(state): State<AuthState>, req: Request<Body>, next: Next) -> Response {
    if is_cross_site_state_change(&req) {
        tracing::warn!("拒绝跨站状态修改请求: {} {}", req.method(), req.uri());
        return forbidden_response();
    }

    if req.uri().path() == "/health" || req.uri().path().starts_with("/strm/") {
        return next.run(req).await;
    }

    let rate_key = auth_rate_key(req.headers());
    let now = Instant::now();
    if state.is_blocked(&rate_key, now) {
        return auth_rate_limited_response();
    }

    let settings = state.settings_store.get().await;
    if settings.app_password.is_empty() {
        tracing::warn!("拒绝请求：应用密码为空，请先配置 SERVER_PASSWORD 或系统设置密码");
        return unauthorized_response();
    }

    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Basic "))
        .and_then(|encoded| general_purpose::STANDARD.decode(encoded).ok())
        .and_then(|decoded| String::from_utf8(decoded).ok())
        .and_then(|credentials| {
            let (username, password) = credentials.split_once(':')?;
            Some(
                constant_time_eq(username, &settings.app_username)
                    && constant_time_eq(password, &settings.app_password),
            )
        })
        .unwrap_or(false);

    if authorized {
        state.clear(&rate_key);
        next.run(req).await
    } else {
        state.record_failure(rate_key, now);
        unauthorized_response()
    }
}

fn auth_rate_limited_response() -> Response {
    let mut response = json_error_response(
        StatusCode::TOO_MANY_REQUESTS,
        "auth_rate_limited",
        "登录失败次数过多，请 60 秒后重试",
    );
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, header::HeaderValue::from_static("60"));
    response
}

fn unauthorized_response() -> Response {
    let mut response = json_error_response(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "认证失败，请提供有效的用户名和密码",
    );
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        r#"Basic realm="my-media-sub""#.parse().unwrap(),
    );
    response
}

fn forbidden_response() -> Response {
    json_error_response(
        StatusCode::FORBIDDEN,
        "csrf_forbidden",
        "拒绝跨站状态修改请求",
    )
}

async fn api_not_found() -> Response {
    json_error_response(StatusCode::NOT_FOUND, "not_found", "请求的 API 不存在")
}

fn is_cross_site_state_change(req: &Request<Body>) -> bool {
    if !is_state_changing_method(req.method()) {
        return false;
    }

    let headers = req.headers();
    if headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.eq_ignore_ascii_case("cross-site"))
        .unwrap_or(false)
    {
        return true;
    }

    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };

    let Some(origin_host) = host_from_origin(origin) else {
        return true;
    };
    let Some(request_host) = request_host(headers) else {
        return true;
    };

    origin_host != request_host
}

fn is_state_changing_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

async fn normalize_api_error_response(req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path();
    let is_api = path == "/api" || path.starts_with("/api/");
    let response = next.run(req).await;
    if !is_api || !response.status().is_client_error() && !response.status().is_server_error() {
        return response;
    }

    let already_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("json"));
    if already_json {
        return response;
    }

    let status = response.status();
    let preserved_headers = response.headers().clone();
    let (error, message) = match status {
        StatusCode::BAD_REQUEST => ("bad_request", "请求参数不正确"),
        StatusCode::UNAUTHORIZED => ("unauthorized", "认证失败"),
        StatusCode::FORBIDDEN => ("forbidden", "请求被拒绝"),
        StatusCode::NOT_FOUND => ("not_found", "请求的 API 不存在"),
        StatusCode::METHOD_NOT_ALLOWED => ("method_not_allowed", "请求方法不受支持"),
        StatusCode::PAYLOAD_TOO_LARGE => ("payload_too_large", "请求内容过大"),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ("unsupported_media_type", "请求内容类型不受支持"),
        StatusCode::UNPROCESSABLE_ENTITY => ("invalid_request", "请求内容无法解析"),
        StatusCode::TOO_MANY_REQUESTS => ("rate_limited", "请求过于频繁"),
        _ if status.is_server_error() => ("internal_error", "服务内部错误"),
        _ => ("request_failed", "请求失败"),
    };
    let mut normalized = json_error_response(status, error, message);
    for (name, value) in &preserved_headers {
        if name != header::CONTENT_TYPE && name != header::CONTENT_LENGTH {
            normalized.headers_mut().insert(name.clone(), value.clone());
        }
    }
    normalized
}

fn host_from_origin(origin: &str) -> Option<String> {
    if origin.eq_ignore_ascii_case("null") {
        return None;
    }
    let (_, rest) = origin.split_once("://")?;
    let authority = rest.split('/').next()?.trim();
    normalize_host(authority)
}

fn request_host(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_host)
}

fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.');
    if host.is_empty() || host.contains('@') {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

async fn request_context(mut req: Request<Body>, next: Next) -> Response {
    let request_id = request_header_id(req.headers(), "x-request-id")
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let correlation_id =
        request_header_id(req.headers(), "x-correlation-id").unwrap_or_else(|| request_id.clone());
    req.extensions_mut().insert(request_id.clone());
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let context = crate::observability::LogContext {
        request_id: Some(request_id.clone()),
        correlation_id: Some(correlation_id.clone()),
        subscription_id: None,
        job_id: None,
    };
    let span = crate::observability::request_span(&context, method.as_str(), &path);
    let started = Instant::now();
    let mut response = crate::observability::in_context(context, span, next.run(req)).await;
    if let Ok(value) = header::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    if let Ok(value) = header::HeaderValue::from_str(&correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    let duration = started.elapsed();
    crate::utils::metrics::global_metrics().observe_http_request(
        method.as_str(),
        response.status().as_u16(),
        duration,
    );
    tracing::info!(request_id = %request_id, correlation_id = %correlation_id, method = %method, path = %path, status = response.status().as_u16(), duration_ms = duration.as_millis(), "request completed");
    response
}

fn request_header_id(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
                })
        })
        .map(ToString::to_string)
}

async fn security_headers(req: Request<Body>, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert("content-security-policy", header::HeaderValue::from_static("default-src 'self'; script-src 'self' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self'; font-src 'self' data:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'"));
    headers.insert(
        "x-content-type-options",
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", header::HeaderValue::from_static("DENY"));
    headers.insert(
        "referrer-policy",
        header::HeaderValue::from_static("no-referrer"),
    );
    if path == "/service-worker.js" {
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-cache"),
        );
        headers.insert(
            "service-worker-allowed",
            header::HeaderValue::from_static("/"),
        );
    }
    headers.insert(
        "permissions-policy",
        header::HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    response
}

/// 创建主应用路由
pub fn create_app(context: Arc<AppContext>) -> Router {
    let settings_store = context.settings_store.clone();
    let auth_state = AuthState::new(settings_store.clone());

    // 静态文件服务
    let serve_static = ServeDir::new("static").append_index_html_on_directories(true);

    // 构建路由：API 优先，静态文件作为 fallback
    Router::new()
        .route("/health", get(health))
        .merge(subscriptions::routes(
            context.subscription_store.clone(),
            settings_store.clone(),
            context.check_service.clone(),
            context.transfer_service.clone(),
            context.job_queue.clone(),
            context.job_store.clone(),
            context.notification_store.clone(),
            context.automation_event_store.clone(),
        ))
        .merge(settings::routes(
            settings_store.clone(),
            context.scheduler.clone(),
            context.quark_signin_scheduler.clone(),
        ))
        .merge(search::routes(settings_store.clone()))
        .merge(metadata::routes(
            settings_store.clone(),
            context.metadata_service.clone(),
        ))
        .merge(automation::routes(context.clone()))
        .merge(calendar::routes(
            context.subscription_store.clone(),
            settings_store.clone(),
            context.job_store.clone(),
            context.notification_store.clone(),
            context.automation_event_store.clone(),
        ))
        .merge(metrics::routes(context.metrics.clone()))
        .merge(diagnostics::routes(context.clone()))
        .merge(storage::routes(context.clone()))
        .merge(backup::routes(context.backup_service.clone()))
        .merge(jobs::routes(
            context.job_store.clone(),
            context.job_queue.clone(),
        ))
        .merge(notifications::routes(context.notification_store.clone()))
        .merge(drive::routes(
            settings_store.clone(),
            context.quark_signin_service.clone(),
            context.download_monitor.clone(),
            context.subscription_store.clone(),
            context.notification_store.clone(),
        ))
        .merge(strm::routes(settings_store.clone()))
        .merge(transfer::routes(context.job_queue.clone()))
        .merge(update::routes())
        .merge(push::routes(
            settings_store,
            context.notification_store.clone(),
        ))
        .merge(subscription_source::routes(context.clone()))
        .route("/api/{*path}", any(api_not_found))
        .fallback_service(serve_static)
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(normalize_api_error_response))
        .layer(middleware::from_fn_with_state(auth_state, basic_auth))
        .layer(middleware::from_fn(request_context))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn request(method: Method, origin: Option<&str>, fetch_site: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri("/api/settings")
            .header(header::HOST, "media.example.com");
        if let Some(origin) = origin {
            builder = builder.header(header::ORIGIN, origin);
        }
        if let Some(fetch_site) = fetch_site {
            builder = builder.header("sec-fetch-site", fetch_site);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn csrf_allows_same_origin_state_change() {
        let req = request(
            Method::POST,
            Some("https://media.example.com"),
            Some("same-origin"),
        );
        assert!(!is_cross_site_state_change(&req));
    }

    #[test]
    fn csrf_blocks_cross_site_state_change() {
        let req = request(
            Method::POST,
            Some("https://evil.example.net"),
            Some("cross-site"),
        );
        assert!(is_cross_site_state_change(&req));
    }

    #[test]
    fn csrf_allows_cli_style_state_change_without_browser_headers() {
        let req = request(Method::POST, None, None);
        assert!(!is_cross_site_state_change(&req));
    }

    #[test]
    fn csrf_does_not_block_reads() {
        let req = request(
            Method::GET,
            Some("https://evil.example.net"),
            Some("cross-site"),
        );
        assert!(!is_cross_site_state_change(&req));
    }
}
