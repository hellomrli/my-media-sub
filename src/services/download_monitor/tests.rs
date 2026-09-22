//! `download_monitor` 的单元测试。
//!
//! 从这里外移（原本是母文件里 1207 行的内联 `mod tests`）：母文件因此从 2284 行
//! 降到约 1080 行，生产代码与测试的边界变清晰。测试内容未做任何修改，
//! 因此 `use super::*;` 的解析目标与原来完全一致。

use super::*;
use crate::models::Notification;

fn completed_task() -> Aria2Task {
    Aria2Task {
        gid: "gid-1".to_string(),
        status: "complete".to_string(),
        file_name: "Show.S01E01.mkv".to_string(),
        total_length: 1024,
        completed_length: 1024,
        download_speed: 0,
        upload_speed: 0,
        connections: 0,
        progress: 100.0,
        eta_seconds: None,
        dir: "/downloads/anime".to_string(),
        error_code: String::new(),
        error_message: String::new(),
        files: vec![],
    }
}

fn task_with(gid: &str, file_name: &str, status: &str) -> Aria2Task {
    let failed = status == "error";
    Aria2Task {
        gid: gid.to_string(),
        status: status.to_string(),
        file_name: file_name.to_string(),
        total_length: 1024,
        completed_length: if failed { 0 } else { 1024 },
        download_speed: 0,
        upload_speed: 0,
        connections: 0,
        progress: if failed { 0.0 } else { 100.0 },
        eta_seconds: None,
        dir: "/downloads/anime".to_string(),
        error_code: if failed {
            "1".to_string()
        } else {
            String::new()
        },
        error_message: if failed {
            "下载地址失效".to_string()
        } else {
            String::new()
        },
        files: vec![],
    }
}

#[test]
fn dedupe_key_cache_evicts_oldest_keys_beyond_cap() {
    let mut cache = DedupeKeyCache::default();
    for index in 0..(MAX_TRACKED_DEDUPE_KEYS + 10) {
        cache.insert(format!("gid:{index}"));
    }
    assert_eq!(cache.keys.len(), MAX_TRACKED_DEDUPE_KEYS);
    assert_eq!(cache.order.len(), MAX_TRACKED_DEDUPE_KEYS);
    assert!(!cache.contains("gid:0"));
    assert!(!cache.contains("gid:9"));
    assert!(cache.contains("gid:10"));
    assert!(cache.contains(&format!("gid:{}", MAX_TRACKED_DEDUPE_KEYS + 9)));

    // 重复插入不产生重复的淘汰顺序条目。
    let newest = format!("gid:{}", MAX_TRACKED_DEDUPE_KEYS + 9);
    cache.insert(newest.clone());
    assert_eq!(cache.order.len(), MAX_TRACKED_DEDUPE_KEYS);
    assert!(cache.contains(&newest));
}

#[test]
fn failed_claim_can_be_removed_for_retry() {
    let mut cache = DedupeKeyCache::default();
    cache.insert("gid:retry".to_string());
    cache.insert("file:retry".to_string());

    cache.remove("gid:retry");
    cache.remove("file:retry");

    assert!(!cache.contains("gid:retry"));
    assert!(!cache.contains("file:retry"));
    assert!(cache.order.is_empty());
}

