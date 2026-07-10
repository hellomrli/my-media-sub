macro_rules! subscription_transfer_notification_methods {
    () => {
    async fn send_completed_notification(&self, sub: &Subscription) {
        let total = completion_target_episode(sub).unwrap_or(sub.current_episode_number);
        let title = format!("订阅已完结: {}", sub.title);
        let message = if total > 0 && sub.sync_download_enabled {
            format!("已转存并提交下载到第 {} 集", total)
        } else if total > 0 {
            format!("已转存到第 {} 集", total)
        } else {
            "订阅已标记为完结".to_string()
        };

        let notification = add_notification(
            &self.notification_store,
            "success",
            "subscription_completed",
            title.clone(),
            message.clone(),
            std::collections::HashMap::new(),
        )
        .await;
        dispatch_push_event_for_notification(
            self.settings_store.clone(),
            self.notification_store.clone(),
            None,
            PushDispatchRequest {
                notification_id: notification.ok().map(|notification| notification.id),
                event: PushEvent::SubscriptionCompleted,
                title,
                message,
                level: PushLevel::Success,
            },
        )
        .await;
    }

    /// 发送转存通知
    async fn send_transfer_notification(
        &self,
        sub: &Subscription,
        file_names: &[String],
        target_dir: &str,
        sync_report: Option<&SyncDownloadReport>,
        strm_report: Option<&StrmGenerationReport>,
    ) -> (String, String, Option<String>) {
        let target_dir_label = if target_dir.is_empty() {
            "根目录"
        } else {
            target_dir
        };
        let message = transfer_notification_message(
            file_names.len(),
            target_dir_label,
            sync_report,
            strm_report,
        );
        let mut meta = std::collections::HashMap::from([
            (
                "mode".to_string(),
                serde_json::Value::String("auto".to_string()),
            ),
            (
                "subscription_id".to_string(),
                serde_json::Value::String(sub.id.clone()),
            ),
            (
                "subscription_title".to_string(),
                serde_json::Value::String(sub.title.clone()),
            ),
            (
                "target_dir".to_string(),
                serde_json::Value::String(target_dir_label.to_string()),
            ),
            (
                "saved_count".to_string(),
                serde_json::Value::Number(serde_json::Number::from(file_names.len())),
            ),
            (
                "file_names".to_string(),
                serde_json::Value::Array(
                    file_names
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            ),
        ]);
        if let Some(report) = sync_report {
            meta.insert(
                "sync_download_dir".to_string(),
                serde_json::Value::String(report.dir.clone()),
            );
            meta.insert(
                "sync_downloads".to_string(),
                serde_json::Value::Array(
                    report
                        .items
                        .iter()
                        .map(|item| {
                            serde_json::json!({
                                "gid": item.gid,
                                "file_name": item.file_name,
                            })
                        })
                        .collect(),
                ),
            );
        }
        if let Some(report) = strm_report {
            meta.insert(
                "strm_generated_count".to_string(),
                serde_json::json!(report.generated_count),
            );
            meta.insert(
                "strm_dir".to_string(),
                serde_json::Value::String(report.dir.clone()),
            );
            if let Some(error) = &report.error {
                meta.insert(
                    "strm_error".to_string(),
                    serde_json::Value::String(error.clone()),
                );
            }
        }

        let title = format!("订阅自动转存: {}", sub.title);
        let notification = add_notification(
            &self.notification_store,
            "success",
            "subscription_transferred",
            title.clone(),
            message.clone(),
            meta,
        )
        .await;
        let notification_id = notification.ok().map(|notification| notification.id);
        (title, message, notification_id)
    }
    };
}
