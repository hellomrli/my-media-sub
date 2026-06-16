use std::collections::HashMap;

use serde_json::Value;
use tracing::warn;

use crate::error::Result;
use crate::models::Notification;
use crate::services::push::{record_push_message, PushEvent, PushLevel, PushService};
use crate::store::{NotificationStore, SettingsStore};
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

pub fn dispatch_push_event(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    event: PushEvent,
    title: impl Into<String>,
    message: impl Into<String>,
    level: PushLevel,
) {
    let title = title.into();
    let message = message.into();

    tokio::spawn(async move {
        send_push_event(
            &settings_store,
            &notification_store,
            event,
            &title,
            &message,
            level,
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
) {
    let settings = settings_store.get().await;
    let push_service = PushService::new(settings);
    let results = push_service.send_event(event, title, message, level).await;

    record_push_message(
        notification_store,
        event.as_str(),
        title,
        message,
        level,
        &results,
    )
    .await;

    let failed = results.values().filter(|&&ok| !ok).count();
    if failed > 0 {
        warn!("业务推送部分失败: {}/{} 个渠道失败", failed, results.len());
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
