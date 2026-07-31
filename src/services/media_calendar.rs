use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveTime, TimeZone, Utc};

use crate::jobs::Job;
use crate::models::{
    AutomationEvent, CalendarConfidence, CalendarQuickActions, CalendarScheduleSource,
    CalendarSourceAlert, CalendarStatus, MediaCalendar, MediaCalendarItem, MediaCalendarSummary,
    Notification, Settings, Subscription,
};
use crate::services::subscription_status::{build_subscription_detail, EpisodeStatusItem};

pub const CALENDAR_TIMEZONE: &str = "Asia/Shanghai";
pub const MAX_CALENDAR_RANGE_DAYS: i64 = 366;
pub const SOURCE_STALE_AFTER_DAYS: i64 = 7;

#[derive(Debug, Clone)]
pub struct MediaCalendarQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub today: NaiveDate,
    pub status: Option<CalendarStatus>,
    pub media_type: Option<String>,
    pub subscription_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ScheduleCandidate {
    episode: Option<i32>,
    episode_title: String,
    date: NaiveDate,
    time: Option<NaiveTime>,
    source: CalendarScheduleSource,
    confidence: CalendarConfidence,
}

#[derive(Debug, Clone, Copy, Default)]
struct CalendarSubscriptionProgress {
    latest_discovered_episode: Option<i32>,
    latest_transferred_episode: Option<i32>,
    source_overdue_days: Option<i64>,
}

pub fn shanghai_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 60 * 60).expect("UTC+8 is a valid fixed offset")
}

pub fn natural_week(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = date - Duration::days(i64::from(date.weekday().num_days_from_sunday()));
    (start, start + Duration::days(6))
}

pub fn build_media_calendar(
    subscriptions: Vec<Subscription>,
    settings: &Settings,
    jobs: &[Job],
    notifications: &[Notification],
    events: &[AutomationEvent],
    query: &MediaCalendarQuery,
) -> MediaCalendar {
    let (week_start, week_end) = natural_week(query.today);
    let mut items = Vec::new();
    let mut source_alerts = Vec::new();

    for subscription in subscriptions {
        if !subscription_matches(&subscription, query) {
            continue;
        }

        let detail =
            build_subscription_detail(subscription.clone(), settings, jobs, notifications, events);
        let mut progress = CalendarSubscriptionProgress {
            latest_discovered_episode: detail.summary.latest_discovered_episode,
            latest_transferred_episode: detail.summary.latest_transferred_episode,
            source_overdue_days: None,
        };
        if let Some(alert) = source_change_alert(&subscription, progress, query.today) {
            progress.source_overdue_days = Some(alert.overdue_days);
            source_alerts.push(alert);
        }
        let episode_states = detail
            .episodes
            .iter()
            .map(|item| (item.episode, item))
            .collect::<BTreeMap<_, _>>();
        let candidates = metadata_schedule_candidates(&subscription, query.from, query.to);

        match candidates {
            Some(candidates) => {
                for candidate in candidates {
                    let state = candidate
                        .episode
                        .and_then(|episode| episode_states.get(&episode).copied());
                    let item = build_item(
                        &subscription,
                        candidate,
                        state,
                        query.today,
                        week_end,
                        progress,
                    );
                    if query
                        .status
                        .is_none_or(|status| item.statuses.contains(&status))
                    {
                        items.push(item);
                    }
                }
            }
            None => {
                let item = unknown_schedule_item(&subscription, progress);
                if query
                    .status
                    .is_none_or(|status| item.statuses.contains(&status))
                {
                    items.push(item);
                }
            }
        }
    }

    items.sort_by(|left, right| {
        left.scheduled_date
            .is_none()
            .cmp(&right.scheduled_date.is_none())
            .then_with(|| left.scheduled_date.cmp(&right.scheduled_date))
            .then_with(|| left.scheduled_time.cmp(&right.scheduled_time))
            .then_with(|| left.subscription_title.cmp(&right.subscription_title))
            .then_with(|| left.episode.cmp(&right.episode))
    });

    let mut subscription_ids = BTreeSet::new();
    let mut by_status = BTreeMap::<String, usize>::new();
    let mut by_media_type = BTreeMap::<String, usize>::new();
    for item in &items {
        subscription_ids.insert(item.subscription_id.clone());
        *by_media_type.entry(item.media_type.clone()).or_default() += 1;
        for status in &item.statuses {
            *by_status.entry(status.as_str().to_string()).or_default() += 1;
        }
    }

    MediaCalendar {
        timezone: CALENDAR_TIMEZONE.to_string(),
        from: query.from.to_string(),
        to: query.to.to_string(),
        today: query.today.to_string(),
        week_start: week_start.to_string(),
        week_end: week_end.to_string(),
        summary: MediaCalendarSummary {
            total: items.len(),
            subscriptions: subscription_ids.len(),
            source_alerts: source_alerts.len(),
            by_status,
            by_media_type,
        },
        source_alerts,
        items,
    }
}

