//! 进程关闭信号。
//!
//! HTTP 优雅关闭会等待所有在途连接结束，而 SSE 这类长连接永远不会自己结束。
//! 在线升级重启因此可能永久卡在优雅关闭里：点「更新」的那个页面自己持有的
//! `/api/jobs/events` 连接，就足以让升级后的新版本永远起不来。
//!
//! 关闭开始时广播一次信号，长连接据此收尾；`main` 另有一道超时兜底。

use std::sync::LazyLock;
use tokio::sync::watch;

static CHANNEL: LazyLock<(watch::Sender<bool>, watch::Receiver<bool>)> =
    LazyLock::new(|| watch::channel(false));

/// 标记关闭已开始。重复调用无副作用。
pub fn begin() {
    let _ = CHANNEL.0.send(true);
}

/// 关闭是否已开始。
pub fn started() -> bool {
    *CHANNEL.1.borrow()
}

/// 关闭开始时 resolve；已经开始则立即返回。
///
/// 长连接用它结束自己的流（例如 `StreamExt::take_until`）。
pub async fn wait() {
    let mut receiver = CHANNEL.1.clone();
    if *receiver.borrow() {
        return;
    }
    // 发送端是进程级静态量，不会被丢弃；真出错也只当作已关闭处理。
    let _ = receiver.changed().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_resolves_after_begin_and_is_idempotent() {
        // 静态通道是进程级的，测试内串行验证「先等待后触发」和「触发后立即返回」。
        let waiter = tokio::spawn(async { wait().await });
        assert!(!started());
        begin();
        begin();
        assert!(started());
        tokio::time::timeout(std::time::Duration::from_secs(5), waiter)
            .await
            .expect("关闭信号应当唤醒等待者")
            .expect("等待任务不应 panic");

        tokio::time::timeout(std::time::Duration::from_secs(5), wait())
            .await
            .expect("关闭已开始时应立即返回");
    }
}
