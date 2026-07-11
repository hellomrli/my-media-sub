pub mod automation_event;
#[allow(dead_code)]
pub mod calendar;
pub mod metadata;
pub mod notification;
pub mod rules;
pub mod search;
pub mod settings;
pub mod source_quality;
pub mod subscription;
pub mod transfer;

// 重新导出常用类型
pub use subscription::Subscription;

pub use automation_event::{AutomationEvent, AutomationStage, AutomationStatus};

pub use calendar::{
    CalendarConfidence, CalendarQuickActions, CalendarScheduleSource, CalendarStatus,
    MediaCalendar, MediaCalendarItem, MediaCalendarSummary, MediaScheduleOverride,
};

pub use rules::TransferRules;

pub use settings::{BrowserPushSubscription, CustomCategory, RulePreset, Settings};

pub use search::SearchResult;

pub use source_quality::SourceQuality;

pub use notification::Notification;

pub use metadata::{
    episode_count_for_season, MediaMetadata, MediaMetadataEpisode, MediaMetadataSeason,
    MetadataProvider,
};