fn subscription_matches(subscription: &Subscription, query: &MediaCalendarQuery) -> bool {
    if let Some(subscription_id) = query.subscription_id.as_deref() {
        if subscription.id != subscription_id {
            return false;
        }
    }
    if let Some(media_type) = query.media_type.as_deref() {
        if !subscription.media_type.eq_ignore_ascii_case(media_type) {
            return false;
        }
    }
    true
}

fn metadata_schedule_candidates(
    subscription: &Subscription,
    from: NaiveDate,
    to: NaiveDate,
) -> Option<Vec<ScheduleCandidate>> {
    let metadata = subscription.metadata.as_ref()?;
    let mut candidates = Vec::<ScheduleCandidate>::new();
    let mut dated_episodes = Vec::<(i32, NaiveDate)>::new();

    for episode in metadata.episodes.iter().filter(|episode| {
        episode.episode_number > 0
            && (episode.season_number <= 0 || episode.season_number == subscription.season)
    }) {
        let Some(date) = parse_date(episode.air_date.as_deref()) else {
            continue;
        };
        dated_episodes.push((episode.episode_number, date));
        if (from..=to).contains(&date) {
            candidates.push(ScheduleCandidate {
                episode: Some(episode.episode_number),
                episode_title: episode.name.clone(),
                date,
                time: None,
                source: CalendarScheduleSource::MetadataEpisode,
                confidence: CalendarConfidence::High,
            });
        }
    }

    if let Some(episode) = metadata.next_episode_to_air.as_ref().filter(|episode| {
        episode.episode_number > 0
            && (episode.season_number <= 0 || episode.season_number == subscription.season)
    }) {
        if let Some(date) = parse_date(episode.air_date.as_deref()) {
            dated_episodes.push((episode.episode_number, date));
            let duplicate = candidates
                .iter()
                .any(|item| item.episode == Some(episode.episode_number) && item.date == date);
            if !duplicate && (from..=to).contains(&date) {
                candidates.push(ScheduleCandidate {
                    episode: Some(episode.episode_number),
                    episode_title: episode.name.clone(),
                    date,
                    time: None,
                    source: CalendarScheduleSource::MetadataNextEpisode,
                    confidence: CalendarConfidence::High,
                });
            }
        }
    }

    let has_exact_schedule = !dated_episodes.is_empty();
    append_inferred_candidates(subscription, &dated_episodes, from, to, &mut candidates);

    if !has_exact_schedule {
        if let Some(date) = parse_date(metadata.release_date.as_deref()) {
            if (from..=to).contains(&date) {
                candidates.push(ScheduleCandidate {
                    episode: (subscription.media_type != "movie").then_some(1),
                    episode_title: String::new(),
                    date,
                    time: None,
                    source: CalendarScheduleSource::MetadataReleaseDate,
                    confidence: CalendarConfidence::Medium,
                });
            }
            return Some(candidates);
        }
        return None;
    }

    candidates.sort_by_key(|item| (item.date, item.episode));
    candidates.dedup_by_key(|item| (item.date, item.episode));
    Some(candidates)
}

