use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::response::ApiResponse;
use crate::app::AppContext;
use crate::error::{AppError, Result};
use crate::jobs::reliability::{
    CIRCUIT_FAILURE_THRESHOLD, CIRCUIT_RECOVERY_SECONDS, JOB_BACKLOG_WARNING_THRESHOLD,
    JOB_STUCK_TIMEOUT_SECONDS, MAX_AUTO_ATTEMPTS,
};
use crate::jobs::{JobErrorClass, JobStatus};
use crate::services::storage::{evaluate_storage, StorageDecision, StorageDecisionInput};
use crate::store::schema::CURRENT_SCHEMA_VERSION;
use crate::utils::metrics::MetricsSnapshot;

#[derive(Serialize)]
struct DiagnosticsSnapshot {
    generated_at: i64,
    version: String,
    schema_version: u32,
    data: DataDiagnostics,
    queue: QueueDiagnostics,
    schedulers: SchedulerDiagnostics,
    external_services: BTreeMap<String, ExternalServiceDiagnostics>,
    backups: BackupDiagnostics,
    metrics: MetricsSnapshot,
    security: SecurityDiagnostics,
    telegram_bot: crate::services::telegram_bot::TelegramBotDiagnostics,
    environment: EnvironmentDiagnostics,
    recommendations: Vec<DiagnosticRecommendation>,
    storage_decision: StorageDecision,
}

#[derive(Serialize)]
struct DataDiagnostics {
    total_bytes: u64,
    files: BTreeMap<String, u64>,
    subscriptions: usize,
    notifications: usize,
    automation_events: usize,
}

#[derive(Serialize)]
struct QueueDiagnostics {
    total: usize,
    queued: usize,
    running: usize,
    failed: usize,
    delayed_retry: usize,
    retrying: usize,
    timed_out: usize,
    archived: usize,
    maintenance_mode: bool,
    backlog_warning: bool,
    retry_max_attempts: u32,
    stuck_timeout_seconds: u64,
    circuit_failure_threshold: u32,
    circuit_recovery_seconds: i64,
}

#[derive(Serialize)]
struct SchedulerDiagnostics {
    subscription_enabled: bool,
    subscription_running: bool,
    quark_signin_enabled: bool,
    quark_signin_running: bool,
}

#[derive(Serialize)]
struct ExternalServiceDiagnostics {
    configured: bool,
    status: &'static str,
}

#[derive(Serialize)]
struct BackupDiagnostics {
    count: usize,
    total_bytes: u64,
    retention: usize,
    max_archive_bytes: u64,
    max_storage_bytes: u64,
    verification_interval_seconds: u64,
    external_copy_configured: bool,
    latest_verification: Option<crate::services::backup::BackupVerificationReport>,
}

#[derive(Serialize)]
struct EnvironmentDiagnostics {
    filesystem: FilesystemDiagnostics,
    timezone: TimezoneDiagnostics,
    dns: Vec<DnsDiagnostics>,
    data_consistency: Vec<DataConsistencyDiagnostics>,
}

#[derive(Serialize)]
struct FilesystemDiagnostics {
    data_dir_exists: bool,
    readable: bool,
    writable_hint: bool,
    total_bytes: Option<u64>,
    available_bytes: Option<u64>,
    available_percent: Option<f64>,
}

#[derive(Serialize)]
struct TimezoneDiagnostics {
    tz_env: Option<String>,
    local_offset_seconds: i32,
    expected_offset_seconds: i32,
    matches_expected: bool,
}

#[derive(Serialize)]
struct DnsDiagnostics {
    host: String,
    status: &'static str,
    latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Serialize)]
struct DataConsistencyDiagnostics {
    store: String,
    status: &'static str,
    bytes: u64,
    schema_version: Option<u64>,
    expected_records: Option<usize>,
    actual_records: Option<usize>,
    message: String,
}

#[derive(Serialize)]
struct DiagnosticRecommendation {
    severity: &'static str,
    category: &'static str,
    code: &'static str,
    summary: String,
    action: String,
}

#[derive(Serialize)]
struct SecurityDiagnostics {
    password_configured: bool,
    password_strength: &'static str,
    default_password_risk: bool,
    csp_enabled: bool,
}

async fn diagnostics(State(context): State<Arc<AppContext>>) -> Result<impl IntoResponse> {
    Ok(Json(ApiResponse::ok(build_snapshot(&context).await?)))
}

