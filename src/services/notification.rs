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
use std::sync::Arc;

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
        let stores = (settings_store.clone(), notification_store.clone());
        let queue = job_queue.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(delay * 60)).await;
            let Ok(items) = stores.1.take_digest_pending().await else {
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
            if let Some(queue) = queue {
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
                let service = PushService::new(stores.0.get().await);
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
        });
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