fn append_inferred_candidates(
    subscription: &Subscription,
    dated_episodes: &[(i32, NaiveDate)],
    from: NaiveDate,
    to: NaiveDate,
    candidates: &mut Vec<ScheduleCandidate>,
) {
    let mut points = dated_episodes.to_vec();
    points.sort_by_key(|point| point.0);
    points.dedup_by_key(|point| point.0);
    let Some((previous_episode, previous_date)) = points.iter().rev().nth(1).copied() else {
        return;
    };
    let Some((last_episode, last_date)) = points.last().copied() else {
        return;
    };
    // TMDB and similar providers often publish placeholder episode objects
    // (title only, no air date) for the remaining season. Do not turn the
    // cadence of the last two aired episodes into a fake "still updating"
    // schedule while such placeholders exist; wait for a real air date.
    if subscription.metadata.as_ref().is_some_and(|metadata| {
        metadata.episodes.iter().any(|episode| {
            episode.episode_number > last_episode
                && (episode.season_number <= 0 || episode.season_number == subscription.season)
                && parse_date(episode.air_date.as_deref()).is_none()
        })
    }) {
        return;
    }
    let episode_delta = last_episode - previous_episode;
    let day_delta = (last_date - previous_date).num_days();
    if episode_delta <= 0
        || day_delta <= 0
        || day_delta > 56
        || day_delta % i64::from(episode_delta) != 0
    {
        return;
    }
    let cadence_days = day_delta / i64::from(episode_delta);
    if !(1..=28).contains(&cadence_days) {
        return;
    }
    let total = subscription
        .total_episode_number
        .or_else(|| {
            subscription
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.number_of_episodes)
        })
        .unwrap_or(last_episode);

    for episode in (last_episode + 1)..=total {
        let date = last_date + Duration::days(cadence_days * i64::from(episode - last_episode));
        if date > to {
            break;
        }
        if date >= from && !candidates.iter().any(|item| item.episode == Some(episode)) {
            candidates.push(ScheduleCandidate {
                episode: Some(episode),
                episode_title: String::new(),
                date,
                time: None,
                source: CalendarScheduleSource::InferredCadence,
                confidence: CalendarConfidence::Low,
            });
        }
    }
}

fn build_item(
    subscription: &Subscription,
    candidate: ScheduleCandidate,
    state: Option<&EpisodeStatusItem>,
    today: NaiveDate,
    week_end: NaiveDate,
    progress: CalendarSubscriptionProgress,
) -> MediaCalendarItem {
    let discovered = state.is_some_and(|item| item.discovered);
    let transferred = state.is_some_and(|item| item.transferred);
    let downloaded = state.is_some_and(|item| item.download_status == "completed");
    let strm_ready = state.is_some_and(|item| item.strm_status == "generated");
    let missing = candidate.episode.is_some() && !discovered;
    let mut statuses = Vec::new();

    if candidate.date == today {
        statuses.push(CalendarStatus::Today);
    } else if candidate.date > today && candidate.date <= week_end {
        statuses.push(CalendarStatus::ThisWeek);
    }

    if subscription.completed && candidate.date <= today && missing {
        statuses.push(CalendarStatus::CompletedMissing);
    } else if transferred && subscription.sync_download_enabled && !downloaded {
        statuses.push(CalendarStatus::TransferredPendingDownload);
    } else if discovered && !transferred && !subscription.notify_only {
        statuses.push(CalendarStatus::DiscoveredPendingTransfer);
    } else if candidate.date < today && missing {
        statuses.push(CalendarStatus::AiredUndiscovered);
    } else if discovered
        && (subscription.notify_only || transferred)
        && (!subscription.sync_download_enabled || downloaded)
    {
        statuses.push(CalendarStatus::Ready);
    } else {
        statuses.push(CalendarStatus::Scheduled);
    }

    let primary_status = primary_status(&statuses);
    let scheduled_time = candidate.time.map(|time| time.format("%H:%M").to_string());
    let scheduled_at = candidate.time.and_then(|time| {
        shanghai_offset()
            .from_local_datetime(&candidate.date.and_time(time))
            .single()
            .map(|value: DateTime<FixedOffset>| value.to_rfc3339())
    });
    let episode_suffix = candidate
        .episode
        .map(|episode| episode.to_string())
        .unwrap_or_else(|| "release".to_string());

    MediaCalendarItem {
        id: format!(
            "{}:{}:{}:{}",
            subscription.id, subscription.season, episode_suffix, candidate.date
        ),
        subscription_id: subscription.id.clone(),
        subscription_title: subscription.title.clone(),
        media_type: normalized_media_type(subscription),
        season: subscription.season.max(1),
        episode: candidate.episode,
        episode_title: candidate.episode_title,
        thumbnail_url: calendar_thumbnail_url(subscription, candidate.episode),
        scheduled_date: Some(candidate.date.to_string()),
        scheduled_time,
        scheduled_at,
        schedule_source: candidate.source,
        confidence: candidate.confidence,
        primary_status,
        statuses,
        discovered,
        transferred,
        downloaded,
        strm_ready,
        missing,
        subscription_completed: subscription.completed,
        latest_discovered_episode: progress.latest_discovered_episode,
        latest_transferred_episode: progress.latest_transferred_episode,
        source_change_recommended: progress.source_overdue_days.is_some(),
        source_overdue_days: progress.source_overdue_days,
        actions: quick_actions(subscription, missing),
    }
}

