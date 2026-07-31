use crate::models::metadata::episode_count_for_season;
use crate::models::Subscription;
use crate::services::episode::{detect_episode, is_video_name};

pub fn completion_target_episode(sub: &Subscription) -> Option<i32> {
    sub.rules
        .finish_after_episode
        .or(sub.total_episode_number)
        .filter(|episode| *episode > 0)
}

/// 把元数据里的季集数回填到 `total_episode_number`。
///
/// 订阅创建时往往还没刮削到元数据，之后补上的集数如果不落到订阅上，
/// 完结判定就永远没有目标，剧集追完了也留在「追更中」。
/// 多季订阅没有单一总集数，电影不按集数完结，两者都跳过。
pub fn backfill_total_episode_from_metadata(sub: &mut Subscription) -> bool {
    if sub.media_type == "movie" || sub.is_multi_season() || sub.total_episode_number.is_some() {
        return false;
    }
    let Some(count) =
        episode_count_for_season(sub.metadata.as_ref(), sub.season).filter(|count| *count > 0)
    else {
        return false;
    };
    sub.total_episode_number = Some(count);
    true
}

/// 电影只有一个正片，拿到片源即可完结，不需要集数目标。
fn movie_source_ready<'a>(
    sub: &Subscription,
    file_names: impl IntoIterator<Item = &'a String>,
) -> bool {
    sub.media_type == "movie" && file_names.into_iter().any(|name| is_video_name(name))
}

pub fn progress_max_episode(sub: &Subscription) -> i32 {
    let transferred_episodes = episode_numbers_from_file_names(sub.transferred_files.iter());
    sub.known_episodes
        .iter()
        .copied()
        .chain(transferred_episodes)
        .chain(std::iter::once(sub.current_episode_number))
        .max()
        .unwrap_or(0)
}

fn has_reached_target_episode(sub: &Subscription, additional_episodes: &[i32]) -> bool {
    let Some(target) = completion_target_episode(sub) else {
        return false;
    };

    sub.current_episode_number == target
        || sub.known_episodes.contains(&target)
        || additional_episodes.contains(&target)
        || episode_numbers_from_file_names(sub.transferred_files.iter()).contains(&target)
}

pub fn should_reopen_completed_subscription(sub: &Subscription) -> bool {
    if !sub.completed && sub.status != "completed" {
        return false;
    }

    // 电影不按集数完结，重开只会让已经拿到片源的订阅反复回到追更中。
    if sub.media_type == "movie" {
        return false;
    }

    completion_target_episode(sub).is_some() && !has_reached_target_episode(sub, &[])
}

pub fn reopen_completed_subscription_status(sub: &mut Subscription) -> bool {
    if !should_reopen_completed_subscription(sub) {
        return false;
    }

    sub.completed = false;
    sub.status = "active".to_string();
    sub.invalid_since = None;
    sub.last_error = String::new();
    true
}

/// Reconcile persisted completion flags after totals, rules, or metadata change.
/// Automatic-transfer subscriptions complete from transferred evidence and notify-only
/// subscriptions from discovered evidence.
///
/// 同步下载订阅同样按转存证据完结。此前它们被排除在外，完全依赖下载监视器把目标集的
/// `sync_downloads` 记录标成已完成——而 Aria2 的任务历史是易失的（重启、结果条数上限、
/// 手动清理都会丢），`completed_at` 常年填不上，订阅就永远停在追更中；没有任何下载记录
/// 的订阅更是结构性地无法完结。追更是否结束取决于片源是否出齐并已转存，本地镜像下载
/// 属于投递环节，不该成为完结的唯一凭据。下载监视器的判定保留为额外触发路径。
pub fn reconcile_completed_subscription_status(sub: &mut Subscription) -> bool {
    backfill_total_episode_from_metadata(sub);
    if reopen_completed_subscription_status(sub) {
        return true;
    }
    if sub.completed || sub.status == "completed" {
        // 历史数据里 completed 与 status 可能不一致，会让列表分组把已完结订阅算进追更中。
        if sub.completed && sub.status == "active" {
            sub.status = "completed".to_string();
        }
        return false;
    }

    let reached = if sub.notify_only {
        should_mark_completed_from_known_episodes(sub, &[])
    } else {
        should_mark_completed_from_transferred_files(sub, &[])
    };
    if !reached {
        return false;
    }

    sub.completed = true;
    sub.status = "completed".to_string();
    sub.invalid_since = None;
    sub.last_error = String::new();
    true
}

pub fn episode_numbers_from_file_names<'a>(
    file_names: impl IntoIterator<Item = &'a String>,
) -> Vec<i32> {
    let mut episodes = file_names
        .into_iter()
        .filter_map(|name| detect_episode(name).episode)
        .collect::<Vec<_>>();
    episodes.sort();
    episodes.dedup();
    episodes
}