#[tokio::test]
async fn same_named_downloads_complete_only_their_own_selected_seasons() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};
    let dir = std::env::temp_dir().join(format!("mms-download-seasons-{}", uuid::Uuid::new_v4()));
    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    let sub = serde_json::from_value(json!({
        "id":"download-seasons", "title":"Example", "url":"https://pan.quark.cn/s/test",
        "media_type":"series", "season":2, "total_episode_number":1,
        "sync_download_enabled":true,
        "known_files":["Season 1/01.mkv"], "transferred_files":["Season 1/01.mkv"],
        "sync_downloads":[{"gid":"gid-1", "file_name":"01.mkv", "download_dir":"/downloads",
            "target_dir":"/series/Example/Season 1", "submitted_at":1}],
        "created_at":1,"updated_at":1,"last_checked_at":1
    }))
    .unwrap();
    context.subscription_store.create(sub).await.unwrap();
    // Legacy notifications carry a basename and must not reassign S1 to S2.
    context
        .notification_store
        .add(Notification {
            id: "old-transfer".into(),
            level: "success".into(),
            event: "subscription_transferred".into(),
            title: "Example".into(),
            message: String::new(),
            read: false,
            created_at: 1,
            meta: HashMap::from([
                ("subscription_id".into(), json!("download-seasons")),
                (
                    "sync_downloads".into(),
                    json!([{"gid":"gid-1", "file_name":"01.mkv"}]),
                ),
            ]),
        })
        .await
        .unwrap();
    context
        .download_monitor
        .complete_subscription_for_download(&task_with("gid-1", "01.mkv", "complete"))
        .await
        .unwrap();
    let sub = context
        .subscription_store
        .get("download-seasons")
        .await
        .unwrap();
    assert!(sub.sync_downloads[0].completed_at.is_some());
    assert!(!sub.completed);
    context
        .subscription_store
        .update("download-seasons", |sub| {
            sub.season = 1;
            sub.season_end = Some(3);
            sub.season_list = Some(vec![1, 3]);
            sub.sync_downloads.push(SyncDownloadRecord {
                gid: "gid-3".into(),
                file_name: "01.mkv".into(),
                download_dir: "/downloads/Season 3".into(),
                target_dir: "/series/Example/Season 3".into(),
                submitted_at: 2,
                completed_at: None,
            });
        })
        .await
        .unwrap();
    context
        .download_monitor
        .complete_subscription_for_download(&task_with("gid-1", "01.mkv", "complete"))
        .await
        .unwrap();
    assert!(
        !context
            .subscription_store
            .get("download-seasons")
            .await
            .unwrap()
            .completed
    );
    context
        .download_monitor
        .complete_subscription_for_download(&task_with("gid-3", "01.mkv", "complete"))
        .await
        .unwrap();
    assert!(
        context
            .subscription_store
            .get("download-seasons")
            .await
            .unwrap()
            .completed
    );
    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn persisted_download_mapping_completes_without_transfer_notification() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};
    use crate::models::SyncDownloadRecord;

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-monitor-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    let mut subscription: Subscription = serde_json::from_value(json!({
        "id": "sub-download",
        "title": "Show",
        "url": "https://pan.quark.cn/s/test",
        "created_at": 1,
        "updated_at": 1,
        "last_checked_at": 1
    }))
    .unwrap();
    subscription.media_type = "series".to_string();
    subscription.total_episode_number = Some(1);
    subscription.sync_download_enabled = true;
    subscription.transferred_files = vec!["Show.S01E01.mkv".to_string()];
    subscription.sync_downloads = vec![SyncDownloadRecord {
        gid: "gid-1".to_string(),
        file_name: "Show.S01E01.mkv".to_string(),
        download_dir: "/downloads/anime".to_string(),
        target_dir: "/series/Show/Season 1".to_string(),
        submitted_at: 1,
        completed_at: None,
    }];
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    // 模拟旧流程已经写入下载完成通知，但订阅状态更新失败；这里没有任何
    // subscription_transferred 通知，业务关联只能来自持久下载记录。
    context
        .notification_store
        .add(Notification {
            id: "existing-download-notification".to_string(),
            level: "success".to_string(),
            event: "download_completed".to_string(),
            title: "下载完成: Show.S01E01.mkv".to_string(),
            message: "already recorded".to_string(),
            meta: HashMap::from([("gid".to_string(), json!("gid-1"))]),
            read: false,
            created_at: 1,
        })
        .await
        .unwrap();

    context
        .download_monitor
        .notify_completed_downloads(&[completed_task()])
        .await;

    let updated = context
        .subscription_store
        .get("sub-download")
        .await
        .unwrap();
    assert!(updated.completed);
    assert_eq!(updated.status, "completed");
    assert!(updated.sync_downloads[0].completed_at.is_some());
    let notifications = context.notification_store.list(true).await;
    assert_eq!(
        notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .count(),
        1
    );

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn batch_downloads_merge_into_one_notification_after_all_complete() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};
    use crate::models::{MediaMetadata, MetadataProvider, SyncDownloadRecord};

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-batch-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    let mut subscription: Subscription = serde_json::from_value(json!({
        "id": "sub-batch",
        "title": "Show",
        "url": "https://pan.quark.cn/s/test",
        "created_at": 1,
        "updated_at": 1,
        "last_checked_at": 1
    }))
    .unwrap();
    subscription.media_type = "series".to_string();
    subscription.sync_download_enabled = true;
    subscription.metadata = Some(MediaMetadata {
        provider: MetadataProvider::Tmdb,
        provider_id: "1".to_string(),
        title: "Show".to_string(),
        original_title: String::new(),
        media_type: "series".to_string(),
        overview: String::new(),
        poster_url: Some("https://image.tmdb.org/t/p/w500/poster.jpg".to_string()),
        backdrop_url: None,
        release_date: None,
        vote_average: None,
        number_of_episodes: None,
        number_of_seasons: None,
        seasons: vec![],
        next_episode_to_air: None,
        episodes: vec![],
    });
    subscription.sync_downloads = vec![
        SyncDownloadRecord {
            gid: "gid-1".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: None,
        },
        SyncDownloadRecord {
            gid: "gid-2".to_string(),
            file_name: "Show.S01E02.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: None,
        },
    ];
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    // 第一批只完成一个文件：不能提前发通知。
    context
        .download_monitor
        .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "complete")])
        .await;
    let notifications = context.notification_store.list(true).await;
    assert_eq!(
        notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .count(),
        0
    );

    // 全部完成后合并为一条通知。
    context
        .download_monitor
        .notify_completed_downloads(&[task_with("gid-2", "Show.S01E02.mkv", "complete")])
        .await;
    let notifications = context.notification_store.list(true).await;
    let completed = notifications
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .collect::<Vec<_>>();
    assert_eq!(completed.len(), 1);
    assert!(completed[0].title.contains("2 个文件"));
    assert!(completed[0].message.contains("Show.S01E01.mkv"));
    assert!(completed[0].message.contains("Show.S01E02.mkv"));
    assert_eq!(
        completed[0]
            .meta
            .get("gids")
            .and_then(Value::as_array)
            .map(|gids| gids.len()),
        Some(2)
    );
    assert_eq!(
        completed[0].meta.get("poster_url").and_then(Value::as_str),
        Some("https://image.tmdb.org/t/p/w500/poster.jpg")
    );

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn failed_download_notifies_immediately() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};
    use crate::models::SyncDownloadRecord;

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-failed-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    let mut subscription: Subscription = serde_json::from_value(json!({
        "id": "sub-failed",
        "title": "Show",
        "url": "https://pan.quark.cn/s/test",
        "created_at": 1,
        "updated_at": 1,
        "last_checked_at": 1
    }))
    .unwrap();
    subscription.sync_download_enabled = true;
    subscription.sync_downloads = vec![SyncDownloadRecord {
        gid: "gid-1".to_string(),
        file_name: "Show.S01E01.mkv".to_string(),
        download_dir: "/downloads/anime".to_string(),
        target_dir: "/series/Show/Season 1".to_string(),
        submitted_at: 100,
        completed_at: None,
    }];
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    context
        .download_monitor
        .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "error")])
        .await;

    let notifications = context.notification_store.list(true).await;
    let failures = notifications
        .iter()
        .filter(|notification| notification.event == "download_failed")
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].level, "error");
    assert!(failures[0].title.contains("下载失败"));
    assert!(failures[0].message.contains("下载地址失效"));

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn failed_download_does_not_block_remaining_batch_success() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};
    use crate::models::SyncDownloadRecord;

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-batch-failed-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    let mut subscription: Subscription = serde_json::from_value(json!({
        "id": "sub-batch-failed",
        "title": "Show",
        "url": "https://pan.quark.cn/s/test",
        "created_at": 1,
        "updated_at": 1,
        "last_checked_at": 1
    }))
    .unwrap();
    subscription.sync_download_enabled = true;
    subscription.sync_downloads = vec![
        SyncDownloadRecord {
            gid: "gid-1".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: None,
        },
        SyncDownloadRecord {
            gid: "gid-2".to_string(),
            file_name: "Show.S01E02.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: None,
        },
    ];
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    context
        .download_monitor
        .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "error")])
        .await;
    let notifications = context.notification_store.list(true).await;
    assert_eq!(
        notifications
            .iter()
            .filter(|notification| notification.event == "download_failed")
            .count(),
        1
    );
    assert_eq!(
        notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .count(),
        0
    );

    context
        .download_monitor
        .notify_completed_downloads(&[task_with("gid-2", "Show.S01E02.mkv", "complete")])
        .await;
    let notifications = context.notification_store.list(true).await;
    let completed = notifications
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .collect::<Vec<_>>();
    assert_eq!(completed.len(), 1);
    assert!(completed[0].title.contains("1 个文件"));
    assert!(completed[0].message.contains("Show.S01E02.mkv"));
    assert!(!completed[0].message.contains("Show.S01E01.mkv"));

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn completed_before_batch_failure_still_sends_merged_notification() {
    // 回归：批次“先完成后失败”时，先到的完成通知在批次未结算时只应
    // 挂起而不应永久持有去重 claim——否则失败通知结算批次后，合并
    // 完成通知永远没有机会再发出。
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};
    use crate::models::SyncDownloadRecord;

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-batch-completed-first-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    let mut subscription: Subscription = serde_json::from_value(json!({
        "id": "sub-batch-completed-first",
        "title": "Show",
        "url": "https://pan.quark.cn/s/test",
        "created_at": 1,
        "updated_at": 1,
        "last_checked_at": 1
    }))
    .unwrap();
    subscription.sync_download_enabled = true;
    subscription.sync_downloads = vec![
        SyncDownloadRecord {
            gid: "gid-1".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: None,
        },
        SyncDownloadRecord {
            gid: "gid-2".to_string(),
            file_name: "Show.S01E02.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: None,
        },
    ];
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    // 第一轮：gid-1 完成先到，同批 gid-2 仍在下载 → 批次未结算，无通知。
    context
        .download_monitor
        .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "complete")])
        .await;
    let notifications = context.notification_store.list(true).await;
    assert_eq!(
        notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .count(),
        0
    );
    assert_eq!(
        notifications
            .iter()
            .filter(|notification| notification.event == "download_failed")
            .count(),
        0
    );

    // 第二轮：gid-2 失败 → 立即写失败通知，批次就此结算。
    context
        .download_monitor
        .notify_completed_downloads(&[task_with("gid-2", "Show.S01E02.mkv", "error")])
        .await;
    let notifications = context.notification_store.list(true).await;
    assert_eq!(
        notifications
            .iter()
            .filter(|notification| notification.event == "download_failed")
            .count(),
        1
    );
    assert_eq!(
        notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .count(),
        0
    );

    // 第三轮：gid-1 的完成事件再次被扫描到（claim 已释放）→ 合并通知发出。
    context
        .download_monitor
        .notify_completed_downloads(&[task_with("gid-1", "Show.S01E01.mkv", "complete")])
        .await;
    let notifications = context.notification_store.list(true).await;
    let completed = notifications
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .collect::<Vec<_>>();
    assert_eq!(completed.len(), 1);
    assert!(completed[0].title.contains("1 个文件"));
    assert!(completed[0].message.contains("Show.S01E01.mkv"));
    assert!(!completed[0].message.contains("Show.S01E02.mkv"));

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn merged_download_notification_matches_any_batch_gid() {
    let task = completed_task();
    let history = vec![Notification {
        id: "merged".to_string(),
        level: "success".to_string(),
        event: "download_completed".to_string(),
        title: "下载完成: Show（2 个文件）".to_string(),
        message: "• Show.S01E01.mkv\n• Show.S01E02.mkv".to_string(),
        meta: HashMap::from([("gids".to_string(), json!(["gid-1", "gid-2"]))]),
        read: false,
        created_at: 1,
    }];

    assert!(completed_download_already_recorded(
        &history,
        &HashSet::new(),
        &task
    ));
}