async fn export_diagnostics(State(context): State<Arc<AppContext>>) -> Result<Response> {
    let snapshot = build_snapshot(&context).await?;
    let bytes = serde_json::to_vec_pretty(&snapshot)?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"my-media-sub-diagnostics.json\""),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn build_snapshot(context: &AppContext) -> Result<DiagnosticsSnapshot> {
    let settings = context.settings_store.get().await;
    let jobs = context.job_store.list().await;
    let notifications = context.notification_store.list(true).await;
    let subscriptions = context.subscription_store.count().await;
    let automation_event_count = context.automation_event_store.count().await;
    let backups = context.backup_service.list_stored_backups().await?;
    let latest_backup_verification = context.backup_service.latest_verification().await?;
    let files = diagnostic_file_sizes(context.backup_service.data_dir())?;
    let total_bytes = files.values().copied().sum();
    let largest_store_bytes = files.values().copied().max().unwrap_or(0);
    let queued = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Queued)
        .count();
    let running = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Running)
        .count();
    let failed = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Failed)
        .count();
    let delayed_retry = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Queued && job.next_attempt_at.is_some())
        .count();
    let retrying = jobs.iter().filter(|job| job.attempt > 1).count();
    let timed_out = jobs
        .iter()
        .filter(|job| job.error_class == Some(JobErrorClass::TimedOut))
        .count();
    let archived = context.job_store.archived_count()?;
    let telegram_bot = context.telegram_bot.diagnostics().await;
    context
        .metrics
        .set_job_queue_depth((queued + running) as u64);

    let mut external_services = BTreeMap::new();
    for (name, configured) in [
        ("quark", !settings.quark_cookie.trim().is_empty()),
        ("aria2", !settings.aria2_rpc_url.trim().is_empty()),
        ("pansou", !settings.pansou_api_url.trim().is_empty()),
        ("tmdb", !settings.tmdb_api_key.trim().is_empty()),
        ("telegram", !settings.telegram_bot_token.trim().is_empty()),
        ("gotify", !settings.gotify_url.trim().is_empty()),
    ] {
        external_services.insert(
            name.to_string(),
            ExternalServiceDiagnostics {
                configured,
                status: if configured {
                    "configured"
                } else {
                    "not_configured"
                },
            },
        );
    }

    let data_dir = context.backup_service.data_dir();
    let filesystem = filesystem_diagnostics(data_dir);
    let timezone = timezone_diagnostics();
    let data_consistency = data_consistency_diagnostics(
        data_dir,
        &[
            ("settings", None),
            ("subscriptions", Some(subscriptions)),
            ("notifications", Some(notifications.len())),
            ("jobs", Some(jobs.len())),
            ("automation_events", Some(automation_event_count)),
            ("telegram_bot", None),
        ],
    );
    let dns = dns_diagnostics(&settings).await;
    let recommendations =
        diagnostic_recommendations(&filesystem, &timezone, &dns, &data_consistency);
    let environment = EnvironmentDiagnostics {
        filesystem,
        timezone,
        dns,
        data_consistency,
    };

    let policy = context.backup_service.policy();
    Ok(DiagnosticsSnapshot {
        generated_at: crate::utils::unix_now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: CURRENT_SCHEMA_VERSION,
        data: DataDiagnostics {
            total_bytes,
            files,
            subscriptions,
            notifications: notifications.len(),
            automation_events: automation_event_count,
        },
        queue: QueueDiagnostics {
            total: jobs.len(),
            queued,
            running,
            failed,
            delayed_retry,
            retrying,
            timed_out,
            archived,
            maintenance_mode: settings.job_maintenance_mode,
            backlog_warning: queued >= JOB_BACKLOG_WARNING_THRESHOLD,
            retry_max_attempts: MAX_AUTO_ATTEMPTS,
            stuck_timeout_seconds: JOB_STUCK_TIMEOUT_SECONDS,
            circuit_failure_threshold: CIRCUIT_FAILURE_THRESHOLD,
            circuit_recovery_seconds: CIRCUIT_RECOVERY_SECONDS,
        },
        schedulers: SchedulerDiagnostics {
            subscription_enabled: settings.subscription_scheduler_enabled,
            subscription_running: context.scheduler.is_running().await,
            quark_signin_enabled: settings.quark_signin_enabled,
            quark_signin_running: context.quark_signin_scheduler.is_running().await,
        },
        external_services,
        backups: BackupDiagnostics {
            count: backups.len(),
            total_bytes: backups.iter().map(|backup| backup.size).sum(),
            retention: policy.retention,
            max_archive_bytes: policy.max_archive_bytes,
            max_storage_bytes: policy.max_storage_bytes,
            verification_interval_seconds: policy.verification_interval.as_secs(),
            external_copy_configured: policy.external_dir.is_some(),
            latest_verification: latest_backup_verification,
        },
        metrics: context.metrics.snapshot(),
        security: SecurityDiagnostics {
            password_configured: !settings.app_password.is_empty(),
            password_strength: password_strength(&settings.app_password),
            default_password_risk: is_default_password(&settings.app_password),
            csp_enabled: true,
        },
        telegram_bot,
        environment,
        recommendations,
        storage_decision: evaluate_storage(StorageDecisionInput {
            subscriptions,
            jobs: jobs.len(),
            notifications: notifications.len(),
            automation_events: automation_event_count,
            largest_store_bytes,
            complex_query_required: false,
        }),
    })
}

