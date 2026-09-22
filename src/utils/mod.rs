use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};

pub mod metrics;

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Resolve the WebUI directory used by both the static file server and the
/// online updater. Relative paths remain relative to the process working
/// directory for compatibility with existing binary deployments.
pub fn static_dir() -> PathBuf {
    static_dir_from_value(std::env::var("STATIC_DIR").ok().as_deref())
}

fn static_dir_from_value(value: Option<&str>) -> PathBuf {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("static"))
}

pub fn write_file_atomic(path: &Path, content: &[u8], mode: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Database(format!("创建目录失败: {}", e)))?;
    }

    let tmp = unique_tmp_path(path);
    let write_result = (|| -> Result<()> {
        let mut file = open_tmp_file(&tmp, mode)?;
        file.write_all(content)
            .map_err(|e| AppError::Database(format!("写入临时文件失败: {}", e)))?;
        file.sync_all()
            .map_err(|e| AppError::Database(format!("同步临时文件失败: {}", e)))?;
        drop(file);

        fs::rename(&tmp, path)
            .map_err(|e| AppError::Database(format!("重命名临时文件失败: {}", e)))?;
        sync_parent_dir(path)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }

    write_result
}

/// 异步原子写入 JSON：序列化在当前线程完成（CPU 操作，开销小），
/// 阻塞的文件系统操作（写入 + fsync + rename）放到 `spawn_blocking`，
/// 避免阻塞 tokio executor 线程。调用方应在持有写锁时序列化以保证写入顺序，
/// 但实际落盘发生在阻塞线程池中。
pub async fn write_json_atomic_async<T: serde::Serialize>(
    path: &Path,
    value: &T,
    mode: u32,
) -> Result<()> {
    let started = std::time::Instant::now();
    // Business stores use compact JSON. Backups and exported diagnostics remain pretty printed.
    let content = serde_json::to_vec(value)
        .map_err(|e| AppError::Database(format!("序列化 JSON 失败: {}", e)))?;
    let bytes = content.len() as u64;
    let metric_name = store_metric_name(path);
    let path = path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || write_file_atomic(&path, &content, mode))
        .await
        .map_err(|e| AppError::Database(format!("写盘任务执行失败: {}", e)))?;
    metrics::global_metrics().observe_store_write(
        &metric_name,
        bytes,
        started.elapsed(),
        result.is_ok(),
    );
    result
}

fn store_metric_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .trim_end_matches("_rust")
        .to_string()
}

pub fn set_file_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| AppError::Database(format!("设置文件权限失败: {}", e)))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

/// 把损坏的 Store 文件改名隔离，返回隔离后的路径。
///
/// 两个细节是刻意的：
/// - 文件名带纳秒级随机后缀。旧实现只用 1 秒分辨率的 `unix_now()`，同一秒内对
///   同一文件再次隔离会**静默覆盖**第一份副本（`rename` 的语义是替换），
///   而隔离文件往往是唯一的数据副本。
/// - 隔离后对父目录做一次 `fsync`，保证"文件已从原位置移走"这件事本身落盘。
/// 进程级环境变量读写的串行化锁。
///
/// 环境变量是**进程全局**状态，而 `cargo test` 默认多线程：两个测试同时读写
/// 同一个变量会互相污染。凡是读写环境变量的测试都应先取这把锁。
///
/// 用 tokio 的 Mutex 是因为调用方都是 async 测试（`#[tokio::test]`）。
#[doc(hidden)]
pub fn env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    &LOCK
}

