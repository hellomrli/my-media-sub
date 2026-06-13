pub mod subscription;
pub mod settings;
pub mod search;
pub mod transfer;
pub mod notification;
pub mod rules;

// 重新导出常用类型
pub use subscription::{
    Subscription,
    SubscriptionStatus,
    MediaType,
    CheckHistoryItem,
    ProbeResult as SubscriptionProbeResult,
    ProbeFile as SubscriptionProbeFile,
};

pub use rules::TransferRules;

pub use settings::{Settings, CustomCategory};

pub use search::{
    SearchResult,
    SearchSession,
    ProbeResult as SearchProbeResult,
    ProbeFile as SearchProbeFile,
};

pub use transfer::{TransferPlan, TransferItem};

pub use notification::Notification;
