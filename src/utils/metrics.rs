use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::LazyLock;

#[derive(Debug, Default)]
pub struct Metrics {
    subscription_checks: AtomicU64,
    subscription_check_failures: AtomicU64,
    transfer_tasks: AtomicU64,
    push_sent: AtomicU64,
    push_failed: AtomicU64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub subscription_checks: u64,
    pub subscription_check_failures: u64,
    pub transfer_tasks: u64,
    pub push_sent: u64,
    pub push_failed: u64,
}

static GLOBAL_METRICS: LazyLock<Arc<Metrics>> = LazyLock::new(|| Arc::new(Metrics::default()));

pub fn global_metrics() -> Arc<Metrics> {
    GLOBAL_METRICS.clone()
}

impl Metrics {
    pub fn increment_subscription_checks(&self) {
        self.subscription_checks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_subscription_check_failures(&self) {
        self.subscription_check_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_transfer_tasks(&self) {
        self.transfer_tasks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_push_results(&self, sent: u64, failed: u64) {
        self.push_sent.fetch_add(sent, Ordering::Relaxed);
        self.push_failed.fetch_add(failed, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            subscription_checks: self.subscription_checks.load(Ordering::Relaxed),
            subscription_check_failures: self.subscription_check_failures.load(Ordering::Relaxed),
            transfer_tasks: self.transfer_tasks.load(Ordering::Relaxed),
            push_sent: self.push_sent.load(Ordering::Relaxed),
            push_failed: self.push_failed.load(Ordering::Relaxed),
        }
    }
}