#[test]
fn completed_download_history_matches_when_gid_changes() {
    let task = completed_task();
    let history = vec![Notification {
        id: "n1".to_string(),
        level: "success".to_string(),
        event: "download_completed".to_string(),
        title: "下载完成: Show.S01E01.mkv".to_string(),
        message: "文件：Show.S01E01.mkv\n目录：/downloads/anime\n大小：1.00 KB".to_string(),
        meta: HashMap::from([
            ("gid".to_string(), json!("old-gid")),
            ("file_name".to_string(), json!("Show.S01E01.mkv")),
            ("dir".to_string(), json!("/downloads/anime")),
            ("total_length".to_string(), json!(1024u64)),
        ]),
        read: false,
        created_at: 1,
    }];

    assert!(completed_download_already_recorded(
        &history,
        &HashSet::new(),
        &task
    ));
}

#[test]
fn completed_download_history_uses_push_jobs_when_notifications_were_cleared() {
    let task = completed_task();
    let (title, message) = download_completed_title_message(&task);
    let pushed_downloads = HashSet::from([(title, message)]);

    assert!(completed_download_already_recorded(
        &[],
        &pushed_downloads,
        &task
    ));
}

/// 重启恢复场景：记录的完成状态已落盘、内存去重缓存为空，同一批次的
/// 任务在同一轮扫描中全部出现。即使先处理的任务已发出合并通知，后续
/// 任务也必须能看见它，整批只能补发一条。
#[tokio::test]
async fn settled_batch_in_one_poll_sends_single_merged_notification() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};
    use crate::models::SyncDownloadRecord;

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-batch-restart-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    let mut subscription: Subscription = serde_json::from_value(json!({
        "id": "sub-batch-restart",
        "title": "Show",
        "url": "https://pan.quark.cn/s/test",
        "created_at": 1,
        "updated_at": 1,
        "last_checked_at": 1
    }))
    .unwrap();
    subscription.sync_download_enabled = true;
    subscription.completed = true;
    subscription.sync_downloads = vec![
        SyncDownloadRecord {
            gid: "gid-1".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: Some(1),
        },
        SyncDownloadRecord {
            gid: "gid-2".to_string(),
            file_name: "Show.S01E02.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: Some(1),
        },
    ];
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    // 两个已完成任务出现在同一轮扫描中。
    context
        .download_monitor
        .notify_completed_downloads(&[
            task_with("gid-1", "Show.S01E01.mkv", "complete"),
            task_with("gid-2", "Show.S01E02.mkv", "complete"),
        ])
        .await;

    let notifications = context.notification_store.list(true).await;
    let completed = notifications
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .collect::<Vec<_>>();
    assert_eq!(completed.len(), 1, "同批只能补发一条合并通知");
    assert!(completed[0].title.contains("2 个文件"));

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

