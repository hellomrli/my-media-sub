//! Structured logging context shared by HTTP requests and background jobs.
//!
//! Identifiers are deliberately stored as owned strings: jobs persist them and may run
//! long after the originating request has completed.

use std::future::Future;
use std::sync::{LazyLock, OnceLock, RwLock};

use tracing_subscriber::{
    layer::SubscriberExt, reload, util::SubscriberInitExt, EnvFilter, Registry,
};

use tracing::{info_span, Instrument, Span};

type FilterHandle = reload::Handle<EnvFilter, Registry>;
static FILTER_HANDLE: OnceLock<FilterHandle> = OnceLock::new();
static LOG_FILTER: LazyLock<RwLock<String>> = LazyLock::new(|| RwLock::new("info".to_string()));

pub fn init_tracing() {
    let filter_text = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let filter = EnvFilter::try_new(&filter_text).unwrap_or_else(|_| EnvFilter::new("info"));
    *LOG_FILTER.write().unwrap_or_else(|p| p.into_inner()) = filter.to_string();
    let (filter_layer, handle) = reload::Layer::new(filter);
    if std::env::var("LOG_FORMAT").is_ok_and(|value| value.eq_ignore_ascii_case("json")) {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }
    let _ = FILTER_HANDLE.set(handle);
}

pub fn runtime_reload_available() -> bool {
    FILTER_HANDLE.get().is_some()
}

pub fn log_filter() -> String {
    LOG_FILTER.read().unwrap_or_else(|p| p.into_inner()).clone()
}

pub fn set_log_filter(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 512 {
        return Err("日志过滤规则长度必须为 1-512 个字符".to_string());
    }
    let filter = EnvFilter::try_new(value).map_err(|error| format!("无效日志过滤规则: {error}"))?;
    if let Some(handle) = FILTER_HANDLE.get() {
        handle
            .reload(filter.clone())
            .map_err(|error| format!("更新日志过滤规则失败: {error}"))?;
    }
    let normalized = filter.to_string();
    *LOG_FILTER.write().unwrap_or_else(|p| p.into_inner()) = normalized.clone();
    Ok(normalized)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogContext {
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub subscription_id: Option<String>,
    pub job_id: Option<String>,
}

tokio::task_local! {
    static LOG_CONTEXT: LogContext;
}

pub fn current_context() -> LogContext {
    LOG_CONTEXT.try_with(Clone::clone).unwrap_or_default()
}

pub async fn in_context<T>(context: LogContext, span: Span, future: impl Future<Output = T>) -> T {
    LOG_CONTEXT.scope(context, future.instrument(span)).await
}

pub fn request_span(context: &LogContext, method: &str, path: &str) -> Span {
    info_span!(
        "http.request",
        request_id = context.request_id.as_deref().unwrap_or(""),
        correlation_id = context.correlation_id.as_deref().unwrap_or(""),
        subscription_id = tracing::field::Empty,
        job_id = tracing::field::Empty,
        http.method = method,
        http.path = path,
    )
}

pub fn subscription_span(context: &LogContext) -> Span {
    info_span!(
        "subscription.check",
        request_id = context.request_id.as_deref().unwrap_or(""),
        correlation_id = context.correlation_id.as_deref().unwrap_or(""),
        subscription_id = context.subscription_id.as_deref().unwrap_or(""),
        job_id = context.job_id.as_deref().unwrap_or(""),
    )
}

pub fn job_span(context: &LogContext, kind: &str, attempt: u32) -> Span {
    info_span!(
        "job.execute",
        request_id = context.request_id.as_deref().unwrap_or(""),
        correlation_id = context.correlation_id.as_deref().unwrap_or(""),
        subscription_id = context.subscription_id.as_deref().unwrap_or(""),
        job_id = context.job_id.as_deref().unwrap_or(""),
        job.kind = kind,
        job.attempt = attempt,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn context_is_scoped_and_restored() {
        assert_eq!(current_context(), LogContext::default());
        let context = LogContext {
            request_id: Some("req-1".into()),
            correlation_id: Some("corr-1".into()),
            subscription_id: None,
            job_id: None,
        };
        in_context(context.clone(), tracing::info_span!("test"), async {
            assert_eq!(current_context(), context);
        })
        .await;
        assert_eq!(current_context(), LogContext::default());
    }
}
