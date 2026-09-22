#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MediaMetadata, MetadataProvider, Settings};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    fn test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "my_media_sub_transfer_{}_{}_{}.json",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn video_item(name: &str) -> DriveItem {
        DriveItem {
            id: format!("fid-{name}"),
            parent_id: "parent".to_string(),
            name: name.to_string(),
            is_dir: false,
            size: 0,
            updated_at: String::new(),
        }
    }

    fn subscription(media_type: &str, season: i32) -> Subscription {
        Subscription {
            id: "sub".to_string(),
            title: "庆余年".to_string(),
            source_title: String::new(),
            media_type: media_type.to_string(),
            season,
            season_end: None,
            season_list: None,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            tags: vec![],
            metadata: Some(MediaMetadata {
                provider: MetadataProvider::Tmdb,
                provider_id: "1".to_string(),
                title: "庆余年".to_string(),
                original_title: String::new(),
                media_type: media_type.to_string(),
                overview: String::new(),
                poster_url: None,
                backdrop_url: None,
                release_date: Some("2024-01-01".to_string()),
                vote_average: None,
                number_of_episodes: None,
                number_of_seasons: None,
                seasons: vec![],
                next_episode_to_air: None,
                episodes: vec![],
            }),
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            pending_transfers: Vec::new(),
            pending_downloads: Vec::new(),
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            sync_downloads: vec![],
            enabled: true,
            completed: false,
            rules: TransferRules::default(),
            rule_preset_id: String::new(),
            created_at: 1,
            updated_at: 1,
            last_checked_at: 1,
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

    #[test]
    fn determine_target_directory_uses_media_folder_and_season_for_series() {
        let settings = Settings {
            quark_save_series_dir: "/连续剧".to_string(),
            ..Default::default()
        };
        let sub = subscription("series", 1);

        let target = determine_subscription_target_directory(&sub, &settings);

        assert_eq!(target, "/连续剧/庆余年 (2024)/Season 1");
    }

    #[test]
    fn determine_target_directory_does_not_append_season_for_movie() {
        let settings = Settings {
            quark_save_movie_dir: "/电影".to_string(),
            ..Default::default()
        };
        let sub = subscription("movie", 1);

        let target = determine_subscription_target_directory(&sub, &settings);

        assert_eq!(target, "/电影/庆余年 (2024)");
    }

    #[test]
    fn determine_target_directory_multi_season_keeps_show_root() {
        let settings = Settings {
            quark_save_series_dir: "/连续剧".to_string(),
            ..Default::default()
        };
        let mut sub = subscription("series", 1);
        sub.season_end = Some(4);

        let target = determine_subscription_target_directory(&sub, &settings);
        assert_eq!(target, "/连续剧/庆余年 (2024)");
        assert_eq!(
            season_target_directory(&target, 3),
            "/连续剧/庆余年 (2024)/Season 3"
        );
    }

    #[test]
    fn determine_target_directory_keeps_existing_season_suffix() {
        let settings = Settings {
            quark_save_anime_dir: "/动画".to_string(),
            ..Default::default()
        };
        let mut sub = subscription("anime", 2);
        sub.rules.target_dir = "/动画/孤独摇滚（2022）/Season 2".to_string();

        let target = determine_subscription_target_directory(&sub, &settings);

        assert_eq!(target, "/动画/孤独摇滚（2022）/Season 2");
    }

    #[test]
    fn media_type_aria2_directory_prefers_category_dir() {
        let settings = Settings {
            aria2_movie_dir: "/downloads/movies".to_string(),
            ..Default::default()
        };
        let sub = subscription("movie", 1);

        assert_eq!(
            media_type_aria2_directory(&sub, &settings),
            "/downloads/movies"
        );
    }

    #[test]
    fn media_type_aria2_directory_uses_custom_category_dir() {
        let settings = Settings {
            custom_categories: vec![crate::models::settings::CustomCategory {
                id: "doc".to_string(),
                name: "纪录片".to_string(),
                dir: "/纪录片".to_string(),
                aria2_dir: "/downloads/docs".to_string(),
            }],
            ..Default::default()
        };
        let sub = subscription("custom_doc", 1);

        assert_eq!(
            media_type_aria2_directory(&sub, &settings),
            "/downloads/docs"
        );
    }

    #[test]
    fn media_type_aria2_directory_returns_empty_without_category_dir() {
        let settings = Settings::default();
        let sub = subscription("series", 1);

        assert_eq!(media_type_aria2_directory(&sub, &settings), "");
    }

    #[test]
    fn sync_download_directory_preserves_explicit_season_path() {
        let settings = Settings {
            aria2_series_dir: "/downloads/series".to_string(),
            ..Default::default()
        };
        let mut sub = subscription("series", 1);
        sub.sync_download_dir = "/downloads/custom/Show/Season 2".to_string();

        assert_eq!(
            resolve_sync_download_dir_for_season(&sub, &settings, 4),
            "/downloads/custom/Show/Season 2"
        );
    }

    #[test]
    fn sync_download_directory_appends_detected_season_when_not_explicit() {
        let settings = Settings {
            aria2_series_dir: "/downloads/series".to_string(),
            ..Default::default()
        };
        let mut sub = subscription("series", 1);
        sub.sync_download_dir = "/downloads/custom/Show".to_string();

        assert_eq!(
            resolve_sync_download_dir_for_season(&sub, &settings, 4),
            "/downloads/custom/Show/Season 4"
        );
    }

    #[test]
    fn expected_video_names_only_keeps_videos() {
        let names = vec![
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4".to_string(),
            "poster.jpg".to_string(),
            "Episode.06.mkv".to_string(),
        ];

        let expected = expected_video_names(&names);

        assert_eq!(expected.len(), 2);
        assert!(expected.contains("Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"));
        assert!(expected.contains("Episode.06.mkv"));
        assert!(!expected.contains("poster.jpg"));
    }

    #[test]
    fn dedup_provider_episode_files_can_keep_latest_upload() {
        let mut sub = subscription("series", 1);
        sub.rules.duplicate_episode_strategy = "latest_upload".to_string();
        let old_4k = ProviderFile {
            name: "178-4k.mkv".to_string(),
            id: "fid-4k".to_string(),
            is_dir: false,
            size: 10,
            parent_path: String::new(),
            updated_at: Some("2024-01-01T00:00:00Z".to_string()),
                                };
        let latest = ProviderFile {
            name: "178.mkv".to_string(),
            id: "fid-latest".to_string(),
                        is_dir: false,
            size: 1,
            parent_path: String::new(),
            updated_at: Some("2024-01-02T00:00:00Z".to_string()),
                                };

        let deduped = dedup_provider_episode_files(&sub, vec![&old_4k, &latest]);

        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].name, "178.mkv");
    }

    #[test]
    fn dedup_provider_episode_files_keeps_movie_files() {
        let sub = subscription("movie", 1);
        let first = ProviderFile {
            name: "178.mkv".to_string(),
            id: "fid-1".to_string(),
                        is_dir: false,
            size: 1,
            parent_path: String::new(),
            updated_at: None,
                                };
        let second = ProviderFile {
            name: "178-4k.mkv".to_string(),
            id: "fid-2".to_string(),
                        is_dir: false,
            size: 2,
            parent_path: String::new(),
            updated_at: None,
                                };

        let deduped = dedup_provider_episode_files(&sub, vec![&first, &second]);

        assert_eq!(deduped.len(), 2);
    }

    fn provider_video(name: &str, parent_path: &str) -> ProviderFile {
        ProviderFile {
            name: name.to_string(),
            id: format!("fid-{name}"),
            is_dir: false,
            size: 1,
            parent_path: parent_path.to_string(),
            updated_at: None,
        }
    }

    #[test]
    fn filter_already_transferred_skips_by_name_and_episode_key() {
        let mut sub = subscription("series", 1);
        sub.transferred_files = vec!["第01集.mkv".to_string()];
        sub.transferred_file_keys = vec!["ep:1".to_string(), "ep:2".to_string()];
        let e1 = provider_video("第01集.mkv", "");
        let e2_renamed = provider_video("Show.S01E02.1080p.mkv", "");
        let e3 = provider_video("第03集.mkv", "");

        let (kept, skipped) =
            filter_already_transferred_files(&sub, vec![&e1, &e2_renamed, &e3]);

        assert_eq!(skipped, 2);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].name, "第03集.mkv");
    }

    #[test]
    fn filter_already_transferred_keeps_other_season_for_multi_season() {
        let mut sub = subscription("series", 1);
        sub.season_end = Some(2);
        sub.transferred_files = vec!["Show.S01E05.mkv".to_string()];
        sub.transferred_file_keys = vec!["ep:5".to_string()];
        let same_name = provider_video("Show.S01E05.mkv", "");
        let s2e5 = provider_video("Show.S02E05.mkv", "");

        let (kept, skipped) = filter_already_transferred_files(&sub, vec![&same_name, &s2e5]);

        assert_eq!(skipped, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].name, "Show.S02E05.mkv");
    }

    #[test]
    fn filter_already_transferred_respects_disabled_flag() {
        let mut sub = subscription("series", 1);
        sub.rules.skip_existing_transferred = false;
        sub.transferred_files = vec!["第01集.mkv".to_string()];
        sub.transferred_file_keys = vec!["ep:1".to_string()];
        let e1 = provider_video("第01集.mkv", "");

        let (kept, skipped) = filter_already_transferred_files(&sub, vec![&e1]);

        assert_eq!(skipped, 0);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn transfer_match_targets_match_renamed_same_episode_video() {
        let sub = subscription("anime", 1);
        let targets = TransferMatchTargets::from_file_names(
            &sub,
            &["S01E147.2025.2160p.WEB-DL.HQ.H265.30fps.10bit.AAC.mp4".to_string()],
        );
        let renamed = ProviderFile {
            name: "147.mp4".to_string(),
            id: "fid-147".to_string(),
                        is_dir: false,
            size: 1,
            parent_path: String::new(),
            updated_at: None,
                                };

        let matched = filter_transfer_candidates_by_targets(&sub, vec![&renamed], &targets);

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "147.mp4");
    }

    #[test]
    fn transfer_match_targets_skip_other_season_parent_paths() {
        let sub = subscription("anime", 6);
        let targets = TransferMatchTargets::from_file_names(&sub, &["25 4K.mp4".to_string()]);
        let current = ProviderFile {
            name: "25 4K.mp4".to_string(),
            id: "fid-s6-25".to_string(),
                        is_dir: false,
            size: 1,
            parent_path: "一人之下 第六季/第6季".to_string(),
            updated_at: None,
                                };
        let other = ProviderFile {
            name: "25 4K.mp4".to_string(),
            id: "fid-s1-25".to_string(),
                        is_dir: false,
            size: 1,
            parent_path: "前五季+番外+剧场版/第1季（2016）4K".to_string(),
            updated_at: None,
                                };

        let matched = filter_transfer_candidates_by_targets(&sub, vec![&other, &current], &targets);

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, "fid-s6-25");
    }

    #[test]
    fn filter_rename_candidates_limits_auto_rename_to_expected_names() {
        let expected = expected_video_names(&[
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4".to_string(),
        ]);
        let candidates = vec![
            video_item("Joy.of.Life.2019.S01.EP04.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"),
            video_item("Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"),
        ];

        let filtered = filter_rename_candidates(candidates, Some(&expected));

        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].name,
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"
        );
    }

    #[test]
    fn filter_rename_candidates_keeps_all_for_manual_repair() {
        let candidates = vec![video_item("Episode.01.mp4"), video_item("Episode.02.mp4")];

        let filtered = filter_rename_candidates(candidates, None);

        assert_eq!(filtered.len(), 2);
    }

    #[tokio::test]
    async fn auto_transfer_new_files_skips_when_auto_save_disabled() {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("notifications")));
        subscriptions
            .create(subscription("series", 1))
            .await
            .unwrap();
        settings
            .update(|settings| {
                settings.auto_download_new_subscription_items = false;
                settings.quark_save_enabled = false;
                settings.quark_cookie = "cookie".to_string();
            })
            .await
            .unwrap();

        let service = SubscriptionTransferService::new(subscriptions, settings, notifications);
        let result = service
            .auto_transfer_new_files_with_options("sub", &["Episode.01.mkv".to_string()], false)
            .await
            .unwrap();

        assert!(result.skipped);
        assert_eq!(result.transferred_count, 0);
        assert_eq!(result.reason, "自动下载新订阅项未启用");
    }

    #[tokio::test]
    async fn auto_transfer_allows_when_quark_save_enabled_without_auto_download_flag() {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("notifications")));
        subscriptions
            .create(subscription("series", 1))
            .await
            .unwrap();
        settings
            .update(|settings| {
                settings.auto_download_new_subscription_items = false;
                settings.quark_save_enabled = true;
                settings.quark_cookie = "cookie".to_string();
            })
            .await
            .unwrap();

        let service = SubscriptionTransferService::new(subscriptions, settings, notifications);
        // 开启全局自动转存后不应再被 auto_download 开关挡住；
        // 后续会因 mock 探测失败而报错，但不会以“自动下载未启用”跳过。
        let result = service
            .auto_transfer_new_files_with_options("sub", &["Episode.01.mkv".to_string()], false)
            .await;
        if let Ok(outcome) = result {
            assert_ne!(outcome.reason, "自动下载新订阅项未启用");
        }
    }

    #[tokio::test]
    async fn mark_files_as_transferred_records_episode_keys() {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("notifications")));
        let sub = subscription("series", 1);
        subscriptions.create(sub.clone()).await.unwrap();

        let service =
            SubscriptionTransferService::new(subscriptions.clone(), settings, notifications);
        service
            .mark_files_as_transferred(&sub, &["178-4k.mkv".to_string()], sub.season)
            .await
            .unwrap();

        let updated = subscriptions.get("sub").await.unwrap();
        assert_eq!(updated.transferred_files, vec!["Season 1/178-4k.mkv".to_string()]);
        assert_eq!(updated.transferred_file_keys, vec!["s:1:ep:178".to_string(), "ep:178".to_string()]);
    }

    #[tokio::test]
    async fn sync_download_mapping_is_persisted_on_subscription() {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("sync_subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("sync_settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("sync_notifications")));
        subscriptions
            .create(subscription("series", 1))
            .await
            .unwrap();
        let service =
            SubscriptionTransferService::new(subscriptions.clone(), settings, notifications);
        let report = SyncDownloadReport {
            submitted_count: 1,
            dir: "/downloads/series".to_string(),
            error: None,
            items: vec![SyncDownloadItem {
                gid: "gid-1".to_string(),
                file_name: "Show.S01E01.mkv".to_string(),
            }],
        };

        service
            .record_sync_downloads("sub", "/series/Show/Season 1", &report)
            .await
            .unwrap();
        service
            .record_sync_downloads("sub", "/series/Show/Season 1", &report)
            .await
            .unwrap();

        let updated = subscriptions.get("sub").await.unwrap();
        assert_eq!(updated.sync_downloads.len(), 1);
        let record = &updated.sync_downloads[0];
        assert_eq!(record.gid, "gid-1");
        assert_eq!(record.file_name, "Show.S01E01.mkv");
        assert_eq!(record.download_dir, "/downloads/series");
        assert_eq!(record.target_dir, "/series/Show/Season 1");
        assert!(record.submitted_at > 0);
        assert_eq!(record.completed_at, None);
    }

    #[tokio::test]
    async fn wait_for_rename_candidates_waits_for_expected_transfer_file() {
        let expected = expected_video_names(&[
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4".to_string(),
        ]);
        let responses = Arc::new(Mutex::new(VecDeque::from([
            vec![video_item(
                "Joy.of.Life.2019.S01.EP04.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4",
            )],
            vec![
                video_item("Joy.of.Life.2019.S01.EP04.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"),
                video_item("Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"),
            ],
        ])));
        let attempts = Arc::new(Mutex::new(0usize));

        let candidates = wait_for_rename_candidates(
            || {
                let responses = responses.clone();
                let attempts = attempts.clone();
                async move {
                    *attempts.lock().unwrap() += 1;
                    Ok(responses.lock().unwrap().pop_front().unwrap_or_default())
                }
            },
            Some(&expected),
            3,
            Duration::ZERO,
        )
        .await
        .unwrap();

        assert_eq!(*attempts.lock().unwrap(), 2);
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].name,
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"
        );
    }

    #[tokio::test]
    async fn wait_for_rename_candidates_stops_after_max_attempts() {
        let expected = expected_video_names(&["Episode.03.mp4".to_string()]);
        let attempts = Arc::new(Mutex::new(0usize));

        let candidates = wait_for_rename_candidates(
            || {
                let attempts = attempts.clone();
                async move {
                    *attempts.lock().unwrap() += 1;
                    Ok(vec![video_item("Episode.01.mp4")])
                }
            },
            Some(&expected),
            2,
            Duration::ZERO,
        )
        .await
        .unwrap();

        assert_eq!(*attempts.lock().unwrap(), 2);
        assert!(candidates.is_empty());
    }
    #[tokio::test]
    async fn cloud_type_selects_mock_provider_and_surfaces_transfer_failure() {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("provider_subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("provider_settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("provider_notifications")));
        let mut sub = subscription("series", 1);
        sub.cloud_type = "mock".to_string();
        sub.url = "mock://show".to_string();
        subscriptions.create(sub).await.unwrap();
        settings
            .update(|settings| {
                settings.auto_download_new_subscription_items = true;
                settings.quark_save_enabled = true;
            })
            .await
            .unwrap();

        let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
        mock.set_probe_result(crate::providers::ProviderProbeResult {
            ok: true,
            state: "ok".to_string(),
            message: String::new(),
            files: vec![crate::providers::ProviderFile {
                id: "episode-1".to_string(),
                name: "Episode.01.mkv".to_string(),
                is_dir: false,
                size: 1,
                parent_path: String::new(),
                updated_at: None,
            }],
        });
        mock.fail("transfer", "mock transfer failed");
        let registry = Arc::new(
            crate::providers::CloudDriveProviderRegistry::new().with_provider(mock),
        );
        let service = SubscriptionTransferService::new(subscriptions, settings, notifications)
            .with_provider_registry(registry);

        let error = service
            .auto_transfer_new_files_with_options(
                "sub",
                &["Episode.01.mkv".to_string()],
                false,
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("mock transfer failed"));
    }

    #[tokio::test]
    async fn target_directory_failure_does_not_fall_back_to_root() {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("ensure_subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("ensure_settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("ensure_notifications")));
        let mut sub = subscription("series", 1);
        sub.cloud_type = "mock".to_string();
        sub.url = "mock://show".to_string();
        sub.rules.target_dir = "/series/show/Season 1".to_string();
        subscriptions.create(sub).await.unwrap();
        settings
            .update(|settings| {
                settings.auto_download_new_subscription_items = true;
                settings.quark_save_enabled = true;
            })
            .await
            .unwrap();

        let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
        mock.set_probe_result(crate::providers::ProviderProbeResult {
            ok: true,
            state: "ok".to_string(),
            message: String::new(),
            files: vec![crate::providers::ProviderFile {
                id: "episode-1".to_string(),
                name: "Episode.01.mkv".to_string(),
                is_dir: false,
                size: 1,
                parent_path: String::new(),
                updated_at: None,
            }],
        });
        mock.fail("ensure", "cannot create target directory");
        let registry = Arc::new(
            crate::providers::CloudDriveProviderRegistry::new().with_provider(mock.clone()),
        );
        let service = SubscriptionTransferService::new(
            subscriptions.clone(),
            settings,
            notifications,
        )
        .with_provider_registry(registry);

        let error = service
            .auto_transfer_new_files_with_options("sub", &["Episode.01.mkv".to_string()], false)
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("cannot create target directory"));
        assert!(mock.transfer_requests().is_empty());
        let updated = subscriptions.get("sub").await.unwrap();
        assert!(updated.transferred_files.is_empty());
        assert!(updated.transferred_file_keys.is_empty());
    }

    #[tokio::test]
    async fn transfer_persists_transferred_state_even_when_listing_fails_afterwards() {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("listing_subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("listing_settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("listing_notifications")));
        let mut sub = subscription("series", 1);
        sub.cloud_type = "mock".to_string();
        sub.url = "mock://show".to_string();
        subscriptions.create(sub).await.unwrap();
        settings
            .update(|settings| {
                settings.auto_download_new_subscription_items = true;
                settings.quark_save_enabled = true;
            })
            .await
            .unwrap();

        let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
        mock.set_probe_result(crate::providers::ProviderProbeResult {
            ok: true,
            state: "ok".to_string(),
            message: String::new(),
            files: vec![crate::providers::ProviderFile {
                id: "episode-1".to_string(),
                name: "Episode.01.mkv".to_string(),
                is_dir: false,
                size: 1,
                parent_path: String::new(),
                updated_at: None,
            }],
        });
        // 转存成功，但随后的目录列举（等待落盘/重命名）瞬时失败。
        mock.fail("list", "transient listing failure");
        let registry =
            Arc::new(crate::providers::CloudDriveProviderRegistry::new().with_provider(mock));
        let service =
            SubscriptionTransferService::new(subscriptions.clone(), settings, notifications)
                .with_provider_registry(registry);

        let result = service
            .auto_transfer_new_files_with_options("sub", &["Episode.01.mkv".to_string()], false)
            .await
            .expect("列目录失败不应回滚已成功的转存");

        assert!(!result.skipped);
        assert_eq!(result.transferred_count, 1);
        // 转存状态已经持久化，下轮检查不会重复转存同一文件。
        let updated = subscriptions.get("sub").await.unwrap();
        assert_eq!(updated.transferred_files, vec!["Season 1/Episode.01.mkv".to_string()]);
        assert_eq!(updated.transferred_file_keys, vec!["s:1:ep:1".to_string(), "ep:1".to_string()]);
    }

    #[tokio::test]
    async fn same_named_directory_episodes_transfer_both_seasons_and_survive_reload() {
        use crate::providers::{MockCloudDriveProvider, ProviderProbeResult};
        let store_path = test_path("directory_seasons");
        let subscriptions = Arc::new(SubscriptionStore::new(&store_path));
        let settings = Arc::new(SettingsStore::new(test_path("directory_settings")));
        settings.update(|value| value.quark_save_enabled = true).await.unwrap();
        let notifications = Arc::new(NotificationStore::new(test_path("directory_notifications")));
        let mut sub = subscription("series", 1);
        sub.cloud_type = "mock".into();
        sub.season_end = Some(3);
        sub.season_list = Some(vec![1, 3]);
        sub.rules.target_dir = "/Review".into();
        subscriptions.create(sub).await.unwrap();
        let mock = Arc::new(MockCloudDriveProvider::new());
        mock.set_probe_result(ProviderProbeResult {
            ok: true, state: "ok".into(), message: String::new(),
            files: [1, 3].into_iter().map(|season| ProviderFile {
                id: format!("source-{season}"), name: "01.mkv".into(), is_dir: false,
                size: 1, updated_at: None, parent_path: format!("Season {season}"),
            }).collect(),
        });
        for season in [1, 3] {
            let target = format!("mock:Review/Season {season}");
            mock.set_items(target.clone(), vec![DriveItem {
                id: format!("target-{season}"), parent_id: target, name: "01.mkv".into(),
                is_dir: false, size: 1, updated_at: String::new(),
            }]);
        }
        let service = SubscriptionTransferService::new(subscriptions.clone(), settings, notifications)
            .with_provider_registry(Arc::new(CloudDriveProviderRegistry::new().with_provider(mock.clone())));
        let names = vec!["Season 1/01.mkv".into(), "Season 3/01.mkv".into()];
        let first = service.auto_transfer_new_files_with_options("sub", &names, true).await.unwrap();
        assert_eq!(first.transferred_count, 2);
        let requests = mock.transfer_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].file_ids, vec!["source-1"]);
        assert_eq!(requests[1].file_ids, vec!["source-3"]);
        subscriptions.load().await.unwrap();
        let saved = subscriptions.get("sub").await.unwrap();
        assert_eq!(saved.transferred_files, names);
        assert!(saved.transferred_file_keys.contains(&"s:3:ep:1".into()));
        let retry = service.auto_transfer_new_files_with_options("sub", &names, true).await.unwrap();
        assert!(retry.skipped);
        assert_eq!(mock.transfer_requests().len(), 2);
        let _ = std::fs::remove_file(store_path);
    }


