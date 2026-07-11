use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 单订阅的手动播出排期覆盖。
///
/// 日期和时间均按 Asia/Shanghai 解释。`weekdays` 使用 ISO 编号：
/// 1 = 周一，7 = 周日。空数组时使用 `start_date` 自身的星期。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaScheduleOverride {
    /// 首个排期日期，格式 YYYY-MM-DD。
    #[serde(default)]
    pub start_date: String,

    /// 播出星期（ISO 1..=7），同一周期可包含多个播出日。
    #[serde(default)]
    pub weekdays: Vec<u8>,

    /// 播出时间，格式 HH:MM；为空表示只有日期、没有可靠的具体时间。
    #[serde(default)]
    pub air_time: String,

    /// 周期周数，1 表示每周，2 表示隔周。
    #[serde(default = "default_interval_weeks")]
    pub interval_weeks: u32,

    /// `start_date` 对应的首集编号。
    #[serde(default = "default_first_episode_number")]
    pub first_episode_number: i32,

    /// 手动覆盖的总集数；为空时回退到订阅或元数据总集数。
    #[serde(default)]
    pub total_episodes: Option<i32>,
}

fn default_interval_weeks() -> u32 {
    1
}

fn default_first_episode_number() -> i32 {
    1
}

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
    Manual,
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
    pub actions: CalendarQuickActions,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaCalendarSummary {
    pub total: usize,
    pub subscriptions: usize,
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
    pub items: Vec<MediaCalendarItem>,
}