pub fn quarantine_corrupt_file(path: &Path) -> Option<std::path::PathBuf> {
    let file_name = path.file_name().and_then(|name| name.to_str())?;
    let unique = uuid::Uuid::new_v4().simple().to_string();
    let quarantine = path.with_file_name(format!(
        "{}.corrupt-{}-{}",
        file_name,
        unix_now(),
        &unique[..8]
    ));
    if let Err(error) = fs::rename(path, &quarantine) {
        tracing::error!("隔离损坏文件 {} 失败: {}", path.display(), error);
        return None;
    }
    if let Some(parent) = quarantine.parent() {
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    tracing::error!(
        "已隔离损坏文件 {} 到 {}（原始字节已保留，恢复备份后可还原）",
        path.display(),
        quarantine.display()
    );
    Some(quarantine)
}

/// 隔离损坏 Store 之后，是否允许继续启动。
///
/// 默认**不允许**。损坏的业务 Store 被替换成空集合后，服务会"看起来正常"，
/// 但订阅、作业、通知等全部消失，而且首次写入会生成一个全新的文件（原文件只剩
/// `.corrupt-*` 副本），用户很难意识到发生了什么。设置存储一直是中止启动的，
/// 这里把同一策略推广到其余业务 Store。
///
/// 需要"先把服务起来再手工修复"时，设 `ALLOW_QUARANTINE_STARTUP=true` 放行，
/// 届时会以警告级别提示当前处于降级状态。
pub fn ensure_quarantine_startup_allowed(
    store_label: &str,
    original: &Path,
    quarantined: Option<&Path>,
    error: &str,
) -> Result<()> {
    let quarantined_hint = quarantined
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| format!("{}.corrupt-<时间戳>", original.display()));
    if std::env::var("ALLOW_QUARANTINE_STARTUP")
        .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    {
        tracing::warn!(
            "ALLOW_QUARANTINE_STARTUP 已启用：{} 将以空数据继续运行（原文件已隔离到 {}），             请尽快手工修复后重启",
            store_label,
            quarantined_hint
        );
        return Ok(());
    }
    Err(AppError::Database(format!(
        "{store_label} 解析失败，已隔离到 {quarantined_hint}（原始字节已保留）。         继续启动会把{store_label}替换为空数据，因此这里中止启动。         请修复该文件或从备份恢复后重启；确需以空数据启动，         可设置环境变量 ALLOW_QUARANTINE_STARTUP=true。解析错误：{error}"
    )))
}

pub fn redact_sensitive(value: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    // 键前带可选的定界符（? & ; 空白 行首）避免 sign= 误伤 design= 之类
    // 的普通单词；清单必须覆盖所有出现在 URL query 里的凭据参数：
    // api_key（TMDB）、kps/vcode/sign（夸克移动端）、token/cookie 等。
    static KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)(?:^|[?&;=\s(])(api[_-]?key|apikey|kps|vcode|signature|sign|token|secret|password|passcode|cookie|authorization)=([^&\s]+)",
        )
        .expect("valid sensitive key regex")
    });
    static HEADER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(cookie|authorization):\s*[^\r\n]+").expect("valid sensitive header regex")
    });
    static QUARK_SHARE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(pan\.quark\.cn/s/)[A-Za-z0-9_-]+").expect("valid quark share regex")
    });
    let value = KEY_VALUE.replace_all(value, "$1=[REDACTED]");
    let value = HEADER.replace_all(&value, "$1: [REDACTED]");
    QUARK_SHARE.replace_all(&value, "$1[REDACTED]").into_owned()
}

pub fn redact_url(value: &str) -> String {
    redact_sensitive(value)
}

/// 日志与追踪用的路径脱敏，输入可以是请求路径或完整 URI。
///
/// Telegram Webhook 把一半凭据放在路径段里
/// （`/api/telegram/webhook/{path_secret}`），而 `request_context` 中间件
/// 位于 `basic_auth` 外层，会在后者的 webhook 短路之前记录完整路径与
/// `http.path` span 字段——不在这里抹掉，每次投递（以及任何 404 探测）
/// 都会把路径密钥写进 INFO 日志。
pub fn redact_log_path(path_or_uri: &str) -> String {
    const TELEGRAM_WEBHOOK_PREFIX: &str = "/api/telegram/webhook/";
    if path_or_uri.starts_with(TELEGRAM_WEBHOOK_PREFIX) {
        // 只保留前缀用于定位，路径段本身（含可能的额外子路径或 query）一律丢弃。
        return format!("{TELEGRAM_WEBHOOK_PREFIX}<redacted>");
    }
    path_or_uri.to_string()
}

// 标识符派生已下移到无依赖叶子模块 `crate::stable_id`（`jobs/model.rs` 会被
// 集成测试以 #[path] 直接编译，那条路径上没有 `utils`）。这里再导出，
// 既有的 `crate::utils::stable_id` 调用点保持不变。
pub use crate::stable_id::{stable_id, stable_id_bytes};