pub fn should_mark_completed_from_known_episodes(sub: &Subscription, new_episodes: &[i32]) -> bool {
    if sub.completed {
        return false;
    }

    if sub.media_type == "movie" {
        return movie_source_ready(
            sub,
            sub.known_files.iter().chain(sub.transferred_files.iter()),
        );
    }

    has_reached_target_episode(sub, new_episodes)
}

pub fn should_mark_completed_from_transferred_files(
    sub: &Subscription,
    additional_file_names: &[String],
) -> bool {
    let mut file_names = sub.transferred_files.clone();
    file_names.extend(additional_file_names.iter().cloned());
    should_mark_completed_from_file_names(sub, &file_names)
}

pub fn should_mark_completed_from_file_names(sub: &Subscription, file_names: &[String]) -> bool {
    if sub.completed {
        return false;
    }

    if sub.media_type == "movie" {
        return movie_source_ready(sub, file_names.iter());
    }

    let Some(target_episode) = completion_target_episode(sub) else {
        return false;
    };

    episode_numbers_from_file_names(file_names.iter()).contains(&target_episode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::rules::TransferRules;

    fn subscription() -> Subscription {
        Subscription {
            id: "sub1".to_string(),
            title: "Show".to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season: 1,
            season_end: None,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: Some(12),
            source_group: String::new(),
            tags: vec![],
            metadata: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![1, 2, 11],
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

    #[test]
    fn test_should_mark_completed_from_known_episodes_uses_total_episode_number() {
        let sub = subscription();
        assert!(should_mark_completed_from_known_episodes(&sub, &[12]));
        assert!(!should_mark_completed_from_known_episodes(&sub, &[10]));
    }

    #[test]
    fn test_out_of_range_episode_does_not_complete_subscription() {
        let mut sub = subscription();
        sub.total_episode_number = Some(10);
        sub.known_episodes = vec![1, 2, 3, 704];
        sub.current_episode_number = 704;

        assert!(!should_mark_completed_from_known_episodes(&sub, &[]));
        assert!(!should_mark_completed_from_file_names(
            &sub,
            &["0704INS直播（有弹幕）.mp4".to_string()]
        ));

        sub.completed = true;
        sub.status = "completed".to_string();
        assert!(should_reopen_completed_subscription(&sub));
    }

    #[test]
    fn test_should_mark_completed_from_transferred_files() {
        let mut sub = subscription();
        sub.transferred_files = vec!["Show.S01E11.mkv".to_string()];

        assert!(should_mark_completed_from_transferred_files(
            &sub,
            &["Show.S01E12.mkv".to_string()]
        ));
        assert!(!should_mark_completed_from_transferred_files(
            &sub,
            &["Show.S01E10.mkv".to_string()]
        ));
    }

    #[test]
    fn test_should_reopen_completed_subscription_when_target_not_reached() {
        let mut sub = subscription();
        sub.completed = true;
        sub.status = "completed".to_string();
        sub.current_episode_number = 178;
        sub.total_episode_number = Some(190);
        sub.known_episodes = vec![177, 178];

        assert!(should_reopen_completed_subscription(&sub));

        sub.current_episode_number = 190;
        assert!(!should_reopen_completed_subscription(&sub));
    }

    #[test]
    fn reconcile_marks_transferred_target_completed_even_when_current_progress_lags() {
        let mut sub = subscription();
        sub.current_episode_number = 11;
        sub.transferred_files = vec!["Show.S01E12.mkv".to_string()];

        assert!(reconcile_completed_subscription_status(&mut sub));
        assert!(sub.completed);
        assert_eq!(sub.status, "completed");
    }

    #[test]
    fn reconcile_uses_known_target_for_notify_only() {
        let mut notify_only = subscription();
        notify_only.notify_only = true;
        notify_only.known_episodes.push(12);
        assert!(reconcile_completed_subscription_status(&mut notify_only));
        assert!(notify_only.completed);
    }

    /// 同步下载订阅曾被排除在完结判定外，完全依赖下载监视器给 `sync_downloads`
    /// 标 `completed_at`。Aria2 的任务历史是易失的，这个标记常年填不上，
    /// 于是片源已出齐、全部转存完毕的订阅永远停在追更中；连一条下载记录都没有的
    /// 订阅更是无法完结。转存证据现在同样适用于它们。
    #[test]
    fn synced_download_subscription_completes_from_transferred_evidence() {
        let mut synced = subscription();
        synced.sync_download_enabled = true;
        synced.transferred_files = vec!["Show.S01E12.mkv".to_string()];

        assert!(reconcile_completed_subscription_status(&mut synced));
        assert!(synced.completed);
        assert_eq!(synced.status, "completed");
    }

    #[test]
    fn synced_download_subscription_without_download_records_still_completes() {
        // 线上真实形态：转存已完成、sync_downloads 为空，此前永远无法完结。
        let mut synced = subscription();
        synced.sync_download_enabled = true;
        synced.sync_downloads.clear();
        synced.total_episode_number = Some(14);
        synced.known_episodes = vec![13, 14];
        synced.current_episode_number = 14;
        synced.transferred_files = vec![
            "My.Royal.Nemesis.S01E13.2026.1080p.NF.WEB-DL.x264.AAC.mkv".to_string(),
            "My.Royal.Nemesis.S01E14.2026.1080p.NF.WEB-DL.x264.AAC.mkv".to_string(),
        ];

        assert!(reconcile_completed_subscription_status(&mut synced));
        assert!(synced.completed);
    }

    #[test]
    fn synced_download_subscription_stays_active_before_target_episode() {
        let mut synced = subscription();
        synced.sync_download_enabled = true;
        synced.transferred_files = vec!["Show.S01E11.mkv".to_string()];

        assert!(!reconcile_completed_subscription_status(&mut synced));
        assert!(!synced.completed);
    }

    #[test]
    fn movie_completes_once_the_feature_file_is_transferred() {
        let mut sub = subscription();
        sub.media_type = "movie".to_string();
        sub.total_episode_number = None;
        sub.known_episodes.clear();

        assert!(!reconcile_completed_subscription_status(&mut sub));

        sub.transferred_files = vec!["阿凡达.2009.2160p.mkv".to_string()];
        assert!(reconcile_completed_subscription_status(&mut sub));
        assert!(sub.completed);
        assert_eq!(sub.status, "completed");
        // 电影没有集数目标，不能因为「没到目标集」被反复重开。
        assert!(!should_reopen_completed_subscription(&sub));
    }

    #[test]
    fn notify_only_movie_completes_from_discovered_file() {
        let mut sub = subscription();
        sub.media_type = "movie".to_string();
        sub.notify_only = true;
        sub.total_episode_number = None;
        sub.known_episodes.clear();
        sub.known_files = vec!["电影说明.txt".to_string()];

        assert!(!reconcile_completed_subscription_status(&mut sub));

        sub.known_files.push("电影.1080p.mp4".to_string());
        assert!(reconcile_completed_subscription_status(&mut sub));
        assert!(sub.completed);
    }

    #[test]
    fn metadata_episode_count_backfills_missing_completion_target() {
        use crate::models::metadata::{MediaMetadata, MediaMetadataSeason, MetadataProvider};

        let mut sub = subscription();
        sub.total_episode_number = None;
        sub.known_episodes = vec![1, 2, 12];
        sub.transferred_files = vec!["Show.S01E12.mkv".to_string()];
        assert_eq!(completion_target_episode(&sub), None);

        sub.metadata = Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "1".to_string(),
            title: "Show".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: None,
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(12),
            number_of_seasons: Some(1),
            seasons: vec![MediaMetadataSeason {
                season_number: 1,
                episode_count: Some(12),
                name: "第 1 季".to_string(),
                air_date: None,
                poster_url: None,
            }],
            next_episode_to_air: None,
            episodes: vec![],
        });

        assert!(reconcile_completed_subscription_status(&mut sub));
        assert_eq!(sub.total_episode_number, Some(12));
        assert!(sub.completed);
        assert_eq!(sub.status, "completed");
    }

    #[test]
    fn multi_season_subscription_keeps_no_backfilled_total() {
        use crate::models::metadata::{MediaMetadata, MediaMetadataSeason, MetadataProvider};

        let mut sub = subscription();
        sub.total_episode_number = None;
        sub.season_end = Some(3);
        sub.metadata = Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "1".to_string(),
            title: "Show".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: None,
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(36),
            number_of_seasons: Some(3),
            seasons: vec![MediaMetadataSeason {
                season_number: 1,
                episode_count: Some(12),
                name: "第 1 季".to_string(),
                air_date: None,
                poster_url: None,
            }],
            next_episode_to_air: None,
            episodes: vec![],
        });

        assert!(!backfill_total_episode_from_metadata(&mut sub));
        assert_eq!(sub.total_episode_number, None);
    }

    #[test]
    fn reconcile_repairs_completed_flag_without_matching_status() {
        let mut sub = subscription();
        sub.completed = true;
        sub.status = "active".to_string();
        sub.known_episodes = vec![12];
        sub.current_episode_number = 12;

        reconcile_completed_subscription_status(&mut sub);

        assert_eq!(sub.status, "completed");
    }

    #[test]
    fn test_reopen_completed_subscription_status_clears_completion_flags() {
        let mut sub = subscription();
        sub.completed = true;
        sub.status = "completed".to_string();
        sub.current_episode_number = 178;
        sub.total_episode_number = Some(190);
        sub.invalid_since = Some(1);
        sub.last_error = "completed".to_string();

        assert!(reopen_completed_subscription_status(&mut sub));
        assert!(!sub.completed);
        assert_eq!(sub.status, "active");
        assert_eq!(sub.invalid_since, None);
        assert!(sub.last_error.is_empty());
    }
}