fn filesystem_diagnostics(data_dir: &std::path::Path) -> FilesystemDiagnostics {
    let data_dir_exists = data_dir.is_dir();
    let readable = data_dir_exists && std::fs::read_dir(data_dir).is_ok();
    let writable_hint = std::fs::metadata(data_dir)
        .map(|metadata| permission_has_write_bit(&metadata.permissions()))
        .unwrap_or(false);
    let (total_bytes, available_bytes) = filesystem_space(data_dir)
        .map(|(total, available)| (Some(total), Some(available)))
        .unwrap_or((None, None));
    let available_percent = total_bytes
        .zip(available_bytes)
        .filter(|(total, _)| *total > 0)
        .map(|(total, available)| available as f64 * 100.0 / total as f64);
    FilesystemDiagnostics {
        data_dir_exists,
        readable,
        writable_hint,
        total_bytes,
        available_bytes,
        available_percent,
    }
}

#[cfg(unix)]
fn permission_has_write_bit(permissions: &std::fs::Permissions) -> bool {
    use std::os::unix::fs::PermissionsExt;
    permissions.mode() & 0o222 != 0
}

#[cfg(not(unix))]
fn permission_has_write_bit(permissions: &std::fs::Permissions) -> bool {
    !permissions.readonly()
}

#[cfg(unix)]
fn filesystem_space(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a valid NUL-terminated path and `stats` points to writable memory.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statvfs returned success and initialized the structure.
    let stats = unsafe { stats.assume_init() };
    let block_size = stats.f_frsize;
    Some((
        stats.f_blocks.saturating_mul(block_size),
        stats.f_bavail.saturating_mul(block_size),
    ))
}

#[cfg(not(unix))]
fn filesystem_space(_path: &std::path::Path) -> Option<(u64, u64)> {
    None
}

fn timezone_diagnostics() -> TimezoneDiagnostics {
    let local_offset_seconds = chrono::Local::now().offset().local_minus_utc();
    let expected_offset_seconds = 8 * 60 * 60;
    TimezoneDiagnostics {
        tz_env: std::env::var("TZ")
            .ok()
            .filter(|value| !value.trim().is_empty()),
        local_offset_seconds,
        expected_offset_seconds,
        matches_expected: local_offset_seconds == expected_offset_seconds,
    }
}

fn data_consistency_diagnostics(
    data_dir: &std::path::Path,
    stores: &[(&str, Option<usize>)],
) -> Vec<DataConsistencyDiagnostics> {
    stores
        .iter()
        .map(|(store, expected_records)| {
            let path = data_dir.join(format!("{store}.json"));
            let Ok(bytes) = std::fs::read(&path) else {
                let has_loaded_data = expected_records.is_some_and(|count| count > 0);
                return DataConsistencyDiagnostics {
                    store: (*store).to_string(),
                    status: if has_loaded_data {
                        "missing_with_data"
                    } else {
                        "missing"
                    },
                    bytes: 0,
                    schema_version: None,
                    expected_records: *expected_records,
                    actual_records: None,
                    message: if has_loaded_data {
                        "磁盘文件缺失，但内存中仍有已加载记录".to_string()
                    } else {
                        "文件尚未创建；空数据实例可忽略".to_string()
                    },
                };
            };
            let size = bytes.len() as u64;
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return DataConsistencyDiagnostics {
                    store: (*store).to_string(),
                    status: "invalid_json",
                    bytes: size,
                    schema_version: None,
                    expected_records: *expected_records,
                    actual_records: None,
                    message: "JSON 无法解析，请从备份恢复".to_string(),
                };
            };
            let schema_version = value.get("schema_version").and_then(|value| value.as_u64());
            let data = value.get("data").unwrap_or(&value);
            let actual_records = data.as_array().map(Vec::len);
            let version_supported =
                schema_version.is_none_or(|version| version <= u64::from(CURRENT_SCHEMA_VERSION));
            let count_matches = expected_records
                .zip(actual_records)
                .is_none_or(|(expected, actual)| expected == actual);
            let (status, message) = if !version_supported {
                ("unsupported_schema", "文件 schema 高于当前程序支持版本")
            } else if !count_matches {
                ("count_mismatch", "磁盘记录数与内存快照不一致")
            } else {
                ("ok", "结构与当前内存快照一致")
            };
            DataConsistencyDiagnostics {
                store: (*store).to_string(),
                status,
                bytes: size,
                schema_version,
                expected_records: *expected_records,
                actual_records,
                message: message.to_string(),
            }
        })
        .collect()
}