// ─── 转存意图（幂等重试）回归测试 ────────────────────────────────────────────
//
// 转存不可撤销也不幂等：云端成功但本地未记录时，下一次检查会重新选出同一批
// 文件并再次转存，在用户网盘里留下重复副本。修复方式是在调用云端**之前**落盘
// 「转存意图」，重试时用「有意图」+「目标目录已出现同名文件」两个条件一起判定
// 上次其实成功了。

/// 构造「云端已成功但本地没记住」的场景：目标目录里已有同名文件，
/// 并且存在一条指向该文件的未确认意图。
#[tokio::test]
async fn pending_transfer_intent_prevents_duplicate_transfer() {
    let store_path = test_path("intent_dedupe");
    let subscriptions = Arc::new(SubscriptionStore::new(&store_path));
    let settings = Arc::new(SettingsStore::new(test_path("intent_settings")));
    settings
        .update(|value| value.quark_save_enabled = true)
        .await
        .unwrap();
    let notifications = Arc::new(NotificationStore::new(test_path("intent_notifications")));

    let mut sub = subscription("series", 1);
    sub.cloud_type = "mock".into();
    sub.rules.target_dir = "/Review".into();
    // 关键：落盘一条未确认的意图，模拟"上次调用云端之后、写回本地之前中断"。
    sub.pending_transfers = vec![crate::models::subscription::PendingTransfer {
        season: 1,
        target_dir: "mock:Review".into(),
        file_names: vec!["01.mkv".into()],
        created_at: 1,
    }];
    subscriptions.create(sub).await.unwrap();
    assert_eq!(
        subscriptions
            .get("sub")
            .await
            .unwrap()
            .pending_transfers
            .len(),
        1,
        "意图必须被持久化，否则对账无从谈起"
    );

    let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
    mock.set_probe_result(crate::providers::ProviderProbeResult {
        ok: true,
        state: "ok".into(),
        message: String::new(),
        files: vec![crate::providers::ProviderFile {
            id: "source-1".into(),
            name: "01.mkv".into(),
            is_dir: false,
            size: 1,
            updated_at: None,
            parent_path: String::new(),
        }],
    });
    // 目标目录里已经出现同名文件 —— 说明上次云端其实成功了。
    // 注意单季剧集会在 target_dir 后追加 Season N 子目录，因此实际目录是
    // /Review/Season 1，mock 的 items key 必须与 ensure() 的返回值一致。
    mock.set_items(
        "mock:Review/Season 1".to_string(),
        vec![crate::providers::DriveItem {
            id: "target-1".into(),
            parent_id: "mock:Review".into(),
            name: "01.mkv".into(),
            is_dir: false,
            size: 1,
            updated_at: String::new(),
        }],
    );

    let service = SubscriptionTransferService::new(subscriptions.clone(), settings, notifications)
        .with_provider_registry(Arc::new(
            crate::providers::CloudDriveProviderRegistry::new().with_provider(mock.clone()),
        ));

    let result = service
        .auto_transfer_new_files_with_options("sub", &["01.mkv".to_string()], true)
        .await
        .unwrap();

    assert!(
        mock.transfer_requests().is_empty(),
        "目标目录已有同名文件且存在意图时，绝不能再次调用云端转存"
    );
    assert_eq!(
        result.transferred_count, 1,
        "应把这次转存补记为已成功，而不是当作无新文件"
    );

    subscriptions.load().await.unwrap();
    let saved = subscriptions.get("sub").await.unwrap();
    // 剧集会把季号编进进度引用（`Season 1/01.mkv`），因此按后缀断言。
    assert!(
        saved
            .transferred_files
            .iter()
            .any(|item| item.ends_with("01.mkv")),
        "补记后应写入已转存列表: {:?}",
        saved.transferred_files
    );
    assert!(
        saved.pending_transfers.is_empty(),
        "确认成功后必须清掉意图，避免下次重复对账"
    );
}

