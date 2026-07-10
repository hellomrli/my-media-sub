use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveTime, TimeZone, Weekday};

use crate::jobs::Job;
use crate::models::{
    AutomationEvent, CalendarConfidence, CalendarQuickActions, CalendarScheduleSource,
    CalendarStatus, MediaCalendar, MediaCalendarItem, MediaCalendarSummary, MediaScheduleOverride,
    Notification, Settings, Subscription,
};
use crate::services::subscription_status::{build_subscription_detail, EpisodeStatusItem};

pub const CALENDAR_TIMEZONE: &str = "Asia/Shanghai";
pub const MAX_CALENDAR_RANGE_DAYS: i64 = 366;

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

pub fn shanghai_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 60 * 60).expect("UTC+8 is a valid fixed offset")
}

pub fn natural_week(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = date - Duration::days(i64::from(date.weekday().num_days_from_monday()));
    (start, start + Duration::days(6))
}

pub fn validate_manual_schedule(schedule: &MediaScheduleOverride) -> Result<(), String> {
    let start_date = NaiveDate::parse_from_str(schedule.start_date.trim(), "%Y-%m-%d")
        .map_err(|_| "手动排期开播日期必须使用 YYYY-MM-DD 格式".to_string())?;
    if schedule.interval_weeks == 0 || schedule.interval_weeks > 52 {
        return Err("手动排期周期周数必须在 1 到 52 之间".to_string());
    }
    if schedule.first_episode_number <= 0 {
        return Err("手动排期首集编号必须大于 0".to_string());
    }
    if let Some(total) = schedule.total_episodes {
        if total < schedule.first_episode_number {
            return Err("手动排期总集数不能小于首集编号".to_string());
        }
    }
    if schedule.weekdays.iter().any(|day| !(1..=7).contains(day)) {
        return Err("手动排期星期必须使用 1（周一）到 7（周日）".to_string());
    }
    let unique = schedule.weekdays.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != schedule.weekdays.len() {
        return Err("手动排期星期不能重复".to_string());
    }
    if !schedule.air_time.trim().is_empty() {
        NaiveTime::parse_from_str(schedule.air_time.trim(), "%H:%M")
            .map_err(|_| "手动排期播出时间必须使用 HH:MM 格式".to_string())?;
    }

    // 空 weekdays 是合法简写，等价于 start_date 的星期。
    let _ = start_date;
    Ok(())
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

    for subscription in subscriptions {
        if !subscription_matches(&subscription, query) {
            continue;
        }

        let detail =
            build_subscription_detail(subscription.clone(), settings, jobs, notifications, events);
        let episode_states = detail
            .episodes
            .iter()
            .map(|item| (item.episode, item))
            .collect::<BTreeMap<_, _>>();
        let candidates = schedule_candidates(&subscription, query.from, query.to);

        match candidates {
            Some(candidates) => {
                for candidate in candidates {
                    let state = candidate
                        .episode
                        .and_then(|episode| episode_states.get(&episode).copied());
                    let item = build_item(&subscription, candidate, state, query.today, week_end);
                    if query
                        .status
                        .is_none_or(|status| item.statuses.contains(&status))
                    {
                        items.push(item);
                    }
                }
            }
            None => {
                let item = unknown_schedule_item(&subscription);
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
            by_status,
            by_media_type,
        },
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

fn schedule_candidates(
    subscription: &Subscription,
    from: NaiveDate,
    to: NaiveDate,
) -> Option<Vec<ScheduleCandidate>> {
    if let Some(manual) = subscription.manual_schedule.as_ref() {
        return manual_schedule_candidates(subscription, manual, from, to);
    }

    metadata_schedule_candidates(subscription, from, to)
}

fn manual_schedule_candidates(
    subscription: &Subscription,
    schedule: &MediaScheduleOverride,
    from: NaiveDate,
    to: NaiveDate,
) -> Option<Vec<ScheduleCandidate>> {
    if validate_manual_schedule(schedule).is_err() {
        return None;
    }
    let start = NaiveDate::parse_from_str(schedule.start_date.trim(), "%Y-%m-%d").ok()?;
    let time = if schedule.air_time.trim().is_empty() {
        None
    } else {
        NaiveTime::parse_from_str(schedule.air_time.trim(), "%H:%M").ok()
    };
    let weekdays = if schedule.weekdays.is_empty() {
        vec![weekday_number(start.weekday())]
    } else {
        schedule.weekdays.clone()
    };
    let anchor_week_start = natural_week(start).0;
    let total = schedule
        .total_episodes
        .or(subscription.total_episode_number)
        .or_else(|| {
            subscription
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.number_of_episodes)
        });
    let mut episode = schedule.first_episode_number;
    let mut date = start;
    let mut candidates = Vec::new();

    while date <= to {
        let weeks_since_anchor = (date - anchor_week_start).num_days() / 7;
        let active_week =
            weeks_since_anchor >= 0 && weeks_since_anchor % i64::from(schedule.interval_weeks) == 0;
        if active_week && weekdays.contains(&weekday_number(date.weekday())) {
            if total.is_some_and(|total| episode > total) {
                break;
            }
            if date >= from {
                candidates.push(ScheduleCandidate {
                    episode: Some(episode),
                    episode_title: String::new(),
                    date,
                    time,
                    source: CalendarScheduleSource::Manual,
                    confidence: CalendarConfidence::High,
                });
            }
            episode += 1;
        }
        date += Duration::days(1);
    }

    Some(candidates)
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
        actions: quick_actions(subscription, missing),
    }
}

fn unknown_schedule_item(subscription: &Subscription) -> MediaCalendarItem {
    MediaCalendarItem {
        id: format!("{}:unknown", subscription.id),
        subscription_id: subscription.id.clone(),
        subscription_title: subscription.title.clone(),
        media_type: normalized_media_type(subscription),
        season: subscription.season.max(1),
        episode: None,
        episode_title: String::new(),
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
        actions: quick_actions(subscription, false),
    }
}

fn quick_actions(subscription: &Subscription, missing: bool) -> CalendarQuickActions {
    CalendarQuickActions {
        detail_url: format!("?tab=subscriptions&subscription={}", subscription.id),
        can_check: subscription.enabled && subscription.status != "invalid",
        can_repair: missing && subscription.enabled && subscription.status != "invalid",
    }
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

fn weekday_number(weekday: Weekday) -> u8 {
    weekday.number_from_monday() as u8
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde_json::json;

    use super::*;
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

    #[test]
    fn natural_week_uses_monday_and_crosses_year() {
        let (start, end) = natural_week(NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        assert_eq!(start.to_string(), "2026-12-28");
        assert_eq!(end.to_string(), "2027-01-03");
    }

    #[test]
    fn validates_manual_schedule_fields() {
        let valid = MediaScheduleOverride {
            start_date: "2026-07-06".to_string(),
            weekdays: vec![1, 4],
            air_time: "20:30".to_string(),
            interval_weeks: 1,
            first_episode_number: 1,
            total_episodes: Some(12),
        };
        assert!(validate_manual_schedule(&valid).is_ok());

        let mut invalid = valid;
        invalid.weekdays = vec![1, 1];
        assert!(validate_manual_schedule(&invalid).is_err());
    }

    #[test]
    fn manual_schedule_overrides_metadata_and_supports_multiple_weekdays() {
        let mut sub = subscription();
        sub.manual_schedule = Some(MediaScheduleOverride {
            start_date: "2026-07-06".to_string(),
            weekdays: vec![1, 4],
            air_time: "20:30".to_string(),
            interval_weeks: 1,
            first_episode_number: 1,
            total_episodes: Some(4),
        });
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
            episodes: vec![MediaMetadataEpisode {
                season_number: 1,
                episode_number: 1,
                name: "wrong source".to_string(),
                overview: String::new(),
                air_date: Some("2026-07-07".to_string()),
                still_url: None,
            }],
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
        let dates = calendar
            .items
            .iter()
            .map(|item| item.scheduled_date.as_deref().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            dates,
            vec!["2026-07-06", "2026-07-09", "2026-07-13", "2026-07-16"]
        );
        assert!(calendar
            .items
            .iter()
            .all(|item| item.schedule_source == CalendarScheduleSource::Manual));
        assert_eq!(
            calendar.items[0].scheduled_at.as_deref(),
            Some("2026-07-06T20:30:00+08:00")
        );
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
                    air_date: Some("2026-07-12".to_string()),
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
            &query("2026-07-06", "2026-07-12", "2026-07-10"),
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
}
