use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CalendarStatus {
    Today,
    ThisWeek,
    AiredUndiscovered,
    DiscoveredPendingTransfer,
    TransferredPendingDownload,
    CompletedMissing,
    Ready,
    Scheduled,
    UnknownSchedule,
}

impl CalendarStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::ThisWeek => "this_week",
            Self::AiredUndiscovered => "aired_undiscovered",
            Self::DiscoveredPendingTransfer => "discovered_pending_transfer",
            Self::TransferredPendingDownload => "transferred_pending_download",
            Self::CompletedMissing => "completed_missing",
            Self::Ready => "ready",
            Self::Scheduled => "scheduled",
            Self::UnknownSchedule => "unknown_schedule",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalendarScheduleSource {
    MetadataEpisode,
    MetadataNextEpisode,
    MetadataReleaseDate,
    InferredCadence,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalendarConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarQuickActions {
    pub detail_url: String,
    pub can_check: bool,
    pub can_repair: bool,
    pub can_switch_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarSourceAlert {
    pub subscription_id: String,
    pub subscription_title: String,
    pub media_type: String,
    pub season: i32,
    pub latest_aired_episode: i32,
    pub latest_aired_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_discovered_episode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_transferred_episode: Option<i32>,
    pub overdue_days: i64,
    pub actions: CalendarQuickActions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCalendarItem {
    pub id: String,
    pub subscription_id: String,
    pub subscription_title: String,
    pub media_type: String,
    pub season: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode: Option<i32>,
    pub episode_title: String,
    /// Episode still when available, otherwise season/subscription poster.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<String>,
    pub schedule_source: CalendarScheduleSource,
    pub confidence: CalendarConfidence,
    pub primary_status: CalendarStatus,
    pub statuses: Vec<CalendarStatus>,
    pub discovered: bool,
    pub transferred: bool,
    pub downloaded: bool,
    pub strm_ready: bool,
    pub missing: bool,
    pub subscription_completed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_discovered_episode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_transferred_episode: Option<i32>,
    pub source_change_recommended: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_overdue_days: Option<i64>,
    pub actions: CalendarQuickActions,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaCalendarSummary {
    pub total: usize,
    pub subscriptions: usize,
    pub source_alerts: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_media_type: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCalendar {
    pub timezone: String,
    pub from: String,
    pub to: String,
    pub today: String,
    pub week_start: String,
    pub week_end: String,
    pub summary: MediaCalendarSummary,
    pub source_alerts: Vec<CalendarSourceAlert>,
    pub items: Vec<MediaCalendarItem>,
}