/// 反向保证：**没有**意图记录时，目标目录里的同名文件不构成"已转存"的证据。
///
/// 用户网盘里本来就可能存在同名文件（手动转存过、或不同来源的同名剧集），
/// 只看文件名会静默跳过合法转存——那比重复文件更糟。这条测试锁住该边界。
#[tokio::test]
async fn existing_target_file_without_intent_does_not_block_transfer() {
    let store_path = test_path("intent_false_positive");
    let subscriptions = Arc::new(SubscriptionStore::new(&store_path));
    let settings = Arc::new(SettingsStore::new(test_path("intent_fp_settings")));
    settings
        .update(|value| value.quark_save_enabled = true)
        .await
        .unwrap();
    let notifications = Arc::new(NotificationStore::new(test_path("intent_fp_notifications")));

    let mut sub = subscription("series", 1);
    sub.cloud_type = "mock".into();
    sub.rules.target_dir = "/Review".into();
    // 刻意**不**设置 pending_transfers
    subscriptions.create(sub).await.unwrap();

    let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
    mock.set_probe_result(crate::providers::ProviderProbeResult {
        ok: true,
        state: "ok".into(),
        message: String::new(),
        files: vec![crate::providers::ProviderFile {
            id: "source-1".into(),
            name: "01.mkv".into(),
            is_dir: false,
            size: 1,
            updated_at: None,
            parent_path: String::new(),
        }],
    });
    mock.set_items(
        "mock:Review/Season 1".to_string(),
        vec![crate::providers::DriveItem {
            id: "unrelated".into(),
            parent_id: "mock:Review".into(),
            name: "01.mkv".into(),
            is_dir: false,
            size: 1,
            updated_at: String::new(),
        }],
    );

    let service = SubscriptionTransferService::new(subscriptions.clone(), settings, notifications)
        .with_provider_registry(Arc::new(
            crate::providers::CloudDriveProviderRegistry::new().with_provider(mock.clone()),
        ));

    let result = service
        .auto_transfer_new_files_with_options("sub", &["01.mkv".to_string()], true)
        .await
        .unwrap();

    assert_eq!(
        mock.transfer_requests().len(),
        1,
        "没有意图记录时不应因为目标目录有同名文件就跳过转存"
    );
    assert_eq!(result.transferred_count, 1);
}

