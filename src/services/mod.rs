pub mod episode;
pub mod metadata;
pub mod notification;
pub mod push;
pub mod quark_signin;
pub mod strm;
pub mod subscription_check;
pub mod subscription_progress;
pub mod subscription_scheduler;
pub mod subscription_transfer;
pub mod transfer_rule;

pub use episode::{detect_episode, is_video_name};
pub use metadata::MetadataService;
pub use quark_signin::{QuarkSigninScheduler, QuarkSigninService};
pub use subscription_check::SubscriptionCheckService;
pub use subscription_scheduler::SubscriptionScheduler;
pub use subscription_transfer::SubscriptionTransferService;
