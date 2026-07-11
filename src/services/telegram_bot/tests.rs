#[cfg(test)]
mod tests {
    use super::*;

    async fn test_service() -> (
        TelegramBotService,
        Arc<crate::app::AppContext>,
        std::path::PathBuf,
    ) {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-telegram-service-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let config = crate::config::Config {
            server: crate::config::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                username: "admin".to_string(),
                password: "test-password".to_string(),
            },
            data_dir: dir.clone(),
        };
        let context = crate::app::AppContext::new(&config).await.unwrap();
        let service = TelegramBotService::with_api_base(
            TelegramBotDependencies {
                settings_store: context.settings_store.clone(),
                subscription_store: context.subscription_store.clone(),
                notification_store: context.notification_store.clone(),
                automation_event_store: context.automation_event_store.clone(),
                job_store: context.job_store.clone(),
                job_queue: context.job_queue.clone(),
                check_service: context.check_service.clone(),
                signin_service: context.quark_signin_service.clone(),
                telegram_store: context.telegram_bot_store.clone(),
            },
            "http://127.0.0.1:9",
        );
        (service, context, dir)
    }

    fn settings() -> Settings {
        Settings {
            telegram_bot_allowed_user_ids: vec![42],
            telegram_bot_allowed_chat_ids: vec![42],
            telegram_bot_private_only: true,
            ..Settings::default()
        }
    }

    #[test]
    fn authorization_uses_numeric_ids_and_private_chat_type() {
        let settings = settings();
        assert!(is_authorized(
            &settings,
            42,
            &TelegramChat {
                id: 42,
                kind: "private".to_string()
            }
        ));
        assert!(!is_authorized(
            &settings,
            7,
            &TelegramChat {
                id: 42,
                kind: "private".to_string()
            }
        ));
        assert!(!is_authorized(
            &settings,
            42,
            &TelegramChat {
                id: 42,
                kind: "group".to_string()
            }
        ));
    }

    #[test]
    fn commands_support_bot_suffix_and_bounded_pages() {
        assert_eq!(
            parse_command("/subscriptions@my_bot 3"),
            Some(("subscriptions", Some("3")))
        );
        assert_eq!(parse_command("/jobs 0"), Some(("jobs", Some("0"))));
        assert_eq!(parse_command("hello"), None);
        assert_eq!(page_bounds(17, 99), (16, 17, 3, 3));
    }

    #[test]
    fn messages_are_split_on_unicode_boundaries_below_telegram_limit() {
        let value = vec!["测试内容"; 1_000].join("\n");
        let parts = split_message(&value);
        assert!(parts.len() > 1);
        assert!(parts
            .iter()
            .all(|part| part.chars().count() <= TELEGRAM_MESSAGE_LIMIT));
    }

    #[test]
    fn errors_remove_bot_tokens_and_webhook_secrets() {
        let mut settings = settings();
        settings.telegram_bot_token = "not-a-standard-token".to_string();
        settings.telegram_bot_webhook_path_secret = "private-path".to_string();
        settings.telegram_bot_webhook_secret = "private-header".to_string();
        let sanitized = sanitize_error_with_settings(
            "request not-a-standard-token private-path private-header bot123456:ABC_def-123 failed",
            &settings,
        );
        assert_eq!(sanitized, "request *** *** *** bot*** failed");
    }

    #[tokio::test]
    async fn confirmation_is_bound_one_time_and_concurrency_safe() {
        let (service, _context, dir) = test_service().await;
        let confirmation = service
            .prepare_confirmation(42, 42, "signin", None)
            .await
            .unwrap();
        assert!(service
            .claim_confirmation(&confirmation.nonce, 7, 42, true)
            .await
            .is_err());
        let (first, second) = tokio::join!(
            service.claim_confirmation(&confirmation.nonce, 42, 42, true),
            service.claim_confirmation(&confirmation.nonce, 42, 42, true)
        );
        assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
        assert!(service
            .claim_confirmation(&confirmation.nonce, 42, 42, true)
            .await
            .is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn expired_confirmation_is_rejected() {
        let (service, _context, dir) = test_service().await;
        let confirmation = service
            .prepare_confirmation(42, 42, "signin", None)
            .await
            .unwrap();
        service
            .confirmations
            .lock()
            .await
            .get_mut(&confirmation.nonce)
            .unwrap()
            .expires_at = crate::utils::unix_now() - 1;
        assert!(service
            .claim_confirmation(&confirmation.nonce, 42, 42, true)
            .await
            .unwrap_err()
            .contains("过期"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn layered_rate_limit_and_failure_cooldown_are_isolated_to_bot() {
        let (service, _context, dir) = test_service().await;
        for _ in 0..6 {
            assert!(service.allow_command(42, 42, "signin", true).await);
        }
        assert!(!service.allow_command(42, 42, "signin", true).await);
        for _ in 0..3 {
            service.record_action_outcome(7, 7, false).await;
        }
        assert!(!service.allow_command(7, 7, "status", false).await);
        assert!(service.allow_command(8, 8, "status", false).await);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn proactive_callback_signature_binds_user_chat_and_expiry() {
        let mut settings = settings();
        settings.telegram_chat_id = "42".to_string();
        settings.telegram_bot_webhook_secret = "a".repeat(64);
        let expires = crate::utils::unix_now() + 60;
        let data = telegram_prompt_callback_data(
            &settings,
            "read",
            "12345678-1234-1234-1234-123456789012",
            expires,
        )
        .unwrap();
        assert!(data.len() <= 64);
        let token = data.strip_prefix("prompt:").unwrap();
        assert!(verify_prompt_callback_data(&settings, token, 42, 42).is_ok());
        assert!(verify_prompt_callback_data(&settings, token, 7, 42).is_err());
        assert!(verify_prompt_callback_data(&settings, token, 42, 7).is_err());
        let expired = telegram_prompt_callback_data(
            &settings,
            "read",
            "12345678-1234-1234-1234-123456789012",
            crate::utils::unix_now() - 1,
        )
        .unwrap();
        assert!(verify_prompt_callback_data(
            &settings,
            expired.strip_prefix("prompt:").unwrap(),
            42,
            42
        )
        .unwrap_err()
        .contains("过期"));
    }

    #[tokio::test]
    async fn confirmed_read_reuses_notification_store_and_returns_correlation() {
        let (service, context, dir) = test_service().await;
        context
            .notification_store
            .add(crate::models::Notification {
                id: "notification-1".to_string(),
                level: "info".to_string(),
                event: "test".to_string(),
                title: "测试".to_string(),
                message: "测试消息".to_string(),
                meta: HashMap::new(),
                read: false,
                created_at: crate::utils::unix_now(),
            })
            .await
            .unwrap();
        let confirmation = service
            .prepare_confirmation(42, 42, "read", Some("notification-1"))
            .await
            .unwrap();
        let response = service
            .execute_confirmation(&confirmation, "correlation-read-1")
            .await
            .unwrap();
        assert!(response.contains("correlation-read-1"));
        assert!(context.notification_store.list(false).await.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn telegram_429_and_5xx_are_sanitized_and_leave_core_data_untouched() {
        let (_service, context, dir) = test_service().await;
        let before = context.subscription_store.count().await;
        let mut settings = context.settings_store.get().await;
        settings.telegram_bot_token = "999:secret".to_string();
        for status in [
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let error = telegram_response_result::<serde_json::Value>(
                status,
                TelegramApiResponse {
                    ok: false,
                    result: None,
                    description: Some("upstream failed 999:secret".to_string()),
                },
                &settings,
            )
            .unwrap_err();
            assert!(!error.contains("999:secret"));
            assert!(error.contains("***"));
        }
        assert_eq!(context.subscription_store.count().await, before);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn write_actions_reuse_automation_api_minimum_scopes() {
        assert_eq!(
            bot_action_scope("check", "all").unwrap(),
            "subscriptions:check"
        );
        assert_eq!(bot_action_scope("retry", "job-1").unwrap(), "jobs:write");
        assert_eq!(
            bot_action_scope("read", "notification-1").unwrap(),
            "notifications:write"
        );
        assert_eq!(bot_action_scope("signin", "quark").unwrap(), "quark:signin");
        assert!(bot_action_scope("delete", "anything").is_err());
    }

    #[test]
    fn forged_username_never_grants_authorization() {
        let update: TelegramUpdate = serde_json::from_value(json!({
            "update_id": 1,
            "message": {
                "message_id": 1,
                "chat": {"id": 42, "type": "private"},
                "from": {"id": 7, "username": "trusted-admin"},
                "text": "/status"
            }
        }))
        .unwrap();
        let message = update.message.unwrap();
        assert!(!is_authorized(
            &settings(),
            message.from.unwrap().id,
            &message.chat
        ));
    }
}
