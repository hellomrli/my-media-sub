use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

use super::response::ApiResponse;
use crate::app::AppContext;
use crate::error::{AppError, Result};
use crate::services::storage::{
    build_cleanup_preview, cleanup_store_preview, evaluate_storage, file_sizes, RetentionPolicy,
    StorageCleanupPreview, StorageDecision, StorageDecisionInput,
};

static CLEANUP_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Deserialize)]
struct StorageCompactRequest {
    #[serde(default)]
    confirmation: String,
}

#[derive(Serialize)]
struct StorageCompactResult {
    snapshot_backup: String,
    removed_subscription_history: usize,
    removed_notifications: usize,
    archived_active_jobs: usize,
    removed_archived_jobs: usize,
    removed_automation_events: usize,
    preview_before: StorageCleanupPreview,
}

async fn cleanup_preview(
    State(context): State<Arc<AppContext>>,
) -> Result<Json<ApiResponse<StorageCleanupPreview>>> {
    Ok(Json(ApiResponse::ok(build_preview(&context).await?)))
}

async fn storage_decision(
    State(context): State<Arc<AppContext>>,
) -> Result<Json<ApiResponse<StorageDecision>>> {
    Ok(Json(ApiResponse::ok(
        build_preview(&context).await?.sqlite_decision,
    )))
}

async fn cleanup_storage(
    State(context): State<Arc<AppContext>>,
    Json(request): Json<StorageCompactRequest>,
) -> Result<Json<ApiResponse<StorageCompactResult>>> {
    if request.confirmation != "CLEANUP DATA" {
        return Err(AppError::Validation(
            "清理确认文本必须为 CLEANUP DATA".to_string(),
        ));
    }
    execute_cleanup(&context).await
}

async fn compact_storage(
    State(context): State<Arc<AppContext>>,
    Json(request): Json<StorageCompactRequest>,
) -> Result<Json<ApiResponse<StorageCompactResult>>> {
    if request.confirmation != "COMPACT JSON" {
        return Err(AppError::Validation(
            "整理确认文本必须为 COMPACT JSON".to_string(),
        ));
    }
    execute_cleanup(&context).await
}

async fn execute_cleanup(
    context: &Arc<AppContext>,
) -> Result<Json<ApiResponse<StorageCompactResult>>> {
    let _guard = CLEANUP_LOCK.lock().await;
    let preview_before = build_preview(context).await?;
    let policy = &preview_before.policy;
    let snapshot = context
        .backup_service
        .create_stored_backup("pre-cleanup")
        .await?;
    let removed_subscription_history = context
        .subscription_store
        .compact_with_retention(
            policy.subscription_check_history,
            policy.subscription_source_switch_history,
            policy.subscription_previous_links,
        )
        .await?;
    let removed_notifications = context
        .notification_store
        .compact_to(policy.notifications)
        .await?;
    let archived_active_jobs = context
        .job_store
        .archive_completed(policy.active_terminal_jobs)
        .await?;
    let removed_archived_jobs = context
        .job_store
        .prune_archive(policy.archived_jobs)
        .await?;
    let removed_automation_events = context
        .automation_event_store
        .compact_with_retention(
            policy.automation_normal_days,
            policy.automation_failed_days,
            policy.automation_events,
        )
        .await?;
    context.settings_store.compact().await?;
    Ok(Json(ApiResponse::with_message(
        StorageCompactResult {
            snapshot_backup: snapshot.name,
            removed_subscription_history,
            removed_notifications,
            archived_active_jobs,
            removed_archived_jobs,
            removed_automation_events,
            preview_before,
        },
        "已先创建并验证备份，再按各 Store 独立保留策略清理紧凑 JSON",
    )))
}

async fn build_preview(context: &AppContext) -> Result<StorageCleanupPreview> {
    let policy = RetentionPolicy::from_env();
    let sizes = file_sizes(context.backup_service.data_dir());
    let subscription_count = context.subscription_store.count().await;
    let (checks, switches, previous) = context.subscription_store.history_counts().await;
    let subscription_histories = checks.saturating_add(switches).saturating_add(previous);
    let subscription_remove = context
        .subscription_store
        .preview_history_retention(
            policy.subscription_check_history,
            policy.subscription_source_switch_history,
            policy.subscription_previous_links,
        )
        .await;
    let notification_count = context.notification_store.count().await;
    let jobs = context.job_store.list().await;
    let terminal_jobs = context.job_store.terminal_count().await;
    let archived_jobs = context.job_store.archived_count().await?;
    let jobs_to_archive = terminal_jobs.saturating_sub(policy.active_terminal_jobs);
    let projected_archive = archived_jobs.saturating_add(jobs_to_archive);
    let automation_count = context.automation_event_store.count().await;
    let automation_remove = context
        .automation_event_store
        .preview_retention(
            policy.automation_normal_days,
            policy.automation_failed_days,
            policy.automation_events,
        )
        .await;
    let warning_bytes = policy.growth_warning_bytes;
    let stores = vec![
        cleanup_store_preview(
            "subscription_histories",
            subscription_histories,
            subscription_histories.saturating_sub(subscription_remove),
            subscription_count.saturating_mul(
                policy.subscription_check_history
                    + policy.subscription_source_switch_history
                    + policy.subscription_previous_links,
            ),
            *sizes.get("subscriptions.json").unwrap_or(&0),
            warning_bytes,
        ),
        cleanup_store_preview(
            "notifications",
            notification_count,
            notification_count.min(policy.notifications),
            policy.notifications,
            *sizes.get("notifications.json").unwrap_or(&0),
            warning_bytes,
        ),
        cleanup_store_preview(
            "active_job_history",
            terminal_jobs,
            terminal_jobs.min(policy.active_terminal_jobs),
            policy.active_terminal_jobs,
            *sizes.get("jobs.json").unwrap_or(&0),
            warning_bytes,
        ),
        cleanup_store_preview(
            "archived_jobs",
            projected_archive,
            projected_archive.min(policy.archived_jobs),
            policy.archived_jobs,
            *sizes.get("jobs.archive.json").unwrap_or(&0),
            warning_bytes,
        ),
        cleanup_store_preview(
            "automation_events",
            automation_count,
            automation_count.saturating_sub(automation_remove),
            policy.automation_events,
            *sizes.get("automation_events.json").unwrap_or(&0),
            warning_bytes,
        ),
    ];
    let largest_store_bytes = sizes.values().copied().max().unwrap_or(0);
    let sqlite_decision = evaluate_storage(StorageDecisionInput {
        subscriptions: subscription_count,
        jobs: jobs.len().saturating_add(archived_jobs),
        notifications: notification_count,
        automation_events: automation_count,
        largest_store_bytes,
        complex_query_required: false,
    });
    Ok(build_cleanup_preview(policy, stores, sqlite_decision))
}

pub fn routes(context: Arc<AppContext>) -> Router {
    Router::new()
        .route("/api/storage/compact", post(compact_storage))
        .route(
            "/api/storage/cleanup",
            get(cleanup_preview).post(cleanup_storage),
        )
        .route("/api/storage/decision", get(storage_decision))
        .with_state(context)
}