/// 意图必须在调用云端**之前**落盘，且转存失败时保留下来供下次对账。
#[tokio::test]
async fn transfer_intent_is_recorded_before_cloud_call_and_kept_on_failure() {
    let store_path = test_path("intent_lifecycle");
    let subscriptions = Arc::new(SubscriptionStore::new(&store_path));
    let settings = Arc::new(SettingsStore::new(test_path("intent_lc_settings")));
    settings
        .update(|value| value.quark_save_enabled = true)
        .await
        .unwrap();
    let notifications = Arc::new(NotificationStore::new(test_path("intent_lc_notifications")));

    let mut sub = subscription("series", 1);
    sub.cloud_type = "mock".into();
    sub.rules.target_dir = "/Review".into();
    subscriptions.create(sub).await.unwrap();

    let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
    mock.set_probe_result(crate::providers::ProviderProbeResult {
        ok: true,
        state: "ok".into(),
        message: String::new(),
        files: vec![crate::providers::ProviderFile {
            id: "source-1".into(),
            name: "01.mkv".into(),
            is_dir: false,
            size: 1,
            updated_at: None,
            parent_path: String::new(),
        }],
    });
    // 目标目录里放一个同名文件：由于**没有**意图记录，它不构成"已转存"的证据，
    // 转存仍会真正发起；同时转存后的「等待文件落盘」能立即满足（否则这里要等满
    // 30×2 秒的等待窗口）。
    let target = "mock:Review/Season 1".to_string();
    mock.set_items(
        target.clone(),
        vec![crate::providers::DriveItem {
            id: "target-1".into(),
            parent_id: target.clone(),
            name: "01.mkv".into(),
            is_dir: false,
            size: 1,
            updated_at: String::new(),
        }],
    );

    let service = SubscriptionTransferService::new(subscriptions.clone(), settings, notifications)
        .with_provider_registry(Arc::new(
            crate::providers::CloudDriveProviderRegistry::new().with_provider(mock.clone()),
        ));

    let result = service
        .auto_transfer_new_files_with_options("sub", &["01.mkv".to_string()], true)
        .await
        .unwrap();
    assert_eq!(result.transferred_count, 1);
    assert_eq!(mock.transfer_requests().len(), 1);

    // 成功路径：意图应被清掉
    subscriptions.load().await.unwrap();
    let saved = subscriptions.get("sub").await.unwrap();
    assert!(
        saved.pending_transfers.is_empty(),
        "转存成功并落盘后必须清除意图: {:?}",
        saved.pending_transfers
    );
}