/// 本地图片测试服务器，返回请求计数与 URL。
async fn spawn_image_server() -> (Arc<std::sync::atomic::AtomicUsize>, String) {
    use std::sync::atomic::AtomicUsize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = requests.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf).await;
            let body = b"fake-jpeg-bytes";
            let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.write_all(body).await;
        }
    });
    (requests, format!("http://{addr}/poster.jpg"))
}

fn completed_task_in_dir(gid: &str, file_name: &str, dir: &str) -> Aria2Task {
    Aria2Task {
        gid: gid.to_string(),
        status: "complete".to_string(),
        file_name: file_name.to_string(),
        total_length: 1024,
        completed_length: 1024,
        download_speed: 0,
        upload_speed: 0,
        connections: 0,
        progress: 100.0,
        eta_seconds: None,
        dir: dir.to_string(),
        error_code: String::new(),
        error_message: String::new(),
        files: vec![],
    }
}

async fn metadata_subscription(dir: &str) -> Subscription {
    use crate::models::{MediaMetadata, MediaMetadataSeason, MetadataProvider};

    let mut subscription: Subscription = serde_json::from_value(json!({
        "id": "sub-metadata",
        "title": "Show",
        "url": "https://pan.quark.cn/s/test",
        "created_at": 1,
        "updated_at": 1,
        "last_checked_at": 1
    }))
    .unwrap();
    subscription.media_type = "series".to_string();
    subscription.sync_download_enabled = true;
    subscription.metadata = Some(MediaMetadata {
        provider: MetadataProvider::Tmdb,
        provider_id: "123".to_string(),
        title: "Show".to_string(),
        original_title: "Original Show".to_string(),
        media_type: "series".to_string(),
        overview: "简介".to_string(),
        poster_url: Some("http://127.0.0.1:1/poster.jpg".to_string()),
        backdrop_url: None,
        release_date: Some("2024-01-01".to_string()),
        vote_average: Some(8.2),
        number_of_episodes: Some(1),
        number_of_seasons: Some(1),
        seasons: vec![MediaMetadataSeason {
            season_number: 1,
            episode_count: Some(1),
            name: "Season 1".to_string(),
            air_date: Some("2024-01-01".to_string()),
            poster_url: Some("http://127.0.0.1:1/poster.jpg".to_string()),
        }],
        next_episode_to_air: None,
        episodes: vec![],
    });
    subscription.sync_downloads = vec![SyncDownloadRecord {
        gid: "gid-1".to_string(),
        file_name: "Show.S01E01.mkv".to_string(),
        download_dir: dir.to_string(),
        target_dir: "/series/Show/Season 1".to_string(),
        submitted_at: 100,
        completed_at: None,
    }];
    subscription
}

