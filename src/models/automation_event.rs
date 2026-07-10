#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AutomationStage {
    SourceCheck,
    FileFilter,
    VersionSelect,
    CloudTransfer,
    Rename,
    Strm,
    Aria2,
    Notification,
}

impl AutomationStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceCheck => "source_check",
            Self::FileFilter => "file_filter",
            Self::VersionSelect => "version_select",
            Self::CloudTransfer => "cloud_transfer",
            Self::Rename => "rename",
            Self::Strm => "strm",
            Self::Aria2 => "aria2",
            Self::Notification => "notification",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AutomationStatus {
    Pending,
    Running,
    Succeeded,
    Skipped,
    Failed,
    Retrying,
    Canceled,
}

impl AutomationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
            Self::Retrying => "retrying",
            Self::Canceled => "canceled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Skipped | Self::Failed | Self::Canceled
        )
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        match self {
            Self::Pending => matches!(
                next,
                Self::Running | Self::Skipped | Self::Failed | Self::Canceled
            ),
            Self::Running => matches!(
                next,
                Self::Succeeded | Self::Skipped | Self::Failed | Self::Retrying | Self::Canceled
            ),
            Self::Failed => matches!(next, Self::Retrying | Self::Canceled),
            Self::Retrying => matches!(
                next,
                Self::Pending | Self::Running | Self::Failed | Self::Canceled
            ),
            Self::Succeeded | Self::Skipped | Self::Canceled => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationEvent {
    pub id: String,
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub stage: AutomationStage,
    pub status: AutomationStatus,
    #[serde(default = "default_attempt")]
    pub attempt: u32,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
}

impl AutomationEvent {
    pub fn new(
        id: impl Into<String>,
        correlation_id: impl Into<String>,
        stage: AutomationStage,
        status: AutomationStatus,
        now: i64,
    ) -> Self {
        Self {
            id: id.into(),
            correlation_id: correlation_id.into(),
            subscription_id: None,
            episode: None,
            job_id: None,
            stage,
            status,
            attempt: 1,
            message: String::new(),
            error: String::new(),
            metadata: BTreeMap::new(),
            created_at: now,
            updated_at: now,
            started_at: (status == AutomationStatus::Running).then_some(now),
            finished_at: status.is_terminal().then_some(now),
        }
    }

    pub fn duration_seconds(&self) -> Option<i64> {
        let started = self.started_at?;
        Some(
            self.finished_at
                .unwrap_or(self.updated_at)
                .saturating_sub(started),
        )
    }
}

fn default_attempt() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_status_machine_rejects_terminal_rewrites() {
        assert!(AutomationStatus::Pending.can_transition_to(AutomationStatus::Running));
        assert!(AutomationStatus::Running.can_transition_to(AutomationStatus::Failed));
        assert!(AutomationStatus::Failed.can_transition_to(AutomationStatus::Retrying));
        assert!(!AutomationStatus::Succeeded.can_transition_to(AutomationStatus::Running));
        assert!(!AutomationStatus::Canceled.can_transition_to(AutomationStatus::Pending));
    }

    #[test]
    fn automation_event_contract_uses_all_stable_stage_and_status_names() {
        let stages = [
            AutomationStage::SourceCheck,
            AutomationStage::FileFilter,
            AutomationStage::VersionSelect,
            AutomationStage::CloudTransfer,
            AutomationStage::Rename,
            AutomationStage::Strm,
            AutomationStage::Aria2,
            AutomationStage::Notification,
        ];
        assert_eq!(
            stages.map(AutomationStage::as_str),
            [
                "source_check",
                "file_filter",
                "version_select",
                "cloud_transfer",
                "rename",
                "strm",
                "aria2",
                "notification",
            ]
        );

        let statuses = [
            AutomationStatus::Pending,
            AutomationStatus::Running,
            AutomationStatus::Succeeded,
            AutomationStatus::Skipped,
            AutomationStatus::Failed,
            AutomationStatus::Retrying,
            AutomationStatus::Canceled,
        ];
        assert_eq!(
            statuses.map(AutomationStatus::as_str),
            [
                "pending",
                "running",
                "succeeded",
                "skipped",
                "failed",
                "retrying",
                "canceled",
            ]
        );
    }

    #[test]
    fn retry_path_requires_explicit_retrying_state() {
        assert!(!AutomationStatus::Failed.can_transition_to(AutomationStatus::Running));
        assert!(AutomationStatus::Failed.can_transition_to(AutomationStatus::Retrying));
        assert!(AutomationStatus::Retrying.can_transition_to(AutomationStatus::Running));
        assert!(AutomationStatus::Running.can_transition_to(AutomationStatus::Succeeded));
    }
}