/// 监督一个长驻后台任务：panic 之后记录并重启，而不是永久静默死亡。
///
/// 背景：`src/` 里多处 `tokio::spawn(async move { loop { .. } })` 丢弃了
/// `JoinHandle`。tokio 会捕获任务内的 panic，但**没有人观察那个 JoinHandle**，
/// 于是循环永久消失，直到进程重启——下载监控、Telegram 长轮询、自动化事件投影、
/// 定时备份都属于这一类，静默死亡对自托管服务是不可接受的。
///
/// `make` 每次被调用都应返回一个全新的循环 future（通常通过克隆 `Arc` 捕获
/// 依赖），因为上一轮的 future 在 panic 时已被丢弃。
///
/// 行为约定：
/// - 循环**正常返回**：视为主动退出（例如通道关闭），不再重启；
/// - 循环 **panic**：ERROR 记录后等待 `restart_delay` 再重启；
/// - 重启不会无限加速：固定延迟，避免 panic 风暴打满 CPU。
pub fn spawn_supervised<F, Fut>(label: &'static str, mut make: F) -> tokio::task::JoinHandle<()>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match tokio::spawn(make()).await {
                Ok(()) => {
                    tracing::info!("{} 已正常结束，不再重启", label);
                    return;
                }
                Err(error) => {
                    // JoinError::is_panic 区分 panic 与取消；取消（例如运行时关闭）
                    // 不应重启。
                    if error.is_cancelled() {
                        tracing::info!("{} 被取消，不再重启", label);
                        return;
                    }
                    tracing::error!("{} 异常退出（panic），5 秒后重启: {}", label, error);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    })
}

/// 安装全局 panic hook，把 panic 连同位置写进结构化日志。
///
/// 默认 hook 只往 stderr 打印，容器里会和普通日志混在一起且没有级别；更关键的
/// 是没有任何一处代码在观察后台任务的 `JoinHandle`，panic 导致的"某个功能永久
/// 停止工作"在日志里几乎不可见。这里把 panic 提升为 ERROR 并带上文件:行号。
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "未知位置".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| (*value).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "非字符串 panic payload".to_string());
        tracing::error!(target: "panic", "线程 panic @ {}: {}", location, payload);
        previous(info);
    }));
}

pub fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();

    for index in 0..max_len {
        let l = left.get(index).copied().unwrap_or(0);
        let r = right.get(index).copied().unwrap_or(0);
        diff |= usize::from(l ^ r);
    }

    diff == 0
}

fn unique_tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("data");
    path.with_file_name(format!(
        ".{}.{}.{}.tmp",
        file_name,
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}

/// 清理崩溃残留的原子写入临时文件。`write_file_atomic` 在临时文件与 rename
/// 之间进程崩溃时会把 `.{name}.{pid}.{uuid}.tmp` 留在数据目录（含敏感数据），
/// 启动时统一扫描清理。
pub fn cleanup_stale_tmp_files(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name.ends_with(".tmp") {
            let path = entry.path();
            if path.is_file() {
                let _ = fs::remove_file(&path);
                tracing::warn!("已清理崩溃残留的临时文件: {}", path.display());
            }
        }
    }
}

#[cfg(unix)]
fn open_tmp_file(path: &Path, mode: u32) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)
        .map_err(|e| AppError::Database(format!("创建临时文件失败: {}", e)))
}

#[cfg(not(unix))]
fn open_tmp_file(path: &Path, _mode: u32) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| AppError::Database(format!("创建临时文件失败: {}", e)))
}

fn sync_parent_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(|e| AppError::Database(format!("同步目录失败: {}", e)))?;
        }
    }
    Ok(())
}