/// 完整链路：开关打开时，下载完成会按 TMDB 元数据把 NFO/海报写入本地下载目录。
#[tokio::test]
async fn completed_download_writes_media_metadata_files() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-metadata-{}",
        uuid::Uuid::new_v4()
    ));
    let season_dir = dir.join("Show/Season 1");
    std::fs::create_dir_all(&season_dir).unwrap();
    let (requests, url) = spawn_image_server().await;

    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    context
        .settings_store
        .update(|settings| settings.media_metadata_files_enabled = true)
        .await
        .unwrap();
    let mut subscription = metadata_subscription(season_dir.to_str().unwrap()).await;
    if let Some(metadata) = subscription.metadata.as_mut() {
        metadata.poster_url = Some(url.clone());
        metadata.seasons[0].poster_url = Some(url.clone());
    }
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    context
        .download_monitor
        .notify_completed_downloads(&[completed_task_in_dir(
            "gid-1",
            "Show.S01E01.mkv",
            season_dir.to_str().unwrap(),
        )])
        .await;

    let show_root = dir.join("Show");
    let tvshow = std::fs::read_to_string(show_root.join("tvshow.nfo")).unwrap();
    assert!(tvshow.contains("<title>Show</title>"));
    assert!(tvshow.contains("<uniqueid type=\"tmdb\">123</uniqueid>"));
    assert!(std::fs::read_to_string(season_dir.join("season.nfo"))
        .unwrap()
        .contains("<seasonnumber>1</seasonnumber>"));
    assert_eq!(
        std::fs::read(show_root.join("poster.jpg")).unwrap(),
        b"fake-jpeg-bytes"
    );
    assert_eq!(
        std::fs::read(season_dir.join("poster.jpg")).unwrap(),
        b"fake-jpeg-bytes"
    );
    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

