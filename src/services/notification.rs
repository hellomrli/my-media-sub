use std::collections::HashMap;

use serde_json::Value;
use tracing::{info, warn};

use crate::error::Result;
use crate::jobs::{JobQueue, PushDispatchPayload};
use crate::models::Notification;
use crate::services::push::{
    record_push_message_report_for_notification, PushEvent, PushLevel, PushRetryPolicy, PushService,
};
use crate::store::{NotificationStore, SettingsStore};
use crate::utils::unix_now;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 全局“摘要冲刷已排班”标志：同一时间进程内只保留一个摘要定时器，
/// 避免每条通知都各自 spawn 一个 sleep 任务。
static DIGEST_FLUSH_SCHEDULED: AtomicBool = AtomicBool::new(false);

pub async fn add_notification(
    notification_store: &NotificationStore,
    level: &str,
    event: &str,
    title: impl Into<String>,
    message: impl Into<String>,
    meta: HashMap<String, Value>,
) -> Result<Notification> {
    let notification = Notification {
        id: uuid::Uuid::new_v4().to_string(),
        level: level.to_string(),
        event: event.to_string(),
        title: title.into(),
        message: message.into(),
        meta,
        read: false,
        created_at: now(),
    };

    notification_store.add(notification).await
}

pub struct PushDispatchRequest {
    pub notification_id: Option<String>,
    pub subscription_id: Option<String>,
    pub event: PushEvent,
    pub title: String,
    pub message: String,
    pub level: PushLevel,
}

pub async fn dispatch_push_event(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    event: PushEvent,
    title: impl Into<String>,
    message: impl Into<String>,
    level: PushLevel,
) {
    dispatch_push_event_for_notification(
        settings_store,
        notification_store,
        job_queue,
        PushDispatchRequest {
            notification_id: None,
            subscription_id: None,
            event,
            title: title.into(),
            message: message.into(),
            level,
        },
    )
    .await;
}

pub async fn dispatch_push_event_for_notification(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    request: PushDispatchRequest,
) {
    // 所有渠道选择、持久化入队和直接发送都脱离核心自动化调用栈；调用方永远
    // 不会因推送 DNS、超时、磁盘或渠道错误而失败或延迟业务结果。
    tokio::spawn(async move {
        dispatch_push_event_impl(settings_store, notification_store, job_queue, request).await;
    });
}

async fn dispatch_push_event_impl(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    request: PushDispatchRequest,
) {
    let PushDispatchRequest {
        notification_id,
        subscription_id,
        event,
        title,
        message,
        level,
    } = request;

    // 注册全局 SettingsStore 句柄，供浏览器推送在遇到 404/410 时清理失效订阅。
    crate::services::push::register_settings_store_for_pruning(&settings_store);

    let settings = settings_store.get().await;
    if settings.push_dedup_window_seconds > 0
        && notification_store
            .recent_push_duplicate(
                event.as_str(),
                &title,
                &message,
                notification_id.as_deref(),
                now() - settings.push_dedup_window_seconds,
            )
            .await
    {
        info!("重复推送已在限频窗口内跳过: {}", event.as_str());
        return;
    }

    if settings.push_digest_enabled
        && event != PushEvent::NotificationDigest
        && notification_id.is_some()
    {
        if let Some(id) = notification_id.as_deref() {
            let _ = notification_store.mark_digest_pending(id).await;
        }
        let delay = settings.push_digest_window_minutes.clamp(1, 1_440) as u64;
        schedule_digest_flush(settings_store, notification_store, job_queue, delay);
        return;
    }

    if let Some(job_queue) = job_queue {
        let push_service = PushService::new(settings);
        if !push_service.event_enabled(event) || push_service.enabled_channels().is_empty() {
            return;
        }

        match job_queue
            .submit_push_dispatch(PushDispatchPayload {
                event: event.as_str().to_string(),
                title: title.clone(),
                message: message.clone(),
                level: level.as_str().to_string(),
                notification_id: notification_id.clone(),
                correlation_id: String::new(),
                subscription_id: subscription_id.clone(),
                episode: None,
            })
            .await
        {
            Ok(job) => {
                info!("已创建推送派发任务: {}", job.id);
                return;
            }
            Err(e) => {
                warn!("创建推送派发任务失败，回退为后台派发: {}", e);
            }
        }
    }

    tokio::spawn(async move {
        send_push_event(
            &settings_store,
            &notification_store,
            event,
            &title,
            &message,
            level,
            notification_id.as_deref(),
            subscription_id.as_deref(),
        )
        .await;
    });
}