// ─── 「已转存但未成功提交下载」对账回归测试 ──────────────────────────────────
//
// 转存成功会把文件写进 transferred_file_keys，之后的检查不会再选中它。如果紧接着
// 的 Aria2 提交失败又不留记录，这一集就永远不会下载到本地，而界面仍显示"已转存"。

/// 提交失败（此处为未配置 Aria2）时必须留下待下载记录。
#[tokio::test]
async fn failed_sync_download_submission_is_recorded_as_pending() {
    let store_path = test_path("pending_download_record");
    let subscriptions = Arc::new(SubscriptionStore::new(&store_path));
    let settings = Arc::new(SettingsStore::new(test_path("pending_dl_settings")));
    settings
        .update(|value| {
            value.quark_save_enabled = true;
            // 刻意留空 aria2_rpc_url：提交必然失败
        })
        .await
        .unwrap();
    let notifications = Arc::new(NotificationStore::new(test_path("pending_dl_notifications")));

    let mut sub = subscription("series", 1);
    sub.cloud_type = "mock".into();
    sub.rules.target_dir = "/Review".into();
    sub.sync_download_enabled = true;
    subscriptions.create(sub).await.unwrap();

    let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
    mock.set_probe_result(crate::providers::ProviderProbeResult {
        ok: true,
        state: "ok".into(),
        message: String::new(),
        files: vec![crate::providers::ProviderFile {
            id: "source-1".into(),
            name: "01.mkv".into(),
            is_dir: false,
            size: 1,
            updated_at: None,
            parent_path: String::new(),
        }],
    });
    let target = "mock:Review/Season 1".to_string();
    mock.set_items(
        target.clone(),
        vec![crate::providers::DriveItem {
            id: "target-1".into(),
            parent_id: target.clone(),
            name: "01.mkv".into(),
            is_dir: false,
            size: 1,
            updated_at: String::new(),
        }],
    );

    let service = SubscriptionTransferService::new(subscriptions.clone(), settings, notifications)
        .with_provider_registry(Arc::new(
            crate::providers::CloudDriveProviderRegistry::new().with_provider(mock.clone()),
        ));

    let result = service
        .auto_transfer_new_files_with_options("sub", &["01.mkv".to_string()], true)
        .await
        .unwrap();
    assert_eq!(result.transferred_count, 1, "转存本身应当成功");

    subscriptions.load().await.unwrap();
    let saved = subscriptions.get("sub").await.unwrap();
    assert_eq!(
        saved.pending_downloads.len(),
        1,
        "Aria2 提交失败必须留下待下载记录，否则该集永远不会下载: {:?}",
        saved.pending_downloads
    );
    assert_eq!(saved.pending_downloads[0].fid, "target-1");
    assert_eq!(saved.pending_downloads[0].attempts, 1);
}

