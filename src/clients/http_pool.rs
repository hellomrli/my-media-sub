use reqwest::Client;
use std::sync::LazyLock;
use std::time::Duration;

static DEFAULT_CLIENT: LazyLock<Client> =
    LazyLock::new(|| build_client(Duration::from_secs(30), "默认"));
static SHORT_CLIENT: LazyLock<Client> =
    LazyLock::new(|| build_client(Duration::from_secs(10), "短超时"));
static MEDIUM_CLIENT: LazyLock<Client> =
    LazyLock::new(|| build_client(Duration::from_secs(20), "中等超时"));
static STREAMING_CLIENT: LazyLock<Client> =
    LazyLock::new(|| build_client(Duration::from_secs(300), "流式代理"));

fn build_client(timeout: Duration, label: &str) -> Client {
    Client::builder()
        .timeout(timeout)
        .pool_max_idle_per_host(10)
        .build()
        .unwrap_or_else(|error| {
            tracing::warn!(
                "创建{}共享 HTTP 客户端失败，使用默认客户端: {}",
                label,
                error
            );
            Client::new()
        })
}

pub fn default_client() -> Client {
    DEFAULT_CLIENT.clone()
}

pub fn short_client() -> Client {
    SHORT_CLIENT.clone()
}

pub fn medium_client() -> Client {
    MEDIUM_CLIENT.clone()
}

pub fn streaming_client() -> Client {
    STREAMING_CLIENT.clone()
}

/// Adds dependency latency/failure metrics to a reqwest request without changing callers'
/// response or error semantics.
pub trait ObservedRequestBuilder {
    fn send_observed(
        self,
        service: &'static str,
    ) -> impl std::future::Future<Output = reqwest::Result<reqwest::Response>> + Send;
}

impl ObservedRequestBuilder for reqwest::RequestBuilder {
    async fn send_observed(self, service: &'static str) -> reqwest::Result<reqwest::Response> {
        let started = std::time::Instant::now();
        let result = self.send().await;
        crate::utils::metrics::global_metrics().observe_external_dependency(
            service,
            started.elapsed(),
            result
                .as_ref()
                .is_ok_and(|response| response.status().is_success()),
        );
        result
    }
}
