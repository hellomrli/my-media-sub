#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};
    use std::sync::{Arc, OnceLock};

    fn mock_env_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    fn test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "my_media_sub_{}_{}_{}.json",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn make_subscription() -> Subscription {
        Subscription {
            id: "sub1".to_string(),
            title: "Show".to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season: 1,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            tags: vec![],
            metadata: None,
            manual_schedule: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            strm_enabled: false,
            enabled: true,
            completed: false,
            rules: crate::models::rules::TransferRules::default(),
            rule_preset_id: String::new(),
            created_at: 0,
            updated_at: 0,
            last_checked_at: 0,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
            source_candidates: vec![],
            last_source_search_time: None,
            previous_share_links: vec![],
            source_failure_count: 0,
            last_source_switch_at: None,
            source_switch_history: vec![],
        }
    }

    fn make_service() -> (
        SubscriptionCheckService,
        Arc<SubscriptionStore>,
        Arc<NotificationStore>,
    ) {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("notifications")));
        (
            SubscriptionCheckService::new(subscriptions.clone(), settings, notifications.clone()),
            subscriptions,
            notifications,
        )
    }

    fn probe_file(name: &str, parent_path: &str, file_key: &str) -> ProbeFile {
        ProbeFile {
            name: name.to_string(),
            is_dir: false,
            parent_path: parent_path.to_string(),
            size: 1,
            updated_at: None,
            file_key: file_key.to_string(),
        }
    }

    fn probe_dir(name: &str, parent_path: &str, file_key: &str) -> ProbeFile {
        ProbeFile {
            name: name.to_string(),
            is_dir: true,
            parent_path: parent_path.to_string(),
            size: 0,
            updated_at: None,
            file_key: file_key.to_string(),
        }
    }

    #[test]
    fn test_extract_episode_number() {
        assert_eq!(extract_episode_number("动画名称 E01 1080p.mkv"), Some(1));
        assert_eq!(
            extract_episode_number("[字幕组] 动画名称 第12集.mp4"),
            Some(12)
        );
        assert_eq!(extract_episode_number("Show.S01E05.720p.mkv"), Some(5));
        assert_eq!(extract_episode_number("[01][1080p].mkv"), Some(1));
        assert_eq!(extract_episode_number("EP 03.mkv"), Some(3));
        assert_eq!(extract_episode_number("03.mkv"), Some(3));
        assert_eq!(extract_episode_number("129 4K.mp4"), Some(129));
        assert_eq!(extract_episode_number("23(1).mp4"), Some(23));
        assert_eq!(extract_episode_number("178重置版.mp4"), Some(178));
        assert_eq!(extract_episode_number("4K.mp4"), None);
        assert_eq!(
            extract_episode_number("S01E144.2025.2160p.WEB-DL.HQ.H265.30fps.10bit.AAC.mp4"),
            Some(144)
        );
        assert_eq!(extract_episode_number("Movie.2024.mkv"), None);
    }

    #[test]
    fn test_find_new_files_respects_start_episode_number() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.start_episode_number = Some(5);

        let files = vec![
            ProbeFile {
                name: "Show.S01E04.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "old-ep".to_string(),
            },
            ProbeFile {
                name: "Show.S01E05.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "start-ep".to_string(),
            },
            ProbeFile {
                name: "special.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "special".to_string(),
            },
        ];

        let new_names = service
            .find_new_files(&sub, &files)
            .into_iter()
            .map(|file| file.name)
            .collect::<Vec<_>>();

        assert_eq!(new_names, vec!["Show.S01E05.mkv"]);
    }

    #[test]
    fn test_find_new_files_dedups_episode_video_variants() {
        let (service, _, _) = make_service();
        let sub = make_subscription();
        let files = vec![
            ProbeFile {
                name: "178.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "ep178".to_string(),
            },
            ProbeFile {
                name: "178-4k.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "ep178-4k".to_string(),
            },
        ];

        let new_names = service
            .find_new_files(&sub, &files)
            .into_iter()
            .map(|file| file.name)
            .collect::<Vec<_>>();

        assert_eq!(new_names, vec!["178-4k.mkv"]);
    }

    #[test]
    fn test_find_new_files_skips_known_episode_video_variant() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.known_episodes = vec![178];
        let files = vec![ProbeFile {
            name: "178-4k.mkv".to_string(),
            is_dir: false,
            parent_path: String::new(),
            size: 1,
            updated_at: None,
            file_key: "ep178-4k".to_string(),
        }];

        let new_files = service.find_new_files(&sub, &files);

        assert!(new_files.is_empty());
    }

    #[test]
    fn test_find_new_files_skips_non_episode_extras_and_non_videos() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.media_type = "anime".to_string();
        sub.start_episode_number = Some(181);

        let files = vec![
            probe_file("181 4K.mp4", "", "ep181-4k"),
            probe_file("《不凡》MV：管它快意或失落 重要的是 我还是我.mp4", "", "mv1"),
            probe_file("不凡-王铮亮【《魔道争锋》片头】.修炼尘世中 千万年仿佛一刹.mp4", "", "op1"),
            probe_file("Show.S01E182.OP1.mp4", "", "op-with-episode"),
            probe_file("Show.S01E183.片尾曲.mp4", "", "ed-song"),
            probe_file("4K.mp4", "", "quality-only"),
            probe_file("欢迎各大影视站长来谈商务《网盘推广》.txt", "", "ad-txt"),
        ];

        let new_names = service
            .find_new_files(&sub, &files)
            .into_iter()
            .map(|file| file.name)
            .collect::<Vec<_>>();

        assert_eq!(new_names, vec!["181 4K.mp4"]);
    }

    #[test]
    fn test_find_new_files_respects_exclude_keywords() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.rules.exclude_keywords.push("特别篇".to_string());

        let files = vec![
            probe_file("Show.S01E05.mkv", "", "ep5"),
            probe_file("Show.S01E06.特别篇.mkv", "", "special-ep6"),
        ];

        let new_names = service
            .find_new_files(&sub, &files)
            .into_iter()
            .map(|file| file.name)
            .collect::<Vec<_>>();

        assert_eq!(new_names, vec!["Show.S01E05.mkv"]);
    }

    #[test]
    fn test_derived_content_filter_does_not_match_plain_substrings() {
        let (service, _, _) = make_service();
        let sub = make_subscription();
        let files = vec![
            probe_file("Show.S01E05.Encoded.WEB-DL.mkv", "", "encoded"),
            probe_file("Show.S01E06.Topaz.WEB-DL.mkv", "", "topaz"),
        ];

        let new_names = service
            .find_new_files(&sub, &files)
            .into_iter()
            .map(|file| file.name)
            .collect::<Vec<_>>();

        assert_eq!(
            new_names,
            vec!["Show.S01E05.Encoded.WEB-DL.mkv", "Show.S01E06.Topaz.WEB-DL.mkv"]
        );
    }

    #[test]
    fn test_find_new_files_skips_directories_and_other_seasons() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.media_type = "anime".to_string();
        sub.season = 6;
        sub.start_episode_number = Some(25);

        let files = vec![
            probe_dir("第6季", "", "dir-s6"),
            probe_dir("前五季+番外+剧场版", "", "dir-archive"),
            probe_file("25 4K.mp4", "一人之下 第六季/第6季", "s6-25"),
            probe_file("26 4K.mp4", "一人之下 第六季/第6季", "s6-26"),
            probe_file("01.mp4", "前五季+番外+剧场版/第1季（2016）4K", "s1-01"),
            probe_file(
                "S03E01.2020.1080p.WEB-DL.H265.mp4",
                "前五季+番外+剧场版/第3季（2020）",
                "s3-01",
            ),
            probe_file(
                "4K.mp4",
                "前五季+番外+剧场版/锈铁重现（2024）4K",
                "movie-extra",
            ),
        ];

        let new_names = service
            .find_new_files(&sub, &files)
            .into_iter()
            .map(|file| file.name)
            .collect::<Vec<_>>();
        let details = service.build_check_details(&sub, &files);

        assert_eq!(new_names, vec!["25 4K.mp4", "26 4K.mp4"]);
        assert_eq!(details.new_count, 2);
        assert_eq!(details.skipped_directory_count, 2);
        assert_eq!(details.skipped_other_season_count, 3);
    }

    #[test]
    fn test_build_check_details_classifies_probe_files() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.known_file_keys = vec!["known-key".to_string()];
        sub.start_episode_number = Some(5);
        let files = vec![
            ProbeFile {
                name: "Show.S01E03.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "known-key".to_string(),
            },
            ProbeFile {
                name: "Show.S01E04.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "before-start".to_string(),
            },
            ProbeFile {
                name: "Show.S01E05.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "new-key".to_string(),
            },
        ];

        let details = service.build_check_details(&sub, &files);

        assert_eq!(details.scanned_count, 3);
        assert_eq!(details.known_count, 1);
        assert_eq!(details.skipped_before_start_count, 1);
        assert_eq!(details.new_count, 1);
        assert_eq!(details.items[0].action, "known");
        assert_eq!(details.items[1].action, "skip");
        assert_eq!(details.items[2].action, "new");
    }

    #[test]
    fn test_build_check_details_marks_duplicate_episode_video() {
        let (service, _, _) = make_service();
        let sub = make_subscription();
        let files = vec![
            ProbeFile {
                name: "178.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "ep178".to_string(),
            },
            ProbeFile {
                name: "178-4k.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "ep178-4k".to_string(),
            },
        ];

        let details = service.build_check_details(&sub, &files);

        assert_eq!(details.new_count, 1);
        assert_eq!(details.skipped_duplicate_episode_count, 1);
        assert_eq!(details.items[0].action, "skip");
        assert_eq!(
            details.items[0].reason,
            "同集重复视频，已保留清晰度最高版本"
        );
        assert_eq!(details.items[1].action, "new");
    }

    #[test]
    fn test_transfer_candidates_retry_known_untransferred_episode() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.media_type = "anime".to_string();
        sub.start_episode_number = Some(144);
        sub.known_episodes = vec![144, 145, 146, 147];
        sub.transferred_files = vec![
            "145.mkv".to_string(),
            "146.mkv".to_string(),
            "S01E144.2025.2160p.WEB-DL.HQ.H265.30fps.10bit.AAC.mp4".to_string(),
        ];
        sub.transferred_file_keys = vec![];
        let files = vec![
            ProbeFile {
                name: "144-1.mp4".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "ep144-new-name".to_string(),
            },
            ProbeFile {
                name: "145.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "ep145".to_string(),
            },
            ProbeFile {
                name: "146.mkv".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "ep146".to_string(),
            },
            ProbeFile {
                name: "147.mp4".to_string(),
                is_dir: false,
                parent_path: String::new(),
                size: 1,
                updated_at: None,
                file_key: "ep147".to_string(),
            },
        ];

        let candidates = service.transfer_candidate_file_names(&sub, &files, &[]);

        assert_eq!(candidates, vec!["147.mp4".to_string()]);
    }

    #[tokio::test]
    async fn test_mock_probe_result_reads_fixture() {
        let _guard = mock_env_lock().lock().await;
        let path = test_path("mock_probe");
        let fixture = r#"{
            "https://pan.quark.cn/s/mock": {
                "ok": true,
                "state": "ok",
                "message": "",
                "files": [
                    {"name": "Show.S01E01.mkv", "size": 1, "file_key": "fid1"}
                ]
            }
        }"#;
        std::fs::write(&path, fixture).unwrap();
        std::env::set_var("MOCK_QUARK_SHARE_FIXTURE", &path);

        let result = mock_probe_result("https://pan.quark.cn/s/mock")
            .unwrap()
            .unwrap();
        let missing = mock_probe_result("https://pan.quark.cn/s/missing")
            .unwrap()
            .unwrap();

        std::env::remove_var("MOCK_QUARK_SHARE_FIXTURE");
        let _ = std::fs::remove_file(&path);

        assert!(result.ok);
        assert_eq!(result.files.len(), 1);
        assert!(!missing.ok);
        assert_eq!(missing.state, "mock_missing");
    }

    #[tokio::test]
    async fn check_all_subscriptions_persists_real_store_once() {
        let _guard = mock_env_lock().lock().await;
        let path = test_path("mock_batch_probe");
        let fixture = r#"{
            "https://pan.quark.cn/s/batch": {
                "ok": true,
                "state": "ok",
                "message": "",
                "files": [
                    {"name": "Show.S01E01.mkv", "size": 1, "file_key": "fid1"}
                ]
            }
        }"#;
        std::fs::write(&path, fixture).unwrap();
        std::env::set_var("MOCK_QUARK_SHARE_FIXTURE", &path);

        let (service, store, _) = make_service();
        let mut first = make_subscription();
        first.id = "batch-1".to_string();
        first.url = "https://pan.quark.cn/s/batch".to_string();
        let mut second = make_subscription();
        second.id = "batch-2".to_string();
        second.url = "https://pan.quark.cn/s/batch".to_string();
        store.create(first).await.unwrap();
        store.create(second).await.unwrap();
        let before = store.save_count();

        let results = service.check_all_subscriptions("cookie").await.unwrap();

        std::env::remove_var("MOCK_QUARK_SHARE_FIXTURE");
        let _ = std::fs::remove_file(&path);
        assert_eq!(results.len(), 2);
        assert_eq!(store.save_count(), before + 1);
        assert_eq!(store.get("batch-1").await.unwrap().known_episodes, vec![1]);
        assert_eq!(store.get("batch-2").await.unwrap().known_episodes, vec![1]);
    }

    #[tokio::test]
    async fn concurrent_checks_for_same_subscription_share_one_mutex() {
        let _guard = mock_env_lock().lock().await;
        let path = test_path("mock_subscription_mutex");
        std::fs::write(
            &path,
            r#"{
                "https://pan.quark.cn/s/mutex": {
                    "ok": true,
                    "state": "ok",
                    "message": "",
                    "files": []
                }
            }"#,
        )
        .unwrap();
        std::env::set_var("MOCK_QUARK_SHARE_FIXTURE", &path);

        let (service, store, _) = make_service();
        let mut sub = make_subscription();
        sub.id = "mutex-sub".to_string();
        sub.url = "https://pan.quark.cn/s/mutex".to_string();
        store.create(sub).await.unwrap();

        let lock = SubscriptionCheckService::named_lock(
            &service.subscription_locks,
            "mutex-sub",
        )
        .await;
        let guard = lock.lock().await;
        let runner = service.clone();
        let mut handle = tokio::spawn(async move {
            runner.check_subscription("mutex-sub", "cookie").await
        });

        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(20),
            &mut handle
        )
        .await
        .is_err());
        drop(guard);
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .unwrap()
            .unwrap();

        std::env::remove_var("MOCK_QUARK_SHARE_FIXTURE");
        let _ = std::fs::remove_file(&path);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn batch_probe_cache_deduplicates_same_share_link() {
        let _guard = mock_env_lock().lock().await;
        let path = test_path("mock_share_dedup");
        std::fs::write(
            &path,
            r#"{
                "https://pan.quark.cn/s/shared": {
                    "ok": true,
                    "state": "ok",
                    "message": "",
                    "files": [
                        {"name": "Show.S01E01.mkv", "size": 1, "file_key": "fid1"}
                    ]
                }
            }"#,
        )
        .unwrap();
        std::env::set_var("MOCK_QUARK_SHARE_FIXTURE", &path);

        let (service, _, _) = make_service();
        let service = service.with_batch_probe_cache();
        let mut sub = make_subscription();
        sub.url = "https://pan.quark.cn/s/shared".to_string();

        let first = service.probe_share(&sub, "cookie").await.unwrap();
        std::fs::write(&path, b"not-valid-json").unwrap();
        let second = service.probe_share(&sub, "cookie").await.unwrap();

        std::env::remove_var("MOCK_QUARK_SHARE_FIXTURE");
        let _ = std::fs::remove_file(&path);
        assert_eq!(first.files.len(), 1);
        assert_eq!(second.files.len(), 1);
        assert_eq!(second.files[0].file_key, "fid1");
    }

    #[tokio::test]
    async fn test_auto_transfer_disabled_reason_respects_switches() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();

        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, false).await,
            Some("自动下载新订阅项未启用")
        );
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, true).await,
            Some("全局自动转存未启用")
        );

        service
            .settings_store
            .update(|settings| {
                settings.auto_download_new_subscription_items = true;
                settings.quark_save_enabled = false;
            })
            .await
            .unwrap();
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, false).await,
            Some("全局自动转存未启用")
        );
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, true).await,
            Some("全局自动转存未启用")
        );

        service
            .settings_store
            .update(|settings| {
                settings.quark_save_enabled = true;
            })
            .await
            .unwrap();
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, false).await,
            None
        );

        service
            .settings_store
            .update(|settings| {
                settings.auto_download_new_subscription_items = false;
            })
            .await
            .unwrap();
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, false).await,
            Some("自动下载新订阅项未启用")
        );
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, true).await,
            None
        );

        sub.notify_only = true;
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, true).await,
            Some("订阅设置为仅通知模式")
        );
    }

    #[tokio::test]
    async fn test_update_subscription_after_check_records_new_files() {
        let (service, store, _) = make_service();
        let mut sub = make_subscription();
        sub.known_file_keys = vec!["old-key".to_string()];
        sub.status = "invalid".to_string();
        sub.invalid_since = Some(1);
        store.create(sub.clone()).await.unwrap();

        let probe = ProbeResult {
            ok: true,
            state: "ok".to_string(),
            message: String::new(),
            files: vec![
                ProbeFile {
                    name: "Show.S01E01.mkv".to_string(),
                    is_dir: false,
                    parent_path: String::new(),
                    size: 1,
                    updated_at: None,
                    file_key: "old-key".to_string(),
                },
                ProbeFile {
                    name: "Show.S01E02.mkv".to_string(),
                    is_dir: false,
                    parent_path: String::new(),
                    size: 1,
                    updated_at: None,
                    file_key: "new-key".to_string(),
                },
            ],
        };
        let new_files = service.find_new_files(&sub, &probe.files);
        let new_names = new_files
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>();
        let new_episodes = service.parse_episodes(&new_names);

        service
            .update_subscription_after_check(
                &sub,
                &probe,
                &new_names,
                &new_episodes,
                "发现 1 个新文件",
                false,
            )
            .await
            .unwrap();

        let updated = store.get("sub1").await.unwrap();
        assert_eq!(new_names, vec!["Show.S01E02.mkv"]);
        assert_eq!(new_episodes, vec![2]);
        assert_eq!(updated.current_episode_number, 2);
        assert_eq!(updated.status, "active");
        assert!(updated.invalid_since.is_none());
        assert!(updated.known_file_keys.contains(&"new-key".to_string()));
    }

    #[tokio::test]
    async fn test_start_episode_skips_old_files_but_records_known_keys() {
        let (service, store, _) = make_service();
        let mut sub = make_subscription();
        sub.start_episode_number = Some(5);
        store.create(sub.clone()).await.unwrap();

        let probe = ProbeResult {
            ok: true,
            state: "ok".to_string(),
            message: String::new(),
            files: vec![
                ProbeFile {
                    name: "Show.S01E04.mkv".to_string(),
                    is_dir: false,
                    parent_path: String::new(),
                    size: 1,
                    updated_at: None,
                    file_key: "ep4-key".to_string(),
                },
                ProbeFile {
                    name: "Show.S01E05.mkv".to_string(),
                    is_dir: false,
                    parent_path: String::new(),
                    size: 1,
                    updated_at: None,
                    file_key: "ep5-key".to_string(),
                },
            ],
        };
        let new_files = service.find_new_files(&sub, &probe.files);
        let new_names = new_files
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>();
        let new_episodes = service.parse_episodes(&new_names);

        service
            .update_subscription_after_check(
                &sub,
                &probe,
                &new_names,
                &new_episodes,
                "发现 1 个新文件",
                false,
            )
            .await
            .unwrap();

        let updated = store.get("sub1").await.unwrap();
        assert_eq!(new_names, vec!["Show.S01E05.mkv"]);
        assert_eq!(new_episodes, vec![5]);
        assert!(updated.known_file_keys.contains(&"ep4-key".to_string()));
        assert!(updated.known_file_keys.contains(&"ep5-key".to_string()));
        assert_eq!(updated.last_new_files, vec!["Show.S01E05.mkv"]);
    }

    #[tokio::test]
    async fn test_mark_subscription_invalid_sets_status() {
        let (service, store, _) = make_service();
        let mut sub = make_subscription();
        sub.rules.notify_on_invalid = false;
        store.create(sub.clone()).await.unwrap();

        service
            .mark_subscription_invalid(&sub, "invalid share")
            .await
            .unwrap();

        let updated = store.get("sub1").await.unwrap();
        assert_eq!(updated.status, "invalid");
        assert_eq!(updated.last_error, "invalid share");
        assert!(updated.invalid_since.is_some());
        assert_eq!(updated.source_failure_count, 1);
    }

    #[test]
    fn test_should_mark_completed() {
        let mut sub = Subscription {
            id: "sub1".to_string(),
            title: "Show".to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season: 1,
            start_episode_number: None,
            current_episode_number: 11,
            total_episode_number: None,
            source_group: String::new(),
            tags: vec![],
            metadata: None,
            manual_schedule: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            strm_enabled: false,
            enabled: true,
            completed: false,
            rules: crate::models::rules::TransferRules {
                finish_after_episode: Some(12),
                ..Default::default()
            },
            rule_preset_id: String::new(),
            created_at: 0,
            updated_at: 0,
            last_checked_at: 0,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
            known_episodes: vec![1, 2, 11],
            source_candidates: vec![],
            last_source_search_time: None,
            previous_share_links: vec![],
            source_failure_count: 0,
            last_source_switch_at: None,
            source_switch_history: vec![],
        };

        assert!(should_mark_completed_from_known_episodes(&sub, &[12]));
        assert!(!should_mark_completed_from_known_episodes(&sub, &[10]));

        sub.completed = true;
        assert!(!should_mark_completed_from_known_episodes(&sub, &[12]));
    }
    #[tokio::test]
    async fn cloud_type_selects_injected_provider_for_subscription_check() {
        let (service, _, _) = make_service();
        let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
        mock.set_probe_result(crate::providers::ProviderProbeResult {
            ok: true,
            state: "ok".to_string(),
            message: "selected mock provider".to_string(),
            files: vec![crate::providers::ProviderFile {
                id: "mock-episode-1".to_string(),
                name: "Show.S01E01.mkv".to_string(),
                is_dir: false,
                size: 1024,
                parent_path: String::new(),
                updated_at: None,
            }],
        });
        let registry = Arc::new(
            crate::providers::CloudDriveProviderRegistry::new().with_provider(mock),
        );
        let service = service.with_provider_registry(registry);
        let mut sub = make_subscription();
        sub.cloud_type = "mock".to_string();
        sub.url = "mock://show".to_string();

        let result = service.probe_share_uncached(&sub, "").await.unwrap();

        assert!(result.ok);
        assert_eq!(result.message, "selected mock provider");
        assert_eq!(result.files[0].file_key, "mock-episode-1");
    }

}
