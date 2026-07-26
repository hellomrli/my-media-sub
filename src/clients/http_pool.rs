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

/// 幂等请求的瞬时故障重试次数（含首次尝试）。
const IDEMPOTENT_MAX_ATTEMPTS: u32 = 3;
/// 首次重试前的等待时间，之后按指数递增。
const IDEMPOTENT_BASE_DELAY: Duration = Duration::from_millis(300);

/// 连接、超时和请求发送阶段的失败可以安全重放：请求要么没送达，要么上游没给出应答。
/// 夸克在高峰期经常出现这种抖动（`error sending request for url ...`），
/// 直接把它当成订阅失效会产生大量误报。
fn transient_send_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

/// Adds dependency latency/failure metrics to a reqwest request without changing callers'
/// response or error semantics.
pub trait ObservedRequestBuilder {
    fn send_observed(
        self,
        service: &'static str,
    ) -> impl std::future::Future<Output = reqwest::Result<reqwest::Response>> + Send;

    /// 与 `send_observed` 相同，但对瞬时网络故障和上游 5xx 做指数退避重试。
    ///
    /// 仅用于幂等请求（读取、探测、取 token）。有副作用的调用（转存、重命名、
    /// 删除）必须继续用 `send_observed`，否则超时重放会重复执行操作。
    fn send_observed_idempotent(
        self,
        service: &'static str,
        operation: &'static str,
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

    async fn send_observed_idempotent(
        self,
        service: &'static str,
        operation: &'static str,
    ) -> reqwest::Result<reqwest::Response> {
        let mut pending = self;
        let mut attempt = 1;
        loop {
            // 流式 body 无法克隆，此时只尝试一次。
            let next = (attempt < IDEMPOTENT_MAX_ATTEMPTS)
                .then(|| pending.try_clone())
                .flatten();
            let result = pending.send_observed(service).await;
            let reason = match &result {
                Err(error) if transient_send_error(error) => error.to_string(),
                Ok(response) if response.status().is_server_error() => {
                    format!("上游返回 {}", response.status())
                }
                _ => return result,
            };
            let Some(next) = next else {
                return result;
            };
            let delay = IDEMPOTENT_BASE_DELAY * 2u32.pow(attempt - 1);
            tracing::warn!(
                "{}第 {}/{} 次尝试失败，{} ms 后重试: {}",
                operation,
                attempt,
                IDEMPOTENT_MAX_ATTEMPTS,
                delay.as_millis(),
                reason
            );
            tokio::time::sleep(delay).await;
            pending = next;
            attempt += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// 前 `failures` 个连接直接断开（复现夸克的 `error sending request`），之后返回 200。
    async fn flaky_server(failures: usize) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = attempts.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let seen = counter.fetch_add(1, Ordering::SeqCst);
                let mut buffer = [0u8; 1024];
                let _ = socket.read(&mut buffer).await;
                if seen < failures {
                    drop(socket);
                    continue;
                }
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                    .await;
                let _ = socket.flush().await;
            }
        });
        (format!("http://{}/", addr), attempts)
    }

    #[tokio::test]
    async fn idempotent_send_retries_transient_failures() {
        let (url, attempts) = flaky_server(2).await;
        let response = short_client()
            .get(&url)
            .send_observed_idempotent("test", "测试请求")
            .await
            .expect("请求应在重试后成功");
        assert!(response.status().is_success());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn idempotent_send_stops_after_max_attempts() {
        let (url, attempts) = flaky_server(usize::MAX).await;
        let result = short_client()
            .get(&url)
            .send_observed_idempotent("test", "测试请求")
            .await;
        assert!(result.is_err());
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            IDEMPOTENT_MAX_ATTEMPTS as usize
        );
    }

    #[tokio::test]
    async fn plain_send_does_not_retry() {
        let (url, attempts) = flaky_server(1).await;
        let result = short_client().get(&url).send_observed("test").await;
        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