/// 开关关闭时不写任何媒体库元数据文件。
#[tokio::test]
async fn completed_download_skips_metadata_files_when_disabled() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-metadata-off-{}",
        uuid::Uuid::new_v4()
    ));
    let season_dir = dir.join("Show/Season 1");
    std::fs::create_dir_all(&season_dir).unwrap();
    let (requests, url) = spawn_image_server().await;

    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    let mut subscription = metadata_subscription(season_dir.to_str().unwrap()).await;
    if let Some(metadata) = subscription.metadata.as_mut() {
        metadata.poster_url = Some(url.clone());
        metadata.seasons[0].poster_url = Some(url.clone());
    }
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    context
        .download_monitor
        .notify_completed_downloads(&[completed_task_in_dir(
            "gid-1",
            "Show.S01E01.mkv",
            season_dir.to_str().unwrap(),
        )])
        .await;

    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(!dir.join("Show/tvshow.nfo").exists());
    assert!(!season_dir.join("poster.jpg").exists());

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

/// 重启回放：第二轮重新扫描已完成任务时，通知不再发、文件不再重写，
/// 但第一轮写入的内容保持不变。
#[tokio::test]
async fn replay_after_restart_does_not_rewrite_metadata_files() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-metadata-replay-{}",
        uuid::Uuid::new_v4()
    ));
    let season_dir = dir.join("Show/Season 1");
    std::fs::create_dir_all(&season_dir).unwrap();
    let (requests, url) = spawn_image_server().await;

    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    context
        .settings_store
        .update(|settings| settings.media_metadata_files_enabled = true)
        .await
        .unwrap();
    let mut subscription = metadata_subscription(season_dir.to_str().unwrap()).await;
    if let Some(metadata) = subscription.metadata.as_mut() {
        metadata.poster_url = Some(url.clone());
        metadata.seasons[0].poster_url = Some(url.clone());
    }
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    let task = completed_task_in_dir("gid-1", "Show.S01E01.mkv", season_dir.to_str().unwrap());
    context
        .download_monitor
        .notify_completed_downloads(&[task])
        .await;
    let tvshow_path = dir.join("Show/tvshow.nfo");
    let first_mtime = std::fs::metadata(&tvshow_path).unwrap().modified().unwrap();

    // 模拟重启：内存去重缓存清空后再次扫描同一批任务。
    context
        .download_monitor
        .notified_completed_downloads
        .write()
        .await
        .keys
        .clear();
    let task = completed_task_in_dir("gid-1", "Show.S01E01.mkv", season_dir.to_str().unwrap());
    context
        .download_monitor
        .notify_completed_downloads(&[task])
        .await;

    let second_mtime = std::fs::metadata(&tvshow_path).unwrap().modified().unwrap();
    assert_eq!(first_mtime, second_mtime, "幂等跳过时不应重写 NFO");
    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 4);
    let notifications = context.notification_store.list(true).await;
    assert_eq!(
        notifications
            .iter()
            .filter(|notification| notification.event == "download_completed")
            .count(),
        1,
        "回放不得重复发送下载完成通知"
    );

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