fn calendar_thumbnail_url(subscription: &Subscription, episode: Option<i32>) -> Option<String> {
    let metadata = subscription.metadata.as_ref()?;
    let non_empty = |value: &Option<String>| {
        value
            .as_ref()
            .map(|url| url.trim())
            .filter(|url| !url.is_empty())
            .map(str::to_owned)
    };

    episode
        .and_then(|number| {
            metadata.episodes.iter().find(|item| {
                item.episode_number == number
                    && (item.season_number == subscription.season.max(1) || item.season_number == 0)
            })
        })
        .and_then(|item| non_empty(&item.still_url))
        .or_else(|| {
            metadata
                .seasons
                .iter()
                .find(|item| item.season_number == subscription.season.max(1))
                .and_then(|item| non_empty(&item.poster_url))
        })
        .or_else(|| non_empty(&metadata.poster_url))
}

fn unknown_schedule_item(
    subscription: &Subscription,
    progress: CalendarSubscriptionProgress,
) -> MediaCalendarItem {
    MediaCalendarItem {
        id: format!("{}:unknown", subscription.id),
        subscription_id: subscription.id.clone(),
        subscription_title: subscription.title.clone(),
        media_type: normalized_media_type(subscription),
        season: subscription.season.max(1),
        episode: None,
        episode_title: String::new(),
        thumbnail_url: calendar_thumbnail_url(subscription, None),
        scheduled_date: None,
        scheduled_time: None,
        scheduled_at: None,
        schedule_source: CalendarScheduleSource::Unknown,
        confidence: CalendarConfidence::Unknown,
        primary_status: CalendarStatus::UnknownSchedule,
        statuses: vec![CalendarStatus::UnknownSchedule],
        discovered: false,
        transferred: false,
        downloaded: false,
        strm_ready: false,
        missing: false,
        subscription_completed: subscription.completed,
        latest_discovered_episode: progress.latest_discovered_episode,
        latest_transferred_episode: progress.latest_transferred_episode,
        source_change_recommended: progress.source_overdue_days.is_some(),
        source_overdue_days: progress.source_overdue_days,
        actions: quick_actions(subscription, false),
    }
}

fn quick_actions(subscription: &Subscription, missing: bool) -> CalendarQuickActions {
    CalendarQuickActions {
        detail_url: format!("?tab=subscriptions&subscription={}", subscription.id),
        can_check: subscription.enabled && subscription.status != "invalid",
        can_repair: missing && subscription.enabled && subscription.status != "invalid",
        can_switch_source: subscription.enabled
            && !subscription.completed
            && subscription.media_type != "movie",
    }
}