/// 人类可读的字节数。收敛自此前散落的 4 份实现（下载监控、夸克签到、
/// 在线更新、drive 测试副本），其中 `api/update.rs` 那份对不足 1KB 也用
/// 两位小数（`512.00 B`），这里统一取整数字节的写法。
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归测试：循环 panic 之后必须被重启，而不是永久消失。
    ///
    /// 全部长驻后台任务（下载监控、Telegram 长轮询、自动化事件投影、定时备份）
    /// 都靠这个语义；没有它时 tokio 会静默吞掉 panic，功能永久停止工作。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn supervised_task_restarts_after_panic() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = attempts.clone();
        let handle = spawn_supervised("测试任务", move || {
            let counter = counter.clone();
            async move {
                let attempt = counter.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    panic!("第一次迭代故意 panic");
                }
                // 第二次正常返回：监督器应视为主动退出并结束，不再重启。
            }
        });

        tokio::time::timeout(std::time::Duration::from_secs(20), handle)
            .await
            .expect("监督任务应在超时前结束")
            .expect("监督任务本身不应 panic");

        assert!(
            attempts.load(Ordering::SeqCst) >= 2,
            "panic 之后必须重启，实际只运行了 {} 次",
            attempts.load(Ordering::SeqCst)
        );
    }

    /// 正常返回的循环不应被重启（否则会把"主动退出"变成死循环）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn supervised_task_does_not_restart_after_clean_exit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = attempts.clone();
        let handle = spawn_supervised("干净退出", move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });

        tokio::time::timeout(std::time::Duration::from_secs(20), handle)
            .await
            .expect("监督任务应在超时前结束")
            .expect("监督任务本身不应 panic");
        assert_eq!(attempts.load(Ordering::SeqCst), 1, "正常返回不应触发重启");
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("my-media-sub-{}-{}", name, uuid::Uuid::new_v4()))
    }

    #[test]
    fn constant_time_eq_matches_exact_bytes_only() {
        assert!(constant_time_eq("abcdef", "abcdef"));
        assert!(!constant_time_eq("abcdef", "abcdeg"));
        assert!(!constant_time_eq("abcdef", "abc"));
        assert!(!constant_time_eq("abc", "abcdef"));
    }

    #[test]
    fn redact_log_path_hides_telegram_webhook_secret() {
        // 路径密钥不得出现在日志、追踪 span 或诊断里。
        assert_eq!(
            redact_log_path("/api/telegram/webhook/0123456789abcdef0123456789abcdef"),
            "/api/telegram/webhook/<redacted>"
        );
        // 额外子路径与 query 一并丢弃，不能让密钥从任何形式漏出。
        assert_eq!(
            redact_log_path("/api/telegram/webhook/secret/extra?token=secret"),
            "/api/telegram/webhook/<redacted>"
        );
        assert_eq!(
            redact_log_path("/api/telegram/webhook/"),
            "/api/telegram/webhook/<redacted>"
        );
        // 前缀相似但并非 webhook 的路径必须原样保留，否则会丢诊断信息。
        assert_eq!(
            redact_log_path("/api/telegram/audits"),
            "/api/telegram/audits"
        );
        assert_eq!(
            redact_log_path("/api/telegram/webhook"),
            "/api/telegram/webhook"
        );
        assert_eq!(
            redact_log_path("/api/subscriptions?limit=10"),
            "/api/subscriptions?limit=10"
        );
    }

    #[test]
    fn static_dir_uses_non_empty_override() {
        assert_eq!(static_dir_from_value(None), PathBuf::from("static"));
        assert_eq!(static_dir_from_value(Some("  ")), PathBuf::from("static"));
        assert_eq!(
            static_dir_from_value(Some(" /srv/my-media-sub/static ")),
            PathBuf::from("/srv/my-media-sub/static")
        );
    }

    #[test]
    fn write_file_atomic_overwrites_existing_file() {
        let dir = temp_path("atomic-write");
        let path = dir.join("nested").join("settings.json");

        write_file_atomic(&path, b"{\"version\":1}", 0o600).unwrap();
        write_file_atomic(&path, b"{\"version\":2}", 0o600).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"version\":2}");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let leftovers = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);

        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn redact_sensitive_covers_url_credential_params() {
        // 回归：TMDB 的 api_key 与夸克移动端的 kps/vcode/sign 是核心凭据，
        // reqwest 错误 Display 携带完整 URL 时不得带出。
        let value = redact_sensitive(
            "error sending request for url (https://api.themoviedb.org/3/search/tv?api_key=SECRET123&language=zh)",
        );
        assert!(!value.contains("SECRET123"));
        let value = redact_sensitive(
            "https://dl-cdn.quark.cn/1/growth/sign?pr=ucpro&kps=KPSVAL&sign=SIGNVAL&vcode=VCODEVAL",
        );
        assert!(!value.contains("KPSVAL"));
        assert!(!value.contains("SIGNVAL"));
        assert!(!value.contains("VCODEVAL"));
        // 普通单词不受影响（design= 不是 sign=）
        assert_eq!(redact_sensitive("design=modern"), "design=modern");
    }

    #[test]
    fn redact_sensitive_hides_query_headers_and_share_ids() {
        let value = redact_sensitive("https://pan.quark.cn/s/abc123?token=secret&x=1 Cookie: k=v");
        assert!(!value.contains("abc123"));
        assert!(!value.contains("token=secret"));
        assert!(!value.contains("k=v"));
        assert!(value.contains("[REDACTED]"));
    }

    #[test]
    fn format_bytes_uses_integers_below_one_kib_and_two_decimals_above() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
        // 超出 TB 后不再进位，避免出现没有单位可用的情况
        assert_eq!(
            format_bytes(2048_u64 * 1024 * 1024 * 1024 * 1024),
            "2048.00 TB"
        );
    }
}
