pub mod metadata;
pub mod notification;
pub mod rules;
pub mod search;
pub mod settings;
pub mod subscription;
pub mod transfer;

// 重新导出常用类型
pub use subscription::Subscription;

pub use rules::TransferRules;

pub use settings::{CustomCategory, RulePreset, Settings};

pub use search::SearchResult;

pub use notification::Notification;

pub use metadata::{
    episode_count_for_season, MediaMetadata, MediaMetadataSeason, MetadataProvider,
};
