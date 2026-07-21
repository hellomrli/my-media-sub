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
            sync_downloads: vec![],
            strm_enabled: false,
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

        assert_eq!(target, "/连续剧/庆余年（2024）/Season 1");
    }

    #[test]
    fn determine_target_directory_does_not_append_season_for_movie() {
        let settings = Settings {
            quark_save_movie_dir: "/电影".to_string(),
            ..Default::default()
        };
        let sub = subscription("movie", 1);

        let target = determine_subscription_target_directory(&sub, &settings);

        assert_eq!(target, "/电影/庆余年（2024）");
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
        assert_eq!(target, "/连续剧/庆余年（2024）");
        assert_eq!(
            season_target_directory(&target, 3),
            "/连续剧/庆余年（2024）/Season 3"
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
    async fn auto_transfer_new_files_respects_subscription_auto_download_switch() {
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
        let result = service
            .auto_transfer_new_files_with_options("sub", &["Episode.01.mkv".to_string()], false)
            .await
            .unwrap();

        assert!(result.skipped);
        assert_eq!(result.transferred_count, 0);
        assert_eq!(result.reason, "自动下载新订阅项未启用");
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
            .mark_files_as_transferred(&sub, &["178-4k.mkv".to_string()])
            .await
            .unwrap();

        let updated = subscriptions.get("sub").await.unwrap();
        assert_eq!(updated.transferred_files, vec!["178-4k.mkv".to_string()]);
        assert_eq!(updated.transferred_file_keys, vec!["ep:178".to_string()]);
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
        assert_eq!(updated.transferred_files, vec!["Episode.01.mkv".to_string()]);
        assert_eq!(updated.transferred_file_keys, vec!["ep:1".to_string()]);
    }

}
