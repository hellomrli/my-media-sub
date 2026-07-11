use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;

use super::response::ApiResponse;
use crate::app::AppContext;
use crate::error::{AppError, Result};
use crate::jobs::JobStatus;
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
    let automation_events = context.automation_event_store.list(usize::MAX).await;
    let backups = context.backup_service.list_stored_backups().await?;
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
            automation_events: automation_events.len(),
        },
        queue: QueueDiagnostics {
            total: jobs.len(),
            queued,
            running,
            failed,
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
        },
        metrics: context.metrics.snapshot(),
        security: SecurityDiagnostics {
            password_configured: !settings.app_password.is_empty(),
            password_strength: password_strength(&settings.app_password),
            default_password_risk: is_default_password(&settings.app_password),
            csp_enabled: true,
        },
        storage_decision: evaluate_storage(StorageDecisionInput {
            subscriptions,
            jobs: jobs.len(),
            notifications: notifications.len(),
            automation_events: automation_events.len(),
            largest_store_bytes,
            complex_query_required: false,
        }),
    })
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
    fn password_strength_detects_defaults_and_strong_values() {
        assert_eq!(password_strength("change-me"), "weak");
        assert!(is_default_password("change-me"));
        assert_eq!(password_strength("Long-Password-2026!"), "strong");
        assert!(!is_default_password("Long-Password-2026!"));
    }
}