async fn dns_diagnostics(settings: &crate::models::Settings) -> Vec<DnsDiagnostics> {
    let mut hosts = BTreeSet::new();
    if !settings.quark_cookie.trim().is_empty() {
        hosts.insert("pan.quark.cn".to_string());
    }
    if !settings.tmdb_api_key.trim().is_empty() {
        hosts.insert("api.themoviedb.org".to_string());
    }
    for url in [
        settings.pansou_api_url.as_str(),
        settings.aria2_rpc_url.as_str(),
        settings.gotify_url.as_str(),
    ] {
        if let Ok(url) = reqwest::Url::parse(url) {
            if let Some(host) = url.host_str() {
                hosts.insert(host.to_string());
            }
        }
    }
    let mut results = Vec::new();
    for host in hosts.into_iter().take(8) {
        let started = std::time::Instant::now();
        let lookup = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            tokio::net::lookup_host(format!("{host}:443")),
        )
        .await;
        let (status, message) = match lookup {
            Ok(Ok(mut addresses)) => {
                if addresses.next().is_some() {
                    ("ok", None)
                } else {
                    ("empty", Some("DNS 未返回地址".to_string()))
                }
            }
            Ok(Err(error)) => ("failed", Some(error.to_string())),
            Err(_) => ("timeout", Some("DNS 查询超过 3 秒".to_string())),
        };
        results.push(DnsDiagnostics {
            host,
            status,
            latency_ms: started.elapsed().as_millis() as u64,
            message,
        });
    }
    results
}

fn diagnostic_recommendations(
    filesystem: &FilesystemDiagnostics,
    timezone: &TimezoneDiagnostics,
    dns: &[DnsDiagnostics],
    consistency: &[DataConsistencyDiagnostics],
) -> Vec<DiagnosticRecommendation> {
    let mut recommendations = Vec::new();
    if !filesystem.data_dir_exists || !filesystem.readable || !filesystem.writable_hint {
        recommendations.push(DiagnosticRecommendation {
            severity: "error",
            category: "filesystem",
            code: "data_dir_permissions",
            summary: "DATA_DIR 不存在或权限不足".to_string(),
            action: "确认目录已挂载，并授予运行用户读写权限；不要在诊断过程中自动修改权限"
                .to_string(),
        });
    }
    if filesystem
        .available_percent
        .is_some_and(|percent| percent < 10.0)
    {
        recommendations.push(DiagnosticRecommendation {
            severity: "warning",
            category: "filesystem",
            code: "disk_space_low",
            summary: "DATA_DIR 所在磁盘可用空间低于 10%".to_string(),
            action: "清理旧备份和无用日志，扩容后再执行转存或恢复".to_string(),
        });
    }
    if !timezone.matches_expected {
        recommendations.push(DiagnosticRecommendation {
            severity: "warning",
            category: "timezone",
            code: "timezone_offset_mismatch",
            summary: format!(
                "当前 UTC 偏移为 {} 秒，预期为 +28800 秒",
                timezone.local_offset_seconds
            ),
            action: "将 TZ 设置为 Asia/Shanghai，并重启服务以保证调度时间一致".to_string(),
        });
    }
    if dns.iter().any(|entry| entry.status != "ok") {
        recommendations.push(DiagnosticRecommendation {
            severity: "warning",
            category: "dns",
            code: "dns_resolution_failed",
            summary: "一个或多个已配置外部服务无法完成 DNS 解析".to_string(),
            action: "检查容器 DNS、代理和防火墙；修复后重新运行只读诊断".to_string(),
        });
    }
    if consistency
        .iter()
        .any(|entry| !matches!(entry.status, "ok" | "missing"))
    {
        recommendations.push(DiagnosticRecommendation {
            severity: "error",
            category: "data",
            code: "store_consistency_failed",
            summary: "一个或多个 JSON Store 存在结构或记录数异常".to_string(),
            action: "先导出诊断与备份，再使用备份预览确认可恢复性；不要直接编辑线上 JSON"
                .to_string(),
        });
    }
    if recommendations.is_empty() {
        recommendations.push(DiagnosticRecommendation {
            severity: "info",
            category: "system",
            code: "no_action_required",
            summary: "只读环境与数据检查未发现需处理问题".to_string(),
            action: "无需操作；可保留诊断快照用于后续对比".to_string(),
        });
    }
    recommendations
}