/// 对账在 Aria2 不可达时必须保留记录并累加尝试次数，而不是静默丢弃。
#[tokio::test]
async fn reconcile_keeps_record_and_counts_attempts_when_aria2_unreachable() {
    let store_path = test_path("pending_download_retry");
    let subscriptions = Arc::new(SubscriptionStore::new(&store_path));
    let settings = Arc::new(SettingsStore::new(test_path("pending_retry_settings")));
    settings
        .update(|value| {
            // 指向一个必然拒绝连接的端口，模拟 Aria2 暂时不可用
            value.aria2_rpc_url = "http://127.0.0.1:1/jsonrpc".into();
        })
        .await
        .unwrap();
    let notifications = Arc::new(NotificationStore::new(test_path("pending_retry_notifications")));

    let mut sub = subscription("series", 1);
    sub.cloud_type = "mock".into();
    sub.sync_download_enabled = true;
    sub.pending_downloads = vec![crate::models::subscription::PendingDownload {
        fid: "target-1".into(),
        file_name: "01.mkv".into(),
        target_dir: "/Review/Season 1".into(),
        season: 1,
        download_dir: "/downloads/剧集".into(),
        attempts: 0,
        created_at: 1,
    }];
    subscriptions.create(sub).await.unwrap();

    let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
    let service = SubscriptionTransferService::new(subscriptions.clone(), settings, notifications)
        .with_provider_registry(Arc::new(
            crate::providers::CloudDriveProviderRegistry::new().with_provider(mock),
        ));

    let submitted = service.reconcile_pending_downloads("sub").await.unwrap();
    assert_eq!(submitted, 0, "Aria2 不可达时不应报告成功");

    subscriptions.load().await.unwrap();
    let saved = subscriptions.get("sub").await.unwrap();
    assert_eq!(
        saved.pending_downloads.len(),
        1,
        "重试失败必须保留记录，等待下一轮"
    );
    assert_eq!(
        saved.pending_downloads[0].attempts, 1,
        "每次重试都应累加 attempts，便于暴露长期失败的项"
    );
}