/// 升级场景：旧版本（2.4.0 之前）已按文件逐条发送「下载完成」通知，
/// meta 只有单个 gid。升级重启后不得把同一批次再次合并补发。
#[tokio::test]
async fn legacy_per_file_history_does_not_trigger_merged_resend() {
    use crate::app::AppContext;
    use crate::config::{Config, ServerConfig};
    use crate::models::SyncDownloadRecord;

    let dir = std::env::temp_dir().join(format!(
        "my-media-sub-download-batch-legacy-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let context = AppContext::new(&Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
        },
        data_dir: dir.clone(),
    })
    .await
    .unwrap();
    let mut subscription: Subscription = serde_json::from_value(json!({
        "id": "sub-batch-legacy",
        "title": "Show",
        "url": "https://pan.quark.cn/s/test",
        "created_at": 1,
        "updated_at": 1,
        "last_checked_at": 1
    }))
    .unwrap();
    subscription.sync_download_enabled = true;
    subscription.completed = true;
    subscription.sync_downloads = vec![
        SyncDownloadRecord {
            gid: "gid-1".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: Some(1),
        },
        SyncDownloadRecord {
            gid: "gid-2".to_string(),
            file_name: "Show.S01E02.mkv".to_string(),
            download_dir: "/downloads/anime".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 100,
            completed_at: Some(1),
        },
    ];
    context
        .subscription_store
        .create(subscription)
        .await
        .unwrap();

    // 旧版本遗留的逐文件通知：标题不同、meta 只有单个 gid。
    for (id, gid, file_name) in [
        ("legacy-1", "gid-1", "Show.S01E01.mkv"),
        ("legacy-2", "gid-2", "Show.S01E02.mkv"),
    ] {
        context
            .notification_store
            .add(Notification {
                id: id.to_string(),
                level: "success".to_string(),
                event: "download_completed".to_string(),
                title: format!("下载完成: {}", file_name),
                message: format!("文件：{}\n目录：/downloads/anime", file_name),
                meta: HashMap::from([("gid".to_string(), json!(gid))]),
                read: false,
                created_at: 1,
            })
            .await
            .unwrap();
    }

    context
        .download_monitor
        .notify_completed_downloads(&[
            task_with("gid-1", "Show.S01E01.mkv", "complete"),
            task_with("gid-2", "Show.S01E02.mkv", "complete"),
        ])
        .await;

    let notifications = context.notification_store.list(true).await;
    // 除遗留的逐文件通知外，不得再新增任何合并通知。
    let merged = notifications
        .iter()
        .filter(|notification| {
            notification.event == "download_completed" && notification.title.contains("个文件")
        })
        .collect::<Vec<_>>();
    assert_eq!(merged.len(), 0, "已逐文件通知过的批次不得合并补发");

    context.job_queue.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}
