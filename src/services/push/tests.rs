#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NotificationStore;

    #[test]
    fn test_push_level() {
        assert_eq!(PushLevel::Info.as_str(), "info");
        assert_eq!(PushLevel::Success.emoji(), "✅");
        assert_eq!(PushLevel::Warning.emoji(), "⚠️");
        assert_eq!(PushLevel::Error.emoji(), "❌");
    }

    #[test]
    fn test_enabled_channels() {
        let settings = Settings {
            wecom_bot_url: "https://test".to_string(),
            telegram_bot_token: "token".to_string(),
            telegram_chat_id: "123".to_string(),
            ..Default::default()
        };

        let service = PushService::new(settings);
        let channels = service.enabled_channels();

        assert_eq!(channels.len(), 2);
        assert!(channels.contains(&"wecom".to_string()));
        assert!(channels.contains(&"telegram".to_string()));
    }

    #[test]
    fn test_retry_policy_uses_exponential_backoff_with_cap() {
        let policy = PushRetryPolicy {
            max_attempts: 0,
            initial_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(5),
        };

        assert_eq!(policy.attempts(), 1);
        assert_eq!(policy.delay_for_retry(0), Duration::from_secs(2));
        assert_eq!(policy.delay_for_retry(1), Duration::from_secs(4));
        assert_eq!(policy.delay_for_retry(2), Duration::from_secs(5));
    }

    #[test]
    fn policy_routes_levels_quiet_hours_and_templates() {
        let settings = Settings {
            wecom_bot_url: "https://test".to_string(),
            telegram_bot_token: "token".to_string(),
            telegram_chat_id: "chat".to_string(),
            push_min_level: "warning".to_string(),
            push_event_routes: HashMap::from([(
                "subscription_failed".to_string(),
                vec!["telegram".to_string()],
            )]),
            push_title_template: "[{{level}}] {{title}}".to_string(),
            push_message_template: "{{event}}: {{message}}".to_string(),
            ..Default::default()
        };
        let service = PushService::new(settings);
        assert!(service.channels_for_event(PushEvent::SubscriptionFailed, PushLevel::Info).is_empty());
        assert_eq!(service.channels_for_event(PushEvent::SubscriptionFailed, PushLevel::Error), vec!["telegram"]);
        assert_eq!(service.render_template(PushEvent::SubscriptionFailed, "A", "B", PushLevel::Error), ("[error] A".to_string(), "subscription_failed: B".to_string()));
        assert!(quiet_hour(23, 23, 8));
        assert!(quiet_hour(7, 23, 8));
        assert!(!quiet_hour(12, 23, 8));
    }

    #[tokio::test]
    async fn test_send_to_channels_retries_until_success() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_send = attempts.clone();
        let (success, attempt_count, last_error) = send_with_retry(
            PushRetryPolicy {
                max_attempts: 3,
                initial_delay: Duration::ZERO,
                max_delay: Duration::ZERO,
            },
            move || {
                let attempts_for_send = attempts_for_send.clone();
                async move {
                    let attempt = attempts_for_send.fetch_add(1, Ordering::SeqCst) + 1;
                    Ok::<bool, AppError>(attempt == 3)
                }
            },
        )
        .await;

        assert!(success);
        assert_eq!(attempt_count, 3);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert_eq!(last_error, "渠道返回失败状态");
    }

    #[test]
    fn test_sanitize_push_error_masks_tokens() {
        let error = "request failed: https://example.com/message?token=abc123&key=secret bot123456:ABC_def SCTabcdef";
        let sanitized = sanitize_push_error(error);

        assert!(sanitized.contains("token=***"));
        assert!(sanitized.contains("key=***"));
        assert!(sanitized.contains("bot***"));
        assert!(sanitized.contains("SCT***"));
        assert!(!sanitized.contains("abc123"));
        assert!(!sanitized.contains("secret"));
        assert!(!sanitized.contains("ABC_def"));
    }

    #[tokio::test]
    async fn test_send_event_respects_global_switch() {
        let settings = Settings {
            push_on_update: false,
            wecom_bot_url: "https://test".to_string(),
            ..Default::default()
        };

        let service = PushService::new(settings);
        let results = service
            .send_event(
                PushEvent::SubscriptionUpdated,
                "title",
                "message",
                PushLevel::Info,
            )
            .await;

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_record_push_message_saves_results() {
        let tmp =
            std::env::temp_dir().join(format!("my-media-sub-push-{}.json", uuid::Uuid::new_v4()));
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        let results = HashMap::from([("telegram".to_string(), true), ("bark".to_string(), false)]);
        record_push_message(
            &store,
            PushEvent::SubscriptionUpdated.as_str(),
            "title",
            "message",
            PushLevel::Info,
            &results,
        )
        .await;

        let notifications = store.list(true).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(
            notifications[0].event,
            PushEvent::SubscriptionUpdated.as_str()
        );
        assert_eq!(notifications[0].level, "warning");
        assert_eq!(notifications[0].meta["push"]["success_count"], json!(1));
        assert_eq!(notifications[0].meta["push"]["failed_count"], json!(1));

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn test_record_push_message_saves_attempts() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-push-attempts-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        let report = PushDeliveryReport {
            results: HashMap::from([("telegram".to_string(), false)]),
            errors: HashMap::from([("telegram".to_string(), "尝试 3 次后失败".to_string())]),
            attempts: HashMap::from([("telegram".to_string(), 3)]),
        };

        record_push_message_report(
            &store,
            "subscription_updated",
            "title",
            "message",
            PushLevel::Info,
            &report,
        )
        .await;

        let notifications = store.list(true).await;
        assert_eq!(
            notifications[0].meta["push"]["attempts"]["telegram"],
            json!(3)
        );
        assert_eq!(
            notifications[0].meta["push"]["errors"]["telegram"],
            json!("尝试 3 次后失败")
        );

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn test_record_push_message_saves_sanitized_errors() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-push-errors-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        let results = HashMap::from([("gotify".to_string(), false)]);
        let errors = HashMap::from([(
            "gotify".to_string(),
            sanitize_push_error("https://gotify.example/message?token=secret-token failed"),
        )]);
        record_push_message_with_errors(
            &store,
            "push_test",
            "title",
            "message",
            PushLevel::Info,
            &results,
            &errors,
        )
        .await;

        let notifications = store.list(true).await;
        assert_eq!(
            notifications[0].meta["push"]["errors"]["gotify"],
            json!("https://gotify.example/message?token=*** failed")
        );

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn test_record_push_message_merges_into_existing_notification() {
        let tmp = std::env::temp_dir().join(format!(
            "my-media-sub-push-merge-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();
        let base = store
            .add(Notification {
                id: "base".to_string(),
                level: "info".to_string(),
                event: "subscription_updated".to_string(),
                title: "订阅有更新".to_string(),
                message: "发现新集".to_string(),
                meta: HashMap::new(),
                read: true,
                created_at: 1,
            })
            .await
            .unwrap();

        let report = PushDeliveryReport {
            results: HashMap::from([("telegram".to_string(), false)]),
            errors: HashMap::from([("telegram".to_string(), "发送失败".to_string())]),
            attempts: HashMap::from([("telegram".to_string(), 1)]),
        };
        record_push_message_report_for_notification(
            &store,
            Some(&base.id),
            "subscription_updated",
            "订阅有更新",
            "发现新集",
            PushLevel::Info,
            &report,
        )
        .await;

        let notifications = store.list(true).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].event, "subscription_updated");
        assert_eq!(notifications[0].level, "warning");
        assert!(!notifications[0].read);
        assert_eq!(notifications[0].meta["push"]["failed_count"], json!(1));
        assert_eq!(
            notifications[0].meta["push"]["results"]["telegram"],
            json!(false)
        );

        let _ = std::fs::remove_file(tmp);
    }
}