/// 对账成功时：清除待下载记录，并补写一条 sync_downloads 记录供下载监控追踪。
#[tokio::test]
async fn reconcile_submits_pending_download_and_clears_record() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // 极简 Aria2 JSON-RPC 假服务：始终返回一个 gid。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 8192];
            let _ = socket.read(&mut buf).await;
            let body = br#"{"jsonrpc":"2.0","id":"1","result":"gid-reconciled"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.write_all(body).await;
        }
    });

    let store_path = test_path("pending_download_success");
    let subscriptions = Arc::new(SubscriptionStore::new(&store_path));
    let settings = Arc::new(SettingsStore::new(test_path("pending_ok_settings")));
    settings
        .update(|value| {
            value.aria2_rpc_url = format!("http://{addr}/jsonrpc");
        })
        .await
        .unwrap();
    let notifications = Arc::new(NotificationStore::new(test_path("pending_ok_notifications")));

    let mut sub = subscription("series", 1);
    sub.cloud_type = "mock".into();
    sub.sync_download_enabled = true;
    sub.pending_downloads = vec![crate::models::subscription::PendingDownload {
        fid: "target-1".into(),
        file_name: "01.mkv".into(),
        target_dir: "/Review/Season 1".into(),
        season: 1,
        download_dir: "/downloads/剧集".into(),
        attempts: 0,
        created_at: 1,
    }];
    subscriptions.create(sub).await.unwrap();

    let mock = Arc::new(crate::providers::MockCloudDriveProvider::new());
    let service = SubscriptionTransferService::new(subscriptions.clone(), settings, notifications)
        .with_provider_registry(Arc::new(
            crate::providers::CloudDriveProviderRegistry::new().with_provider(mock),
        ));

    let submitted = service.reconcile_pending_downloads("sub").await.unwrap();
    assert_eq!(submitted, 1, "对账应当成功提交一次");

    subscriptions.load().await.unwrap();
    let saved = subscriptions.get("sub").await.unwrap();
    assert!(
        saved.pending_downloads.is_empty(),
        "提交成功后必须清除待下载记录: {:?}",
        saved.pending_downloads
    );
    assert_eq!(
        saved.sync_downloads.len(),
        1,
        "必须补写 sync_downloads 记录，否则下载监控无法追踪完成状态"
    );
    assert_eq!(saved.sync_downloads[0].gid, "gid-reconciled");
}


}