fn diagnostic_file_sizes(data_dir: &std::path::Path) -> Result<BTreeMap<String, u64>> {
    let mut files = BTreeMap::new();
    if !data_dir.exists() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(data_dir)
        .map_err(|error| AppError::Database(format!("读取 DATA_DIR 失败: {error}")))?
    {
        let entry =
            entry.map_err(|error| AppError::Database(format!("读取数据文件失败: {error}")))?;
        if entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            files.insert(name, size);
        }
    }
    Ok(files)
}

pub(crate) fn password_strength(password: &str) -> &'static str {
    if password.is_empty() || password.len() < 12 {
        return "weak";
    }
    let classes = [
        password
            .chars()
            .any(|character| character.is_ascii_lowercase()),
        password
            .chars()
            .any(|character| character.is_ascii_uppercase()),
        password.chars().any(|character| character.is_ascii_digit()),
        password
            .chars()
            .any(|character| !character.is_alphanumeric()),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if password.len() >= 16 && classes >= 3 {
        "strong"
    } else if classes >= 2 {
        "medium"
    } else {
        "weak"
    }
}

pub(crate) fn is_default_password(password: &str) -> bool {
    matches!(password, "change-me" | "admin" | "password" | "123456")
}

pub fn routes(context: Arc<AppContext>) -> Router {
    Router::new()
        .route("/api/diagnostics", get(diagnostics))
        .route("/api/diagnostics/export", get(export_diagnostics))
        .with_state(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consistency_reports_invalid_json_without_modifying_it() {
        let dir =
            std::env::temp_dir().join(format!("diagnostics-invalid-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("jobs.json"), b"not json").unwrap();
        let checks = data_consistency_diagnostics(&dir, &[("jobs", Some(0))]);
        assert_eq!(checks[0].status, "invalid_json");
        assert_eq!(std::fs::read(dir.join("jobs.json")).unwrap(), b"not json");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn recommendations_cover_disk_timezone_dns_and_store_failures() {
        let filesystem = FilesystemDiagnostics {
            data_dir_exists: true,
            readable: true,
            writable_hint: true,
            total_bytes: Some(100),
            available_bytes: Some(5),
            available_percent: Some(5.0),
        };
        let timezone = TimezoneDiagnostics {
            tz_env: Some("UTC".into()),
            local_offset_seconds: 0,
            expected_offset_seconds: 28_800,
            matches_expected: false,
        };
        let dns = vec![DnsDiagnostics {
            host: "example.invalid".into(),
            status: "failed",
            latency_ms: 1,
            message: None,
        }];
        let consistency = vec![DataConsistencyDiagnostics {
            store: "jobs".into(),
            status: "invalid_json",
            bytes: 1,
            schema_version: None,
            expected_records: Some(0),
            actual_records: None,
            message: String::new(),
        }];
        let items = diagnostic_recommendations(&filesystem, &timezone, &dns, &consistency);
        assert!(items.iter().any(|item| item.code == "disk_space_low"));
        assert!(items
            .iter()
            .any(|item| item.code == "timezone_offset_mismatch"));
        assert!(items
            .iter()
            .any(|item| item.code == "dns_resolution_failed"));
        assert!(items
            .iter()
            .any(|item| item.code == "store_consistency_failed"));
    }

    #[test]
    fn password_strength_detects_defaults_and_strong_values() {
        assert_eq!(password_strength("change-me"), "weak");
        assert!(is_default_password("change-me"));
        assert_eq!(password_strength("Long-Password-2026!"), "strong");
        assert!(!is_default_password("Long-Password-2026!"));
    }
}
