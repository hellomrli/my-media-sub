pub mod automation;
pub mod automation_token;
pub mod backup;
pub mod calendar;
pub mod diagnostics;
pub mod drive;
pub mod image_proxy;
pub mod jobs;
pub mod metadata;
pub mod metrics;
pub mod notifications;
pub mod push;
pub mod response;
pub mod search;
pub mod settings;
pub mod storage;
pub mod subscription_exchange;
pub mod subscription_source;
pub mod subscriptions;
pub mod telegram;
pub mod transfer;
pub mod update;
pub mod utils;

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
use tower_http::{compression::CompressionLayer, services::ServeDir};

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
    token_store: Arc<crate::store::AutomationTokenStore>,
    failures: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
}

impl AuthState {
    fn new(
        settings_store: Arc<SettingsStore>,
        token_store: Arc<crate::store::AutomationTokenStore>,
    ) -> Self {
        Self {
            settings_store,
            token_store,
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
            // 容量已满时淘汰最旧的记录，而不是丢弃新的失败记录。
            if let Some(oldest_key) = failures
                .iter()
                .min_by_key(|(_, attempts)| attempts.front().copied().unwrap_or(now))
                .map(|(key, _)| key.clone())
            {
                failures.remove(&oldest_key);
            }
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

/// 计算登录限速使用的 Key。
///
/// X-Forwarded-For 由客户端提供，可被伪造用于绕过按 IP 的锁定；只有在
/// `trust_proxy_headers` 显式开启（即部署在可信反向代理之后）时才使用它。
/// 否则使用对端 Socket IP（main.rs 通过 `into_make_service_with_connect_info`
/// 注入）；测试等无 ConnectInfo 的环境退化为全局常量 Key。
fn auth_rate_key(
    headers: &HeaderMap,
    peer: Option<std::net::SocketAddr>,
    trust_proxy_headers: bool,
) -> String {
    if !trust_proxy_headers {
        return peer
            .map(|addr| addr.ip().to_string())
            .unwrap_or_else(|| "direct-client".to_string());
    }
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        // 常见反代（proxy_add_x_forwarded_for）是追加模式：最左侧一段是
        // 客户端自带的、完全可控的头；最右侧一段由可信代理追加，才是
        // 可信的客户端地址。取最右侧可避免攻击者轮换伪造 XFF 获得无限
        // 个独立限流键绕过登录锁定。
        .and_then(|value| value.rsplit(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .unwrap_or("direct-client")
        .to_string()
}

async fn basic_auth(State(state): State<AuthState>, req: Request<Body>, next: Next) -> Response {
    // Telegram 回调使用随机路径和 Header Secret 双重认证，不属于浏览器会话；
    // 必须先于 Origin/Fetch-Metadata CSRF 判断放行到专用 Handler 校验。
    if req.uri().path().starts_with("/api/telegram/webhook/") {
        return next.run(req).await;
    }
    if is_cross_site_state_change(&req) {
        // webhook 已在上方短路，这里仍然脱敏，避免将来调整顺序时泄漏路径密钥。
        tracing::warn!(
            "拒绝跨站状态修改请求: {} {}",
            req.method(),
            crate::utils::redact_log_path(&req.uri().to_string())
        );
        return forbidden_response();
    }

    if req.uri().path() == "/health" {
        return next.run(req).await;
    }

    let settings = state.settings_store.get().await;

    // Host 白名单：配置后拒绝不在列表内的 Host。
    //
    // 放在凭据校验之前：这类请求不该消耗一次凭据比较，也不该被计入登录失败限流
    // （那会让攻击者用伪造 Host 把正常用户打进限流）。
    if !host_is_allowed(&settings.allowed_hosts, req.headers()) {
        tracing::warn!(
            "拒绝 Host 不在白名单内的请求: {} {}",
            req.headers()
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<缺失>"),
            crate::utils::redact_log_path(&req.uri().to_string())
        );
        return forbidden_response();
    }
    let peer = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0);
    let rate_key = auth_rate_key(req.headers(), peer, settings.trust_proxy_headers);
    let now = Instant::now();

    // 限流**只惩罚凭据错误的请求**，绝不能拦下凭据正确的请求。
    //
    // 旧实现在这里先判 `is_blocked` 再校验凭据，于是失败计数一旦饱和，
    // 携带正确密码/Token 的请求也会被 429 拦下，而清除计数只发生在校验成功之后
    // （永远到不了），攻击者只要维持约 5 次/分钟的错误请求就能让整个实例不可用。
    // 反代后 trust_proxy_headers=false 时所有请求共用同一限流键，一人打满即全员锁死。
    if let Some(token) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        let Some(scope) = required_token_scope(req.method(), req.uri().path()) else {
            return record_auth_failure(&state, &rate_key, now);
        };
        return if state.token_store.authenticate(token, scope).await {
            state.clear(&rate_key);
            next.run(req).await
        } else {
            record_auth_failure(&state, &rate_key, now)
        };
    }

    if settings.app_password.is_empty() {
        tracing::warn!("拒绝请求：应用密码为空，请先配置 SERVER_PASSWORD 或系统设置密码");
        return unauthorized_response();
    }
    if is_default_app_password(&settings.app_password) {
        tracing::warn!(
            "拒绝请求：应用密码仍为默认值 change-me，请通过 APP_PASSWORD 环境变量或系统设置修改密码"
        );
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
        record_auth_failure(&state, &rate_key, now)
    }
}

/// 记录一次凭据失败；已经超限则回 429，否则回 401。
///
/// 注意顺序：**先判是否已超限，再记录本次失败**，以保持「窗口内连续 5 次失败
/// 后开始限流」的既有语义（前 5 次返回 401，第 6 次起返回 429）。
/// 这个函数只在凭据校验**失败之后**被调用，因此携带正确凭据的请求永远不会
/// 被限流拦住——这正是修复未认证 DoS 的关键。
fn record_auth_failure(state: &AuthState, rate_key: &str, now: Instant) -> Response {
    if state.is_blocked(rate_key, now) {
        return auth_rate_limited_response();
    }
    state.record_failure(rate_key.to_string(), now);
    unauthorized_response()
}

pub(crate) fn is_default_app_password(password: &str) -> bool {
    password == "change-me"
}

pub(crate) fn required_token_scope(method: &Method, path: &str) -> Option<&'static str> {
    if path.starts_with("/api/automation-token")
        || path.starts_with("/api/settings")
        || path.starts_with("/api/backups")
        || path.starts_with("/api/storage")
        || path.starts_with("/api/update")
    {
        return None;
    }
    let read = method == Method::GET;
    if path.starts_with("/api/subscriptions") {
        Some(if read {
            "subscriptions:read"
        } else if path.ends_with("/check") || path == "/api/subscriptions/check" {
            "subscriptions:check"
        } else {
            "subscriptions:write"
        })
    } else if path.starts_with("/api/jobs") {
        Some(if read { "jobs:read" } else { "jobs:write" })
    } else if read && path.starts_with("/api/images/") {
        Some("read")
    } else if path == "/api/quark/signin" {
        Some("quark:signin")
    } else if path.starts_with("/api/notifications") {
        Some(if read {
            "notifications:read"
        } else {
            "notifications:write"
        })
    } else if path.starts_with("/api/diagnostics")
        || path.starts_with("/api/telegram/audits")
        || path == "/metrics"
    {
        Some("diagnostics:read")
    } else if read
        && TOKEN_READ_PATH_PREFIXES
            .iter()
            .any(|prefix| path == prefix.trim_end_matches('/') || path.starts_with(prefix))
    {
        Some("read")
    } else {
        // 默认拒绝：未在允许清单中的路径不对自动化 Token 开放。
        None
    }
}

/// 允许 `read` Scope 的自动化 Token 访问的 GET 路径前缀（显式允许清单）。
const TOKEN_READ_PATH_PREFIXES: &[&str] = &[
    "/api/automation/",
    "/api/calendar/",
    "/api/metadata/",
    "/api/push/",
    "/api/drive/",
    "/api/utils/",
];

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
        // from_static 不再每个 401 都解析并分配一次 HeaderValue。
        header::HeaderValue::from_static(r#"Basic realm="my-media-sub""#),
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
        // 没有 Origin 也没有 Sec-Fetch-Site：判定为**非浏览器客户端**并放行。
        //
        // 这里是刻意不 fail-closed 的。本服务用 Header 凭据（Basic / Bearer），
        // 不用 Cookie，浏览器不会自动附带凭据，因此不存在「跨站请求自动带上
        // 身份」的 CSRF 前提；而 README 与 docs/automation-api.md 里所有示例都是
        // `curl -u ... -X POST`，curl 从不发送 Origin，改成 fail-closed 会让
        // 全部文档化的用法失效。
        //
        // 残余风险也已封堵：所有状态变更 handler 都消费 `Json<T>`，仓库内不存在
        // form/raw-body 提取器，因此 HTML 表单跨站提交会被 415 拒绝。
        // 若将来引入 Cookie 会话或表单端点，必须把这里改成 fail-closed。
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

/// 校验请求的 Host 是否在允许列表内。
///
/// - 白名单为空 → 不校验（返回 true），保持与未配置的旧版本一致；
/// - 列表项允许写成 `example.com`、`example.com:56001` 或带 scheme 的完整 URL，
///   统一归一化到主机名后比较（忽略大小写与结尾的点）；
/// - `Host` 缺失或含 `@`（userinfo 混淆）一律拒绝。
///
/// 同时拒绝 **absolute-form** 请求行里与 Host 不一致的 authority：
/// `GET http://evil.example/api/... HTTP/1.1` 这种形式在代理场景合法，
/// 但它的 authority 才是路由依据，若与 Host 不一致就说明有人在混淆目标。
pub(crate) fn host_is_allowed(allowed: &[String], headers: &HeaderMap) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let Some(host) = request_host(headers) else {
        return false;
    };
    let host_without_port = strip_port(&host).to_string();
    allowed.iter().any(|entry| {
        normalize_host_entry(entry)
            .is_some_and(|candidate| candidate == host || candidate == host_without_port)
    })
}

/// 去掉 authority 里的端口。IPv6 字面量带方括号（`[::1]:56001`），不能简单按
/// 第一个冒号切——那会切出 `[`。
fn strip_port(authority: &str) -> &str {
    if authority.starts_with('[') {
        return authority
            .find(']')
            .map(|end| &authority[..=end])
            .unwrap_or(authority);
    }
    authority.split(':').next().unwrap_or(authority)
}

/// 把白名单项归一化成主机名：容忍 `https://host:port/path`、`host:port`、`host` 三种写法。
fn normalize_host_entry(entry: &str) -> Option<String> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    // 去掉 scheme 与路径
    let without_scheme = entry
        .strip_prefix("https://")
        .or_else(|| entry.strip_prefix("http://"))
        .unwrap_or(entry);
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    normalize_host(strip_port(authority))
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
    // Webhook 路径段本身是凭据，日志与 span 只能记录脱敏后的形态。
    let path = crate::utils::redact_log_path(req.uri().path());
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

async fn security_headers(
    State(settings_store): State<Arc<SettingsStore>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let hsts_enabled = settings_store.get().await.hsts_enabled;
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
    // HSTS 必须是显式开关（默认关闭）：它按**主机名**生效、不区分端口。自托管里
    // 常见「一个域名多个端口、只有本服务走 TLS」——无条件发送会让浏览器把同主机
    // 其它端口的纯 HTTP 服务也强制升级成 https，直接打不开。开启后不加
    // includeSubDomains，那会影响同域下的其它子域，超出本服务的权限范围。
    if hsts_enabled {
        headers.insert(
            "strict-transport-security",
            header::HeaderValue::from_static("max-age=31536000"),
        );
    }
    if path == "/service-worker.js" {
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-cache"),
        );
        headers.insert(
            "service-worker-allowed",
            header::HeaderValue::from_static("/"),
        );
    } else if !path.starts_with("/api/")
        && (path.starts_with("/js/")
            || path.starts_with("/icons/")
            || path.ends_with(".css")
            || path.ends_with(".webmanifest"))
    {
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("public, max-age=3600, must-revalidate"),
        );
    } else if path == "/" || path.ends_with(".html") {
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-cache"),
        );
    } else if path.starts_with("/api/") && !path.starts_with("/api/images/") {
        // 认证后的 API 响应一律不得进入任何缓存。`GET /api/settings/secret/{key}`
        // 会返回 app_password / quark_cookie / telegram_bot_token / VAPID 私钥原文，
        // Token 轮换响应会返回新 Token —— 旧实现对这些响应不设缓存头，
        // 中间代理或浏览器启发式缓存可能把凭据留在鉴权边界之外。
        // `/api/images/` 例外：图片代理自己设置长缓存，且内容不含凭据。
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
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
    let auth_state = AuthState::new(
        settings_store.clone(),
        context.automation_token_store.clone(),
    );

    // 静态文件服务
    let serve_static =
        ServeDir::new(crate::utils::static_dir()).append_index_html_on_directories(true);

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
        .merge(automation_token::routes(
            context.automation_token_store.clone(),
        ))
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
        .merge(image_proxy::routes())
        .merge(notifications::routes(context.notification_store.clone()))
        .merge(drive::routes(
            settings_store.clone(),
            context.quark_signin_service.clone(),
            context.download_monitor.clone(),
            context.subscription_store.clone(),
            context.notification_store.clone(),
        ))
        .merge(telegram::routes(context.telegram_bot.clone()))
        .merge(transfer::routes(context.job_queue.clone()))
        .merge(update::routes())
        .merge(push::routes(
            settings_store,
            context.notification_store.clone(),
        ))
        .merge(subscription_source::routes(context.clone()))
        .merge(subscription_exchange::routes(
            context.subscription_store.clone(),
        ))
        .merge(utils::routes())
        .route("/api/{*path}", any(api_not_found))
        .fallback_service(serve_static)
        .layer(middleware::from_fn_with_state(
            context.settings_store.clone(),
            security_headers,
        ))
        .layer(middleware::from_fn(normalize_api_error_response))
        .layer(middleware::from_fn_with_state(auth_state, basic_auth))
        .layer(middleware::from_fn(request_context))
        .layer(CompressionLayer::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
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

    #[test]
    fn default_password_is_rejected_for_login() {
        assert!(is_default_app_password("change-me"));
        assert!(!is_default_app_password("s3cure-password"));
        assert!(!is_default_app_password(""));
    }

    #[test]
    fn auth_rate_key_ignores_forwarded_for_without_trust() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        assert_eq!(auth_rate_key(&headers, None, false), "direct-client");
        let peer: std::net::SocketAddr = "9.9.9.9:4242".parse().unwrap();
        assert_eq!(auth_rate_key(&headers, Some(peer), false), "9.9.9.9");
    }

    #[test]
    fn auth_rate_key_uses_forwarded_for_when_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        // 追加模式下最右侧一段由可信代理写入，客户端伪造的最左段不参与限流。
        assert_eq!(auth_rate_key(&headers, None, true), "5.6.7.8");
        assert_eq!(
            auth_rate_key(&HeaderMap::new(), None, true),
            "direct-client"
        );
    }

    #[test]
    fn auth_rate_key_takes_rightmost_forwarded_entry() {
        let mut headers = HeaderMap::new();
        // 攻击者轮换伪造最左段不应产生新的限流键。
        headers.insert(
            "x-forwarded-for",
            "6.6.6.6, 7.7.7.7, 203.0.113.9".parse().unwrap(),
        );
        assert_eq!(auth_rate_key(&headers, None, true), "203.0.113.9");
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "   ".parse().unwrap());
        assert_eq!(auth_rate_key(&headers, None, true), "direct-client");
    }

    #[test]
    fn record_failure_evicts_oldest_when_full() {
        let mut failures: HashMap<String, VecDeque<Instant>> = HashMap::new();
        let now = Instant::now();
        for i in 0..10_000 {
            failures.insert(format!("key-{i}"), VecDeque::from([now]));
        }
        // 让 key-0 成为最旧记录
        failures.insert(
            "key-0".to_string(),
            VecDeque::from([now - Duration::from_secs(30)]),
        );
        let state = AuthState::new(
            Arc::new(SettingsStore::new(
                std::env::temp_dir().join("auth-test-settings.json"),
            )),
            Arc::new(crate::store::AutomationTokenStore::new(
                std::env::temp_dir().join("auth-test-tokens.json"),
            )),
        );
        *state
            .failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = failures;
        state.record_failure("new-key".to_string(), now);
        let failures = state.failures.lock().unwrap();
        assert!(failures.contains_key("new-key"), "新记录必须被记录");
        assert!(!failures.contains_key("key-0"), "最旧记录应被淘汰");
        assert!(failures.len() <= 10_000);
    }

    #[test]
    fn token_scope_default_denies_unlisted_paths() {
        assert_eq!(required_token_scope(&Method::GET, "/api/search"), None);
        assert_eq!(required_token_scope(&Method::GET, "/api/transfer"), None);
        assert_eq!(
            required_token_scope(&Method::GET, "/api/some/unknown/path"),
            None
        );
        assert_eq!(required_token_scope(&Method::GET, "/api/settings"), None);
    }

    #[test]
    fn token_scope_allows_listed_read_paths() {
        assert_eq!(
            required_token_scope(&Method::GET, "/api/automation/events"),
            Some("read")
        );
        assert_eq!(
            required_token_scope(&Method::GET, "/api/calendar"),
            Some("read")
        );
        assert_eq!(
            required_token_scope(&Method::GET, "/api/metadata/search"),
            Some("read")
        );
        assert_eq!(
            required_token_scope(&Method::GET, "/api/push/status"),
            Some("read")
        );
        assert_eq!(
            required_token_scope(&Method::GET, "/api/drive"),
            Some("read")
        );
        // 写操作不落入 read 允许清单
        assert_eq!(required_token_scope(&Method::POST, "/api/calendar"), None);
    }

    // ── Host 白名单（DNS rebinding 防线）────────────────────────────────────

    fn host_headers(host: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(value) = host {
            headers.insert(header::HOST, HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    /// 未配置白名单时不校验，保持与旧版本一致。
    #[test]
    fn empty_host_allowlist_accepts_any_host() {
        assert!(host_is_allowed(
            &[],
            &host_headers(Some("anything.example"))
        ));
        // 即使 Host 缺失也放行——此时不应因为一个未启用的功能而拒绝请求
        assert!(host_is_allowed(&[], &host_headers(None)));
    }

    /// 配置后只接受列表内的主机。
    #[test]
    fn configured_host_allowlist_filters_hosts() {
        let allowed = vec!["media.example.com".to_string()];
        assert!(host_is_allowed(
            &allowed,
            &host_headers(Some("media.example.com"))
        ));
        // 大小写与结尾的点都应归一化
        assert!(host_is_allowed(
            &allowed,
            &host_headers(Some("Media.Example.com."))
        ));
        // 带端口也接受（Host 头默认含端口）
        assert!(host_is_allowed(
            &allowed,
            &host_headers(Some("media.example.com:56001"))
        ));
        // DNS rebinding 的目标主机必须被拒
        assert!(!host_is_allowed(
            &allowed,
            &host_headers(Some("evil.example"))
        ));
        // Host 缺失必须被拒（无法证明它就是白名单主机）
        assert!(!host_is_allowed(&allowed, &host_headers(None)));
        // userinfo 混淆
        assert!(!host_is_allowed(
            &allowed,
            &host_headers(Some("media.example.com@evil.example"))
        ));
    }

    /// 白名单项支持 `host`、`host:port` 与完整 URL 三种写法。
    #[test]
    fn host_allowlist_entries_accept_several_notations() {
        for entry in [
            "media.example.com",
            "media.example.com:56001",
            "https://media.example.com",
            "https://media.example.com:56001/",
            "http://media.example.com/path?x=1",
        ] {
            let allowed = vec![entry.to_string()];
            assert!(
                host_is_allowed(&allowed, &host_headers(Some("media.example.com"))),
                "白名单项 {entry} 应匹配 media.example.com"
            );
            assert!(
                !host_is_allowed(&allowed, &host_headers(Some("other.example"))),
                "白名单项 {entry} 不应匹配 other.example"
            );
        }
        // 空字符串项被忽略，不会意外放行一切
        let allowed = vec!["".to_string(), "   ".to_string()];
        assert!(!host_is_allowed(
            &allowed,
            &host_headers(Some("media.example.com"))
        ));
    }

    /// IPv6 字面量带方括号，端口剥离不能按第一个冒号切。
    #[test]
    fn host_allowlist_handles_ipv6_literals() {
        let allowed = vec!["[::1]".to_string()];
        assert!(host_is_allowed(
            &allowed,
            &host_headers(Some("[::1]:56001"))
        ));
        assert!(host_is_allowed(&allowed, &host_headers(Some("[::1]"))));
        assert!(!host_is_allowed(
            &allowed,
            &host_headers(Some("[fe80::1]:56001"))
        ));
        let allowed = vec!["http://[::1]:56001/".to_string()];
        assert!(host_is_allowed(
            &allowed,
            &host_headers(Some("[::1]:56001"))
        ));
        assert_eq!(strip_port("[::1]:56001"), "[::1]");
        assert_eq!(strip_port("example.com:443"), "example.com");
        assert_eq!(strip_port("example.com"), "example.com");
    }
}
