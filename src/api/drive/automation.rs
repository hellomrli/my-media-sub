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

    contexts
}

#[cfg(test)]
pub(super) fn download_completed_title_message(task: &Aria2Task) -> (String, String) {
    let file_name = if task.file_name.trim().is_empty() {
        task.gid.as_str()
    } else {
        task.file_name.trim()
    };
    let title = format!("下载完成: {}", file_name);
    let mut parts = vec![format!("文件：{}", file_name)];
    if !task.dir.trim().is_empty() {
        parts.push(format!("目录：{}", task.dir.trim()));
    }
    if task.total_length > 0 {
        parts.push(format!("大小：{}", format_bytes(task.total_length)));
    }
    let message = parts.join("\n");
    (title, message)
}

#[cfg(test)]
pub(super) fn completed_download_already_recorded(
    history: &[Notification],
    pushed_downloads: &HashSet<(String, String)>,
    task: &Aria2Task,
) -> bool {
    let (title, message) = download_completed_title_message(task);
    pushed_downloads.contains(&(title.clone(), message.clone()))
        || history.iter().any(|notification| {
            notification_matches_completed_download(notification, task, &title, &message)
        })
}

#[cfg(test)]
pub(super) fn notification_matches_completed_download(
    notification: &Notification,
    task: &Aria2Task,
    title: &str,
    message: &str,
) -> bool {
    if notification.event != PushEvent::DownloadCompleted.as_str() {
        return false;
    }
    if notification.meta.get("gid").and_then(Value::as_str) == Some(task.gid.as_str()) {
        return true;
    }
    if notification.title == title && notification.message == message {
        return true;
    }
    let same_file =
        notification.meta.get("file_name").and_then(Value::as_str) == Some(task.file_name.as_str());
    let same_dir = notification.meta.get("dir").and_then(Value::as_str) == Some(task.dir.as_str());
    let same_size = notification
        .meta
        .get("total_length")
        .and_then(Value::as_u64)
        == Some(task.total_length);
    same_file && same_dir && same_size
}

#[cfg(test)]
pub(super) fn subscription_id_for_download_gid(
    history: &[Notification],
    gid: &str,
) -> Option<String> {
    history
        .iter()
        .filter(|notification| notification.event == "subscription_transferred")
        .find_map(|notification| {
            let downloads = notification.meta.get("sync_downloads")?.as_array()?;
            let matched = downloads
                .iter()
                .any(|item| item.get("gid").and_then(Value::as_str) == Some(gid));
            if !matched {
                return None;
            }
            notification
                .meta
                .get("subscription_id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

#[cfg(test)]
pub(super) fn completed_subscription_download_files(
    history: &[Notification],
    subscription_id: &str,
    completed_gids: &HashSet<String>,
) -> Vec<String> {
    let mut files = history
        .iter()
        .filter(|notification| notification.event == "subscription_transferred")
        .filter(|notification| {
            notification
                .meta
                .get("subscription_id")
                .and_then(Value::as_str)
                == Some(subscription_id)
        })
        .filter_map(|notification| notification.meta.get("sync_downloads")?.as_array())
        .flat_map(|downloads| downloads.iter())
        .filter(|item| {
            item.get("gid")
                .and_then(Value::as_str)
                .map(|gid| completed_gids.contains(gid))
                .unwrap_or(false)
        })
        .filter_map(|item| item.get("file_name").and_then(Value::as_str))
        .filter(|file_name| !file_name.trim().is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
pub(super) fn format_bytes(bytes: u64) -> String {
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
