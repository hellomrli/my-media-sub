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

/// 自更新允许下载自/跳转到的主机白名单。
///
/// 为什么需要：更新器会**逐字采纳** GitHub API 返回的 `browser_download_url`，
/// 而 reqwest 默认最多跟随 10 次重定向。旧实现既没有校验 URL 主机，也没有限制
/// 重定向，因此只要 API 响应被篡改（或上游出现开放重定向），下载就会被引到任意的
/// 第三方地址——而下载内容会被解包并覆盖运行中的二进制。
///
/// GitHub 的资产下载确实需要重定向：`github.com/.../releases/download/...` 会 302
/// 到 `objects.githubusercontent.com` 或 `release-assets.githubusercontent.com`，
/// 因此不能简单禁用重定向，必须**逐跳校验目标主机**。
const UPDATE_ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "api.github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "github-releases.githubusercontent.com",
];

/// 判断自更新下载目标主机是否在白名单内。
///
/// 只接受 https；比较是**精确匹配**而不是后缀匹配——后缀匹配会让
/// `github.com.evil.example` 通过。
pub fn is_allowed_update_host(url: &reqwest::Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    // 每一跳都必须是 https：重定向到 http://github.com/... 会让后续下载
    // 明文进行，中间人即可替换载荷。
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| UPDATE_ALLOWED_HOSTS.contains(&host))
}

static UPDATE_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(300))
        .pool_max_idle_per_host(4)
        // 逐跳校验重定向目标：任何一跳落到白名单之外就中止下载。
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("更新下载重定向次数过多");
            }
            if is_allowed_update_host(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("更新下载被重定向到非白名单主机")
            }
        }))
        .build()
        .unwrap_or_else(|error| {
            tracing::warn!("创建自更新 HTTP 客户端失败，使用默认客户端: {}", error);
            Client::new()
        })
});

/// 自更新专用的 HTTP 客户端：限时 300 秒、限制重定向目标主机。
pub fn update_client() -> Client {
    UPDATE_CLIENT.clone()
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
