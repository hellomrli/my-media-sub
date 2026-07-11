use serde::Serialize;
use std::collections::BTreeMap;

pub const SQLITE_SUBSCRIPTION_THRESHOLD: usize = 500;
pub const SQLITE_HISTORY_THRESHOLD: usize = 10_000;
pub const SQLITE_FILE_SIZE_THRESHOLD: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct StorageDecisionInput {
    pub subscriptions: usize,
    pub jobs: usize,
    pub notifications: usize,
    pub automation_events: usize,
    pub largest_store_bytes: u64,
    pub complex_query_required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageThresholds {
    pub subscriptions: usize,
    pub history_records: usize,
    pub file_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SqliteMigrationContract {
    pub preserve_json_source: bool,
    pub repeatable_import: bool,
    pub validate_counts_and_checksums: bool,
    pub rollback_before_cutover: bool,
    pub no_long_term_dual_write: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageDecision {
    pub recommendation: &'static str,
    pub sqlite_required: bool,
    pub reasons: Vec<String>,
    pub input: StorageDecisionInput,
    pub thresholds: StorageThresholds,
    pub migration_contract: SqliteMigrationContract,
    pub runtime_backend: &'static str,
    pub migration_phase: &'static str,
    pub threshold_evaluation_started: bool,
    pub dual_write_active: bool,
}

pub fn evaluate_storage(input: StorageDecisionInput) -> StorageDecision {
    let history_records = input
        .jobs
        .saturating_add(input.notifications)
        .saturating_add(input.automation_events);
    let mut reasons = Vec::new();
    if input.subscriptions >= SQLITE_SUBSCRIPTION_THRESHOLD {
        reasons.push(format!(
            "订阅数 {} 达到阈值 {}",
            input.subscriptions, SQLITE_SUBSCRIPTION_THRESHOLD
        ));
    }
    if history_records >= SQLITE_HISTORY_THRESHOLD {
        reasons.push(format!(
            "历史记录 {} 达到阈值 {}",
            history_records, SQLITE_HISTORY_THRESHOLD
        ));
    }
    if input.largest_store_bytes >= SQLITE_FILE_SIZE_THRESHOLD {
        reasons.push(format!(
            "最大 Store {} bytes 达到阈值 {} bytes",
            input.largest_store_bytes, SQLITE_FILE_SIZE_THRESHOLD
        ));
    }
    if input.complex_query_required {
        reasons.push("出现 JSON 内存索引无法满足的复杂查询".to_string());
    }
    let sqlite_required = !reasons.is_empty();
    if !sqlite_required {
        reasons.push("当前规模低于迁移门槛，继续使用紧凑 JSON 与内存索引".to_string());
    }
    StorageDecision {
        recommendation: if sqlite_required {
            "prepare_sqlite_migration"
        } else {
            "keep_json"
        },
        sqlite_required,
        reasons,
        input,
        thresholds: StorageThresholds {
            subscriptions: SQLITE_SUBSCRIPTION_THRESHOLD,
            history_records: SQLITE_HISTORY_THRESHOLD,
            file_bytes: SQLITE_FILE_SIZE_THRESHOLD,
        },
        migration_contract: SqliteMigrationContract {
            preserve_json_source: true,
            repeatable_import: true,
            validate_counts_and_checksums: true,
            rollback_before_cutover: true,
            no_long_term_dual_write: true,
        },
        runtime_backend: "json",
        migration_phase: if sqlite_required {
            "decision_required"
        } else {
            "not_started"
        },
        threshold_evaluation_started: sqlite_required,
        dual_write_active: false,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionPolicy {
    pub subscription_check_history: usize,
    pub subscription_source_switch_history: usize,
    pub subscription_previous_links: usize,
    pub notifications: usize,
    pub active_terminal_jobs: usize,
    pub archived_jobs: usize,
    pub automation_events: usize,
    pub automation_normal_days: u64,
    pub automation_failed_days: u64,
    pub growth_warning_bytes: u64,
}

impl RetentionPolicy {
    pub fn from_env() -> Self {
        Self {
            subscription_check_history: env_usize("RETENTION_SUBSCRIPTION_CHECKS", 30, 1, 30),
            subscription_source_switch_history: env_usize(
                "RETENTION_SOURCE_SWITCHES",
                50,
                1,
                1_000,
            ),
            subscription_previous_links: env_usize("RETENTION_PREVIOUS_LINKS", 50, 1, 50),
            notifications: env_usize("RETENTION_NOTIFICATIONS", 300, 1, 300),
            active_terminal_jobs: env_usize("RETENTION_ACTIVE_JOBS", 300, 0, 500),
            archived_jobs: env_usize("RETENTION_ARCHIVED_JOBS", 5_000, 0, 5_000),
            automation_events: env_usize("RETENTION_AUTOMATION_EVENTS", 5_000, 1, 5_000),
            automation_normal_days: env_u64("RETENTION_AUTOMATION_DAYS", 30, 1, 3_650),
            automation_failed_days: env_u64("RETENTION_FAILED_AUTOMATION_DAYS", 90, 1, 3_650),
            growth_warning_bytes: env_u64("STORE_GROWTH_WARNING_MB", 24, 1, 1_024) * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StoreCleanupPreview {
    pub store: String,
    pub current_records: usize,
    pub retained_records: usize,
    pub retention_limit: usize,
    pub would_remove: usize,
    pub current_bytes: u64,
    pub warning: bool,
    pub warning_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageCleanupPreview {
    pub generated_at: i64,
    pub policy: RetentionPolicy,
    pub stores: Vec<StoreCleanupPreview>,
    pub total_would_remove: usize,
    pub growth_warning: bool,
    pub sqlite_decision: StorageDecision,
    pub execution_requires: &'static str,
    pub mutates_data: bool,
}

pub fn cleanup_store_preview(
    store: &str,
    current_records: usize,
    retained_records: usize,
    retention_limit: usize,
    current_bytes: u64,
    warning_bytes: u64,
) -> StoreCleanupPreview {
    let would_remove = current_records.saturating_sub(retained_records);
    let mut warning_reasons = Vec::new();
    if current_bytes >= warning_bytes {
        warning_reasons.push(format!(
            "Store 大小 {} bytes 达到增长预警线 {} bytes",
            current_bytes, warning_bytes
        ));
    }
    if retained_records > 0
        && current_records.saturating_mul(100) >= retained_records.saturating_mul(80)
    {
        warning_reasons.push(format!(
            "记录数已达到独立保留上限 {} 的 80%",
            retained_records
        ));
    }
    StoreCleanupPreview {
        store: store.to_string(),
        current_records,
        retained_records,
        retention_limit,
        would_remove,
        current_bytes,
        warning: !warning_reasons.is_empty(),
        warning_reasons,
    }
}

pub fn build_cleanup_preview(
    policy: RetentionPolicy,
    stores: Vec<StoreCleanupPreview>,
    sqlite_decision: StorageDecision,
) -> StorageCleanupPreview {
    let total_would_remove = stores.iter().map(|store| store.would_remove).sum();
    let growth_warning = stores.iter().any(|store| store.warning);
    StorageCleanupPreview {
        generated_at: crate::utils::unix_now(),
        policy,
        stores,
        total_would_remove,
        growth_warning,
        sqlite_decision,
        execution_requires: "CLEANUP DATA",
        mutates_data: false,
    }
}

pub fn file_sizes(data_dir: &std::path::Path) -> BTreeMap<String, u64> {
    [
        "subscriptions.json",
        "notifications.json",
        "jobs.json",
        "jobs.archive.json",
        "automation_events.json",
    ]
    .into_iter()
    .map(|name| {
        let bytes = std::fs::metadata(data_dir.join(name))
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        (name.to_string(), bytes)
    })
    .collect()
}

fn env_usize(key: &str, default: usize, min: usize, max: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn env_u64(key: &str, default: u64, min: u64, max: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_json_below_all_thresholds() {
        let decision = evaluate_storage(StorageDecisionInput {
            subscriptions: 120,
            jobs: 500,
            notifications: 300,
            automation_events: 5_000,
            largest_store_bytes: 8 * 1024 * 1024,
            complex_query_required: false,
        });
        assert!(!decision.sqlite_required);
        assert_eq!(decision.recommendation, "keep_json");
        assert_eq!(decision.runtime_backend, "json");
        assert_eq!(decision.migration_phase, "not_started");
        assert!(!decision.threshold_evaluation_started);
        assert!(!decision.dual_write_active);
    }

    #[test]
    fn recommends_migration_only_after_a_gate_is_reached() {
        let decision = evaluate_storage(StorageDecisionInput {
            subscriptions: 500,
            jobs: 10_000,
            notifications: 10_000,
            automation_events: 10_000,
            largest_store_bytes: 40 * 1024 * 1024,
            complex_query_required: false,
        });
        assert!(decision.sqlite_required);
        assert_eq!(decision.recommendation, "prepare_sqlite_migration");
        assert!(decision.migration_contract.preserve_json_source);
        assert!(decision.migration_contract.repeatable_import);
        assert!(decision.migration_contract.validate_counts_and_checksums);
        assert!(decision.migration_contract.rollback_before_cutover);
        assert!(decision.migration_contract.no_long_term_dual_write);
        assert_eq!(decision.runtime_backend, "json");
        assert_eq!(decision.migration_phase, "decision_required");
        assert!(decision.threshold_evaluation_started);
        assert!(!decision.dual_write_active);
    }

    #[test]
    fn cleanup_preview_warns_without_mutating_or_starting_sqlite() {
        let store = cleanup_store_preview(
            "notifications",
            90,
            90,
            100,
            25 * 1024 * 1024,
            24 * 1024 * 1024,
        );
        assert!(store.warning);
        let preview = build_cleanup_preview(
            RetentionPolicy::from_env(),
            vec![store],
            evaluate_storage(StorageDecisionInput {
                subscriptions: 1,
                jobs: 1,
                notifications: 90,
                automation_events: 1,
                largest_store_bytes: 25 * 1024 * 1024,
                complex_query_required: false,
            }),
        );
        assert!(preview.growth_warning);
        assert!(!preview.mutates_data);
        assert!(!preview.sqlite_decision.dual_write_active);
    }

    #[test]
    fn sqlite_gate_does_not_add_a_database_or_dual_write_dependency_early() {
        let manifest = include_str!("../../Cargo.toml");
        assert!(!manifest.contains("rusqlite"));
        assert!(!manifest.contains("sqlx"));
        assert!(!manifest.contains("diesel"));
    }
}