#[allow(clippy::too_many_arguments)]
async fn send_push_event(
    settings_store: &SettingsStore,
    notification_store: &NotificationStore,
    event: PushEvent,
    title: &str,
    message: &str,
    level: PushLevel,
    notification_id: Option<&str>,
    subscription_id: Option<&str>,
) {
    let settings = settings_store.get().await;
    let push_service =
        PushService::new(settings).with_telegram_actions(event, notification_id, subscription_id);
    let report = push_service
        .send_event_with_retry_detailed(
            event,
            title,
            message,
            level,
            PushRetryPolicy::background_default(),
        )
        .await;

    record_push_message_report_for_notification(
        notification_store,
        notification_id,
        event.as_str(),
        title,
        message,
        level,
        &report,
    )
    .await;

    let failed = report.results.values().filter(|&&ok| !ok).count();
    if failed > 0 {
        warn!(
            "业务推送部分失败: {}/{} 个渠道失败",
            failed,
            report.results.len()
        );
    }
}

fn now() -> i64 {
    unix_now()
}

/// 排班一次摘要冲刷。进程内同一时间最多只有一个定时器：
/// 若已有定时器在等待窗口结束，则直接复用，返回 `false`。
fn schedule_digest_flush(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    delay_minutes: u64,
) -> bool {
    if DIGEST_FLUSH_SCHEDULED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return false;
    }
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(delay_minutes * 60)).await;
        // 先清标志再取待发通知：冲刷开始后新到的通知会重新排班下一个窗口。
        DIGEST_FLUSH_SCHEDULED.store(false, Ordering::SeqCst);
        flush_digest_pending(settings_store, notification_store, job_queue).await;
    });
    true
}

async fn flush_digest_pending(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
) {
    let Ok(items) = notification_store.take_digest_pending().await else {
        return;
    };
    if items.is_empty() {
        return;
    }
    let digest_message = items
        .iter()
        .take(20)
        .map(|item| format!("• [{}] {}", item.level, item.title))
        .collect::<Vec<_>>()
        .join("\n");
    let title = format!("通知摘要（{} 条）", items.len());
    let level = if items.iter().any(|item| item.level == "error") {
        PushLevel::Error
    } else {
        PushLevel::Info
    };
    if let Some(queue) = job_queue {
        let _ = queue
            .submit_push_dispatch(PushDispatchPayload {
                event: PushEvent::NotificationDigest.as_str().to_string(),
                title,
                message: digest_message,
                level: level.as_str().to_string(),
                notification_id: None,
                correlation_id: String::new(),
                subscription_id: None,
                episode: None,
            })
            .await;
    } else {
        let service = PushService::new(settings_store.get().await);
        let _ = service
            .send_event_with_retry_detailed(
                PushEvent::NotificationDigest,
                &title,
                &digest_message,
                level,
                PushRetryPolicy::background_default(),
            )
            .await;
    }
}

/// 启动恢复：进程重启会丢失内存中的摘要定时器，导致存储里已标记
/// `digest_pending` 的通知永远不会被推送。服务启动时扫描一次并补排冲刷。
pub fn recover_digest_pending_on_startup(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
) {
    tokio::spawn(async move {
        let settings = settings_store.get().await;
        if !settings.push_digest_enabled {
            return;
        }
        let has_pending = notification_store.list(true).await.iter().any(|item| {
            item.meta
                .get("digest_pending")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
        if !has_pending {
            return;
        }
        let delay = settings.push_digest_window_minutes.clamp(1, 1_440) as u64;
        if schedule_digest_flush(settings_store, notification_store, job_queue, delay) {
            info!("已恢复重启前遗留的通知摘要冲刷定时器");
        }
    });
}

#[cfg(test)]
mod digest_tests {
    use super::*;

    #[tokio::test]
    async fn only_one_digest_flush_timer_is_scheduled_per_window() {
        let dir =
            std::env::temp_dir().join(format!("my-media-sub-digest-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let settings_store = Arc::new(SettingsStore::new(dir.join("settings.json")));
        let notification_store = Arc::new(NotificationStore::new(dir.join("notifications.json")));

        // 保证测试独立于其他用例的全局标志状态。
        DIGEST_FLUSH_SCHEDULED.store(false, Ordering::SeqCst);
        assert!(schedule_digest_flush(
            settings_store.clone(),
            notification_store.clone(),
            None,
            1_440,
        ));
        assert!(!schedule_digest_flush(
            settings_store,
            notification_store,
            None,
            1_440,
        ));
        DIGEST_FLUSH_SCHEDULED.store(false, Ordering::SeqCst);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
