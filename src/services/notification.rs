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
    let PushDispatchRequest {
        notification_id,
        event,
        title,
        message,
        level,
    } = request;

    if let Some(job_queue) = job_queue {
        let settings = settings_store.get().await;
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
        )
        .await;
    });
}

async fn send_push_event(
    settings_store: &SettingsStore,
    notification_store: &NotificationStore,
    event: PushEvent,
    title: &str,
    message: &str,
    level: PushLevel,
    notification_id: Option<&str>,
) {
    let settings = settings_store.get().await;
    let push_service = PushService::new(settings);
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