fn source_change_alert(
    subscription: &Subscription,
    progress: CalendarSubscriptionProgress,
    today: NaiveDate,
) -> Option<CalendarSourceAlert> {
    if !subscription.enabled || subscription.completed || subscription.media_type == "movie" {
        return None;
    }
    let (latest_aired_episode, latest_aired_date) =
        latest_aired_metadata_episode(subscription, today)?;
    let overdue_days = (today - latest_aired_date).num_days();
    if overdue_days <= SOURCE_STALE_AFTER_DAYS
        || progress
            .latest_discovered_episode
            .is_some_and(|episode| episode >= latest_aired_episode)
    {
        return None;
    }

    // 临时网络故障也会更新 last_checked_at，不能当成“来源里没有该集”的证据。
    // 只接受当前来源的最近一次成功探测，并且它必须发生在宽限期结束后。
    let last_successful_check_date = latest_successful_source_check_date(subscription)?;
    if last_successful_check_date < latest_aired_date + Duration::days(SOURCE_STALE_AFTER_DAYS) {
        return None;
    }

    Some(CalendarSourceAlert {
        subscription_id: subscription.id.clone(),
        subscription_title: subscription.title.clone(),
        media_type: normalized_media_type(subscription),
        season: subscription.season.max(1),
        latest_aired_episode,
        latest_aired_date: latest_aired_date.to_string(),
        latest_discovered_episode: progress.latest_discovered_episode,
        latest_transferred_episode: progress.latest_transferred_episode,
        overdue_days,
        actions: quick_actions(subscription, false),
    })
}

fn latest_successful_source_check_date(subscription: &Subscription) -> Option<NaiveDate> {
    if !subscription
        .last_probe
        .as_ref()
        .is_some_and(|probe| probe.ok)
    {
        return None;
    }

    let source_started_at = subscription.last_source_switch_at.unwrap_or(i64::MIN);
    let checked_at = subscription
        .check_history
        .iter()
        .map(|item| item.time)
        .filter(|timestamp| {
            *timestamp > 0
                && *timestamp >= source_started_at
                && *timestamp <= subscription.last_checked_at
        })
        .max()?;
    Some(
        DateTime::<Utc>::from_timestamp(checked_at, 0)?
            .with_timezone(&shanghai_offset())
            .date_naive(),
    )
}

fn latest_aired_metadata_episode(
    subscription: &Subscription,
    today: NaiveDate,
) -> Option<(i32, NaiveDate)> {
    let metadata = subscription.metadata.as_ref()?;
    let mut latest = None;
    let mut consider = |episode: &crate::models::MediaMetadataEpisode| {
        if episode.episode_number <= 0
            || (episode.season_number > 0 && episode.season_number != subscription.season)
        {
            return;
        }
        let Some(date) = parse_date(episode.air_date.as_deref()).filter(|date| *date <= today)
        else {
            return;
        };
        let candidate = (episode.episode_number, date);
        if latest.as_ref().is_none_or(|(number, current_date)| {
            (date, episode.episode_number) > (*current_date, *number)
        }) {
            latest = Some(candidate);
        }
    };
    for episode in &metadata.episodes {
        consider(episode);
    }
    if let Some(episode) = &metadata.next_episode_to_air {
        consider(episode);
    }
    latest
}

fn primary_status(statuses: &[CalendarStatus]) -> CalendarStatus {
    [
        CalendarStatus::CompletedMissing,
        CalendarStatus::TransferredPendingDownload,
        CalendarStatus::DiscoveredPendingTransfer,
        CalendarStatus::AiredUndiscovered,
        CalendarStatus::Today,
        CalendarStatus::ThisWeek,
        CalendarStatus::Ready,
        CalendarStatus::Scheduled,
        CalendarStatus::UnknownSchedule,
    ]
    .into_iter()
    .find(|candidate| statuses.contains(candidate))
    .unwrap_or(CalendarStatus::Scheduled)
}

fn normalized_media_type(subscription: &Subscription) -> String {
    let media_type = subscription.media_type.trim();
    if media_type.is_empty() {
        "series".to_string()
    } else {
        media_type.to_ascii_lowercase()
    }
}

