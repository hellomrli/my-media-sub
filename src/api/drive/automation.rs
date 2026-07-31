use super::*;

pub(super) fn aria2_automation_contexts(
    notifications: &[Notification],
    subscriptions: &[Subscription],
) -> HashMap<String, Aria2AutomationContext> {
    let subscription_titles = subscriptions
        .iter()
        .map(|subscription| (subscription.id.as_str(), subscription.title.as_str()))
        .collect::<HashMap<_, _>>();
    let mut contexts = HashMap::new();

    // NotificationStore::list returns newest-first, so the first context wins.
    for notification in notifications
        .iter()
        .filter(|notification| notification.event == "subscription_transferred")
    {
        let Some(subscription_id) = notification
            .meta
            .get("subscription_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let subscription_title = notification
            .meta
            .get("subscription_title")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .or_else(|| subscription_titles.get(subscription_id).copied())
            .unwrap_or("未命名订阅")
            .to_string();
        let target_dir = notification
            .meta
            .get("target_dir")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let strm_status = if notification
            .meta
            .get("strm_error")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
            || notification.message.contains("STRM 生成失败")
        {
            "failed"
        } else if notification
            .meta
            .get("strm_generated_count")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0)
            || (notification.message.contains("STRM")
                && notification.message.contains("生成")
                && !notification.message.contains("生成失败"))
        {
            "generated"
        } else {
            "not_recorded"
        };
        let Some(downloads) = notification
            .meta
            .get("sync_downloads")
            .and_then(Value::as_array)
        else {
            continue;
        };
        for download in downloads {
            let Some(gid) = download
                .get("gid")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            else {
                continue;
            };
            let file_name = download
                .get("file_name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            contexts
                .entry(gid.to_string())
                .or_insert_with(|| Aria2AutomationContext {
                    subscription_id: subscription_id.to_string(),
                    subscription_title: subscription_title.clone(),
                    target_dir: target_dir.clone(),
                    submitted_at: notification.created_at,
                    episode: crate::services::detect_episode(file_name).episode,
                    transfer_status: "completed".to_string(),
                    rename_status: "completed".to_string(),
                    strm_status: strm_status.to_string(),
                });
        }
    }

    // 通知可能被用户清理；持久化在订阅中的下载记录作为权威兜底。
    // 若通知仍存在，则保留其中更丰富的 STRM/重命名展示信息。
    for subscription in subscriptions {
        for download in &subscription.sync_downloads {
            if download.gid.trim().is_empty() {
                continue;
            }
            contexts
                .entry(download.gid.clone())
                .or_insert_with(|| Aria2AutomationContext {
                    subscription_id: subscription.id.clone(),
                    subscription_title: subscription.title.clone(),
                    target_dir: download.target_dir.clone(),
                    submitted_at: download.submitted_at,
                    episode: crate::services::detect_episode(&download.file_name).episode,
                    transfer_status: "completed".to_string(),
                    rename_status: "completed".to_string(),
                    strm_status: "not_recorded".to_string(),
                });
        }
    }

    contexts
}
