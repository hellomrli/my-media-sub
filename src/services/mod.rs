pub mod automation_events;
pub mod backup;
pub mod download_monitor;
pub mod episode;
pub mod media_calendar;
pub mod media_library;
pub mod metadata;
pub mod notification;
pub mod post_transfer;
pub mod push;
pub mod quark_signin;
pub mod source_quality;
pub mod storage;
pub mod strm;
pub mod subscription_check;
pub mod subscription_progress;
pub mod subscription_scheduler;
pub mod subscription_source_switch;
pub mod subscription_status;
pub mod subscription_transfer;
pub mod title_normalize;
pub mod transfer_rule;

pub use download_monitor::DownloadMonitorService;
pub use episode::{detect_episode, is_video_name};
pub use metadata::MetadataService;
pub use quark_signin::{QuarkSigninScheduler, QuarkSigninService};
pub use subscription_check::SubscriptionCheckService;
pub use subscription_scheduler::SubscriptionScheduler;
pub use subscription_source_switch::SubscriptionSourceSwitchService;
pub use subscription_transfer::SubscriptionTransferService;

pub mod telegram_bot;

/// STRM is temporarily kept as migration-compatible code, but is not part of
/// the active application. Keeping the switch in Rust makes every execution
/// path fail closed until STRM returns as an independently mounted module.
pub const STRM_MODULE_ENABLED: bool = false;

/// 订阅检查/转存探测分享时的文件数上限。
/// 超限会在 ProbeResult 中标记 `partial` 状态，不再静默截断。
pub const SHARE_PROBE_MAX_FILES: usize = 500;