fn parse_date(value: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value?.trim(), "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde_json::json;

    use super::*;
    use crate::models::subscription::{CheckHistoryItem, ProbeResult};
    use crate::models::{MediaMetadata, MediaMetadataEpisode, MetadataProvider, TransferRules};

    fn subscription() -> Subscription {
        serde_json::from_value(json!({
            "id": "sub-1",
            "title": "Example",
            "media_type": "series",
            "season": 1,
            "current_episode_number": 2,
            "total_episode_number": 6,
            "url": "https://pan.quark.cn/s/example",
            "known_episodes": [1, 2],
            "transferred_file_keys": ["ep:1"],
            "sync_download_enabled": true,
            "enabled": true,
            "completed": false,
            "rules": TransferRules::default(),
            "created_at": 1,
            "updated_at": 2,
            "last_checked_at": 3,
            "status": "active"
        }))
        .unwrap()
    }

    fn query(from: &str, to: &str, today: &str) -> MediaCalendarQuery {
        MediaCalendarQuery {
            from: NaiveDate::parse_from_str(from, "%Y-%m-%d").unwrap(),
            to: NaiveDate::parse_from_str(to, "%Y-%m-%d").unwrap(),
            today: NaiveDate::parse_from_str(today, "%Y-%m-%d").unwrap(),
            status: None,
            media_type: None,
            subscription_id: None,
        }
    }

    fn shanghai_timestamp(date: &str) -> i64 {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
        shanghai_offset()
            .from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap()
            .timestamp()
    }

    fn record_successful_source_check(subscription: &mut Subscription, date: &str) {
        let checked_at = shanghai_timestamp(date);
        subscription.last_checked_at = checked_at;
        subscription.last_probe = Some(ProbeResult {
            ok: true,
            state: "ok".to_string(),
            message: String::new(),
            files: vec![],
        });
        subscription.check_history.insert(
            0,
            CheckHistoryItem {
                time: checked_at,
                state: "ok".to_string(),
                matched_count: 0,
                transfer_count: 0,
                scanned_count: 0,
                new_count: 0,
                known_count: subscription.known_episodes.len() as i32,
                skipped_directory_count: 0,
                skipped_other_season_count: 0,
                skipped_before_start_count: 0,
                skipped_duplicate_episode_count: 0,
                new_files: vec![],
                new_episodes: vec![],
                summary: "无更新".to_string(),
            },
        );
    }

    fn stale_source_subscription() -> Subscription {
        let mut sub = subscription();
        sub.current_episode_number = 1;
        sub.total_episode_number = Some(2);
        sub.known_episodes = vec![1];
        sub.transferred_file_keys = vec!["ep:1".to_string()];
        sub.metadata = Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "1".to_string(),
            title: "Example".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: None,
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(2),
            number_of_seasons: Some(1),
            seasons: vec![],
            next_episode_to_air: None,
            episodes: vec![
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 1,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-01".to_string()),
                    still_url: None,
                },
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 2,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-10".to_string()),
                    still_url: None,
                },
            ],
        });
        sub
    }

    #[test]
    fn natural_week_uses_sunday_and_crosses_year() {
        let (start, end) = natural_week(NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        assert_eq!(start.to_string(), "2026-12-27");
        assert_eq!(end.to_string(), "2027-01-02");
    }

    #[test]
    fn falls_back_from_episode_still_to_poster_without_mutating_metadata() {
        let mut sub = subscription();
        sub.metadata = Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "1".to_string(),
            title: "Example".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: Some("https://example.test/poster.jpg".to_string()),
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(2),
            number_of_seasons: Some(1),
            seasons: vec![],
            next_episode_to_air: None,
            episodes: vec![
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 1,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-06".to_string()),
                    still_url: Some("https://example.test/episode-1.jpg".to_string()),
                },
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 2,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-09".to_string()),
                    still_url: None,
                },
            ],
        });
        let original_metadata = serde_json::to_value(sub.metadata.as_ref().unwrap()).unwrap();

        let calendar = build_media_calendar(
            vec![sub.clone()],
            &Settings::default(),
            &[],
            &[],
            &[],
            &query("2026-07-06", "2026-07-19", "2026-07-06"),
        );

        // 有剧照的集用剧照，没有的回退到剧集海报。
        assert_eq!(
            calendar.items[0].thumbnail_url.as_deref(),
            Some("https://example.test/episode-1.jpg")
        );
        assert_eq!(
            calendar.items[1].thumbnail_url.as_deref(),
            Some("https://example.test/poster.jpg")
        );
        // 构建日历是只读的，不得改写原始元数据。
        assert_eq!(
            serde_json::to_value(sub.metadata.as_ref().unwrap()).unwrap(),
            original_metadata
        );
    }

    #[test]
    fn preserves_multiple_metadata_episodes_on_the_same_day() {
        let mut sub = subscription();
        sub.metadata = Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "1".to_string(),
            title: "Example".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: None,
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(2),
            number_of_seasons: Some(1),
            seasons: vec![],
            next_episode_to_air: None,
            episodes: vec![
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 1,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-10".to_string()),
                    still_url: None,
                },
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 2,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-10".to_string()),
                    still_url: None,
                },
            ],
        });

        let calendar = build_media_calendar(
            vec![sub],
            &Settings::default(),
            &[],
            &[],
            &[],
            &query("2026-07-10", "2026-07-10", "2026-07-10"),
        );
        assert_eq!(calendar.items.len(), 2);
        assert_eq!(calendar.items[0].episode, Some(1));
        assert_eq!(calendar.items[1].episode, Some(2));
    }

    #[test]
    fn merges_episode_pipeline_states_and_calendar_buckets() {
        let mut sub = subscription();
        sub.metadata = Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "1".to_string(),
            title: "Example".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: None,
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(4),
            number_of_seasons: Some(1),
            seasons: vec![],
            next_episode_to_air: None,
            episodes: vec![
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 1,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-08".to_string()),
                    still_url: None,
                },
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 2,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-10".to_string()),
                    still_url: None,
                },
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 3,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-11".to_string()),
                    still_url: None,
                },
            ],
        });
        let calendar = build_media_calendar(
            vec![sub],
            &Settings::default(),
            &[],
            &[],
            &[],
            &query("2026-07-06", "2026-07-11", "2026-07-10"),
        );

        assert!(calendar.items[0]
            .statuses
            .contains(&CalendarStatus::TransferredPendingDownload));
        assert!(calendar.items[1].statuses.contains(&CalendarStatus::Today));
        assert!(calendar.items[1]
            .statuses
            .contains(&CalendarStatus::DiscoveredPendingTransfer));
        assert!(calendar.items[2]
            .statuses
            .contains(&CalendarStatus::ThisWeek));
    }

    #[test]
    fn infers_future_cadence_with_low_confidence() {
        let mut sub = subscription();
        sub.total_episode_number = Some(4);
        sub.metadata = Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "1".to_string(),
            title: "Example".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: None,
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(4),
            number_of_seasons: Some(1),
            seasons: vec![],
            next_episode_to_air: None,
            episodes: vec![
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 1,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-01".to_string()),
                    still_url: None,
                },
                MediaMetadataEpisode {
                    season_number: 1,
                    episode_number: 2,
                    name: String::new(),
                    overview: String::new(),
                    air_date: Some("2026-07-08".to_string()),
                    still_url: None,
                },
            ],
        });
        let calendar = build_media_calendar(
            vec![sub],
            &Settings::default(),
            &[],
            &[],
            &[],
            &query("2026-07-01", "2026-07-31", "2026-07-10"),
        );
        let inferred = calendar
            .items
            .iter()
            .filter(|item| item.schedule_source == CalendarScheduleSource::InferredCadence)
            .collect::<Vec<_>>();
        assert_eq!(inferred.len(), 2);
        assert!(inferred
            .iter()
            .all(|item| item.confidence == CalendarConfidence::Low));
    }

    #[test]
    fn emits_unknown_item_only_when_schedule_is_unavailable() {
        let calendar = build_media_calendar(
            vec![subscription()],
            &Settings::default(),
            &[],
            &[],
            &[],
            &query("2026-07-01", "2026-07-31", "2026-07-10"),
        );
        assert_eq!(calendar.items.len(), 1);
        assert_eq!(
            calendar.items[0].primary_status,
            CalendarStatus::UnknownSchedule
        );
    }

    #[test]
    fn reports_transfer_progress_and_a_checked_source_that_is_over_a_week_behind() {
        let mut sub = stale_source_subscription();
        record_successful_source_check(&mut sub, "2026-07-19");

        // The missed episode is outside the visible week, but the dashboard still
        // receives one subscription-level reminder.
        let current_week = build_media_calendar(
            vec![sub.clone()],
            &Settings::default(),
            &[],
            &[],
            &[],
            &query("2026-07-19", "2026-07-25", "2026-07-20"),
        );
        assert!(current_week.items.is_empty());
        assert_eq!(current_week.summary.source_alerts, 1);
        assert_eq!(current_week.source_alerts.len(), 1);
        let alert = &current_week.source_alerts[0];
        assert_eq!(alert.latest_aired_episode, 2);
        assert_eq!(alert.latest_discovered_episode, Some(1));
        assert_eq!(alert.latest_transferred_episode, Some(1));
        assert_eq!(alert.overdue_days, 10);
        assert!(alert.actions.can_switch_source);

        let full_range = build_media_calendar(
            vec![sub],
            &Settings::default(),
            &[],
            &[],
            &[],
            &query("2026-07-01", "2026-07-20", "2026-07-20"),
        );
        let episode = full_range
            .items
            .iter()
            .find(|item| item.episode == Some(2))
            .unwrap();
        assert_eq!(episode.latest_discovered_episode, Some(1));
        assert_eq!(episode.latest_transferred_episode, Some(1));
        assert!(episode.source_change_recommended);
        assert_eq!(episode.source_overdue_days, Some(10));
    }

    #[test]
    fn source_change_alert_requires_conclusive_current_source_evidence() {
        let build = |sub| {
            build_media_calendar(
                vec![sub],
                &Settings::default(),
                &[],
                &[],
                &[],
                &query("2026-07-01", "2026-07-31", "2026-07-20"),
            )
        };

        let never_successfully_checked = stale_source_subscription();
        assert!(build(never_successfully_checked).source_alerts.is_empty());

        let mut checked_before_grace_ended = stale_source_subscription();
        record_successful_source_check(&mut checked_before_grace_ended, "2026-07-16");
        assert!(build(checked_before_grace_ended.clone())
            .source_alerts
            .is_empty());

        // A later transient failure updates last_checked_at, but must not turn
        // the earlier successful probe into post-grace evidence.
        checked_before_grace_ended.last_checked_at = shanghai_timestamp("2026-07-19");
        assert!(build(checked_before_grace_ended).source_alerts.is_empty());

        let mut caught_up = stale_source_subscription();
        caught_up.current_episode_number = 2;
        caught_up.known_episodes.push(2);
        record_successful_source_check(&mut caught_up, "2026-07-19");
        assert!(build(caught_up).source_alerts.is_empty());

        let mut movie = stale_source_subscription();
        movie.media_type = "movie".to_string();
        record_successful_source_check(&mut movie, "2026-07-19");
        assert!(build(movie).source_alerts.is_empty());
    }

    #[test]
    fn inferred_episode_dates_never_trigger_source_change_alerts() {
        let mut sub = stale_source_subscription();
        sub.current_episode_number = 2;
        sub.known_episodes = vec![1, 2];
        sub.total_episode_number = Some(3);
        record_successful_source_check(&mut sub, "2026-07-29");

        let calendar = build_media_calendar(
            vec![sub],
            &Settings::default(),
            &[],
            &[],
            &[],
            &query("2026-07-01", "2026-07-31", "2026-07-30"),
        );
        assert!(calendar.items.iter().any(|item| {
            item.episode == Some(3)
                && item.schedule_source == CalendarScheduleSource::InferredCadence
        }));
        assert!(calendar.source_alerts.is_empty());
    }
}
