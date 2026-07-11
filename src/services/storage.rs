use serde::Serialize;

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
    }
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
    }
}
