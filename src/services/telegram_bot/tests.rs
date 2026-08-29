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
        assert_eq!(page_bounds(0, 1), (0, 0, 1, 1));
        assert_eq!(
            parse_command("/subscription abc123"),
            Some(("subscription", Some("abc123")))
        );
        assert_eq!(parse_command("/job deadbeef"), Some(("job", Some("deadbeef"))));
        assert_eq!(parse_command("hello"), None);
        assert_eq!(page_bounds(17, 99), (16, 17, 3, 3));
    }

    #[test]
    fn html_content_is_escaped_for_telegram_parse_mode() {
        assert_eq!(tg_escape("<a & b>"), "&lt;a &amp; b&gt;");
        assert_eq!(tg_escape("普通文本 123"), "普通文本 123");
    }

    #[test]
    fn chinese_plain_text_maps_to_commands() {
        assert_eq!(map_menu_text("任务"), Some("/jobs"));
        assert_eq!(map_menu_text("任务列表"), Some("/jobs"));
        assert_eq!(map_menu_text("通知列表"), Some("/notifications"));
        assert_eq!(map_menu_text("未读通知"), Some("/notifications"));
        assert_eq!(map_menu_text("诊断"), Some("/diagnostics"));
        assert_eq!(map_menu_text("状态"), Some("/status"));
        assert_eq!(map_menu_text("订阅"), Some("/subscriptions"));
        assert_eq!(map_menu_text("日历"), Some("/calendar"));
        assert_eq!(map_menu_text("检查全部"), Some("/check all"));
        assert_eq!(map_menu_text("帮助"), Some("/help"));
        assert_eq!(map_menu_text("随便说说"), None);
    }

    #[test]
    fn page_buttons_roundtrip_through_whitelist() {
        assert_eq!(
            parse_page_callback("page:subscriptions:2"),
            Some(("subscriptions", 2))
        );
        assert_eq!(parse_page_callback("page:jobs:0"), Some(("jobs", 1)));
        assert_eq!(parse_page_callback("page:calendar:abc"), None);
        assert_eq!(parse_page_callback("page:sub:2"), None);
        assert!(list_page_markup("subscriptions", 1, 1).is_none());
        let markup = list_page_markup("jobs", 2, 3).unwrap();
        let row = markup["inline_keyboard"][0].as_array().unwrap();
        assert_eq!(row.len(), 2);
        assert_eq!(
            row[0]["callback_data"].as_str().unwrap(),
            "page:jobs:1"
        );
        assert_eq!(
            row[1]["callback_data"].as_str().unwrap(),
            "page:jobs:3"
        );
    }

    #[test]
    fn telegram_parse_errors_trigger_html_fallback() {
        assert!(telegram_parse_error(
            "Bad Request: can't parse entities: line 1 column 2"
        ));
        assert!(!telegram_parse_error("Bad Request: message is too long"));
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
    fn proactive_callback_works_without_webhook_secret() {
        // 回归：纯 long_polling 部署默认没有 webhook secret，
        // 「继续下载/查看详情」等主动按钮此前会静默消失。
        let mut polling_settings = settings();
        polling_settings.telegram_chat_id = "42".to_string();
        polling_settings.telegram_bot_token = "123456:test-bot-token".to_string();
        polling_settings.telegram_bot_webhook_secret = String::new();
        let expires = crate::utils::unix_now() + 60;
        let data = telegram_prompt_callback_data(
            &polling_settings,
            "download",
            "12345678-1234-1234-1234-123456789012",
            expires,
        )
        .expect("long_polling 部署也应生成主动按钮");
        let token = data.strip_prefix("prompt:").unwrap();
        assert!(verify_prompt_callback_data(&polling_settings, token, 42, 42).is_ok());

        // 完全没有密钥材料（无 token 也无 secret）时仍拒绝生成。
        let mut no_secret_settings = settings();
        no_secret_settings.telegram_chat_id = "42".to_string();
        no_secret_settings.telegram_bot_token = String::new();
        no_secret_settings.telegram_bot_webhook_secret = String::new();
        assert!(telegram_prompt_callback_data(
            &no_secret_settings,
            "download",
            "12345678-1234-1234-1234-123456789012",
            expires,
        )
        .is_none());
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

        // 「继续下载」按钮使用 download 动作，资源为任务 UUID。
        let download = telegram_prompt_callback_data(
            &settings,
            "download",
            "12345678-1234-1234-1234-123456789012",
            crate::utils::unix_now() + 60,
        )
        .unwrap();
        assert!(download.starts_with("prompt:d."));
        let token = download.strip_prefix("prompt:").unwrap();
        let (action, resource) =
            verify_prompt_callback_data(&settings, token, 42, 42).unwrap();
        assert_eq!(action, "download");
        assert_eq!(resource, "12345678-1234-1234-1234-123456789012");
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

    #[tokio::test]
    async fn rate_limit_attempt_keys_are_removed_once_window_expires() {
        let (service, _context, dir) = test_service().await;
        service.allow_command(42, 42, "status", false).await;
        assert!(!service.command_rates.lock().await.attempts.is_empty());
        // 将所有尝试时间戳挪到窗口之外，下一次检查应把空键清理掉。
        {
            let mut rates = service.command_rates.lock().await;
            for attempts in rates.attempts.values_mut() {
                for at in attempts.iter_mut() {
                    *at -= RATE_WINDOW_SECONDS * 2;
                }
            }
        }
        assert!(service.allow_command(7, 7, "status", false).await);
        let rates = service.command_rates.lock().await;
        assert!(rates
            .attempts
            .keys()
            .all(|key| !key.contains("42")), "过期键应被移除: {:?}", rates.attempts.keys().collect::<Vec<_>>());
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
        assert_eq!(
            bot_action_scope("transfer", "1").unwrap(),
            "subscriptions:write"
        );
        assert_eq!(bot_action_scope("download", "job-1").unwrap(), "jobs:write");
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

    fn search_session(user_id: i64, chat_id: i64, hits: Vec<SearchHit>) -> UserSession {
        UserSession {
            user_id,
            chat_id,
            expires_at: crate::utils::unix_now() + 600,
            kind: SessionKind::Search { hits },
        }
    }

    #[tokio::test]
    async fn transfer_confirmation_binds_search_hit_snapshot() {
        // 回归：确认期间会话被新搜索覆盖时，按旧序号执行必须被指纹
        // 校验拦下，而不是静默转存新搜索的同序号结果。
        let (service, context, dir) = test_service().await;
        let hits = vec![
            SearchHit {
                title: "第一部剧".to_string(),
                url: "https://pan.quark.cn/s/aaa".to_string(),
                password: String::new(),
            },
            SearchHit {
                title: "第二部剧".to_string(),
                url: "https://pan.quark.cn/s/bbb".to_string(),
                password: String::new(),
            },
        ];
        service
            .sessions
            .lock()
            .await
            .put(search_session(42, 42, hits));
        let confirmation = service
            .transfer_prepare(Some("1"), 42, 42)
            .await
            .unwrap();
        assert!(confirmation.resource_label.contains("第一部剧"));

        // 同一用户再发一次搜索，会话被整体覆盖，序号 1 现在是另一条结果。
        let replaced = vec![SearchHit {
            title: "完全无关的资源".to_string(),
            url: "https://pan.quark.cn/s/zzz".to_string(),
            password: String::new(),
        }];
        service
            .sessions
            .lock()
            .await
            .put(search_session(42, 42, replaced));
        let error = service
            .execute_transfer(
                42,
                42,
                &confirmation.resource,
                &confirmation.resource_fingerprint,
            )
            .await
            .unwrap_err();
        assert!(error.contains("搜索结果已变化"));

        // 会话恢复为确认时的结果后，指纹一致才允许执行。
        let original = vec![
            SearchHit {
                title: "第一部剧".to_string(),
                url: "https://pan.quark.cn/s/aaa".to_string(),
                password: String::new(),
            },
            SearchHit {
                title: "第二部剧".to_string(),
                url: "https://pan.quark.cn/s/bbb".to_string(),
                password: String::new(),
            },
        ];
        service
            .sessions
            .lock()
            .await
            .put(search_session(42, 42, original));
        context
            .settings_store
            .update(|settings| settings.quark_cookie = "test-cookie".to_string())
            .await
            .unwrap();
        let ok = service
            .execute_transfer(
                42,
                42,
                &confirmation.resource,
                &confirmation.resource_fingerprint,
            )
            .await
            .unwrap();
        assert!(ok.contains("转存任务已提交"));

        service.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn subscribe_confirmation_binds_search_hit_snapshot() {
        let (service, context, dir) = test_service().await;
        let hits = vec![SearchHit {
            title: "要订阅的剧".to_string(),
            url: "https://pan.quark.cn/s/xyz".to_string(),
            password: String::new(),
        }];
        service
            .sessions
            .lock()
            .await
            .put(search_session(42, 42, hits));
        let confirmation = service
            .subscribe_prepare(Some("1 1-2"), 42, 42)
            .await
            .unwrap();

        let replaced = vec![SearchHit {
            title: "另一个人搜的剧".to_string(),
            url: "https://pan.quark.cn/s/ooo".to_string(),
            password: String::new(),
        }];
        service
            .sessions
            .lock()
            .await
            .put(search_session(42, 42, replaced));
        let error = service
            .execute_subscribe(
                42,
                42,
                &confirmation.resource,
                &confirmation.resource_fingerprint,
            )
            .await
            .unwrap_err();
        assert!(error.contains("搜索结果已变化"));
        assert!(
            context
                .subscription_store
                .list()
                .await
                .iter()
                .all(|sub| sub.url != "https://pan.quark.cn/s/ooo")
        );

        service.job_queue.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
