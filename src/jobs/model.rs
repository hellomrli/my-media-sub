use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ManualTransfer,
    SubscriptionTransfer,
    MetadataScrape,
    PushDispatch,
}

/// 任务调度优先级。
///
/// 旧版持久化任务没有该字段，反序列化时保持为普通优先级，避免升级后改变
/// 已有队列的相对语义。新任务由提交入口按交互性选择默认优先级。
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum JobPriority {
    High,
    #[default]
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobErrorClass {
    RateLimited,
    Transient,
    Authentication,
    Validation,
    NotFound,
    Permanent,
    Internal,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    /// Originating HTTP request, when the job was submitted by an API call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Stable identifier shared by all stages of one automation operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Subscription resource associated with this job, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_id: Option<String>,
    #[serde(default)]
    pub priority: JobPriority,
    #[serde(default = "default_attempt")]
    pub attempt: u32,
    #[serde(default)]
    pub next_attempt_at: Option<i64>,
    #[serde(default)]
    pub error_class: Option<JobErrorClass>,
    pub status: JobStatus,
    pub progress: u8,
    pub title: String,
    pub message: String,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub finished_at: Option<i64>,
}

fn default_attempt() -> u32 {
    1
}

impl JobKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::ManualTransfer => "manual_transfer",
            Self::SubscriptionTransfer => "subscription_transfer",
            Self::MetadataScrape => "metadata_scrape",
            Self::PushDispatch => "push_dispatch",
        }
    }

    pub(crate) fn default_priority(&self) -> JobPriority {
        match self {
            // 用户主动发起的转存应尽快得到反馈。
            Self::ManualTransfer => JobPriority::High,
            // 推送是短任务，单独受并发层限制，不应被长批处理压住。
            Self::PushDispatch => JobPriority::High,
            Self::SubscriptionTransfer => JobPriority::Normal,
            // 元数据批处理通常耗时最长，默认让位于交互和追更任务。
            Self::MetadataScrape => JobPriority::Low,
        }
    }

    pub(crate) fn supports_automatic_retry(&self) -> bool {
        !matches!(self, Self::ManualTransfer)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualTransferPayload {
    pub url: String,
    #[serde(default)]
    pub passcode: String,
    #[serde(default)]
    pub target_fid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionTransferPayload {
    pub subscription_id: String,
    pub file_names: Vec<String>,
    #[serde(default)]
    pub force_transfer: bool,
    #[serde(default)]
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataScrapePayload {
    #[serde(default)]
    pub subscription_id: Option<String>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushDispatchPayload {
    pub event: String,
    pub title: String,
    pub message: String,
    pub level: String,
    #[serde(default)]
    pub notification_id: Option<String>,
    #[serde(default)]
    pub correlation_id: String,
    #[serde(default)]
    pub subscription_id: Option<String>,
    #[serde(default)]
    pub episode: Option<i32>,
}

pub(crate) fn job_idempotency_key(kind: &JobKind, payload: &serde_json::Value) -> String {
    let material = format!("{:?}:{}", kind, payload);
    format!("{:x}", md5::compute(material))
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_job_without_priority_defaults_to_normal() {
        let job: Job = serde_json::from_value(json!({
            "id": "legacy",
            "kind": "manual_transfer",
            "status": "queued",
            "progress": 0,
            "title": "legacy",
            "message": "queued",
            "payload": {"url": "https://pan.quark.cn/s/test"},
            "created_at": 1,
            "updated_at": 1
        }))
        .unwrap();

        assert_eq!(job.priority, JobPriority::Normal);
        assert_eq!(job.attempt, 1);
        assert!(job.error_class.is_none());
        assert!(job.request_id.is_none());
        assert!(job.correlation_id.is_none());
        assert!(job.subscription_id.is_none());
    }

    #[test]
    fn new_job_kind_defaults_favor_interactive_work() {
        assert_eq!(
            JobKind::ManualTransfer.default_priority(),
            JobPriority::High
        );
        assert_eq!(JobKind::PushDispatch.default_priority(), JobPriority::High);
        assert_eq!(
            JobKind::SubscriptionTransfer.default_priority(),
            JobPriority::Normal
        );
        assert_eq!(JobKind::MetadataScrape.default_priority(), JobPriority::Low);
    }
}
