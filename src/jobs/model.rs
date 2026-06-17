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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub progress: u8,
    pub title: String,
    pub message: String,
    pub payload: serde_json::Value,
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
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
