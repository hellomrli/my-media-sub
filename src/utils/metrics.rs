use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct Metrics {
    subscription_checks: AtomicU64,
    subscription_check_failures: AtomicU64,
    transfer_tasks: AtomicU64,
    push_sent: AtomicU64,
    push_failed: AtomicU64,
    subscription_check_duration_ms_total: AtomicU64,
    subscription_check_duration_count: AtomicU64,
    transfer_duration_ms_total: AtomicU64,
    transfer_duration_count: AtomicU64,
    failed_stages: AtomicU64,
    source_switches: AtomicU64,
    job_queue_depth: AtomicU64,
    backup_successes: AtomicU64,
    backup_failures: AtomicU64,
    restore_successes: AtomicU64,
    store_io: Mutex<BTreeMap<String, StoreIoSnapshot>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StoreIoSnapshot {
    pub current_bytes: u64,
    pub read_count: u64,
    pub write_count: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub parse_duration_us_total: u64,
    pub write_duration_us_total: u64,
    pub failures: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub subscription_checks: u64,
    pub subscription_check_failures: u64,
    pub transfer_tasks: u64,
    pub push_sent: u64,
    pub push_failed: u64,
    pub subscription_check_duration_ms_total: u64,
    pub subscription_check_duration_count: u64,
    pub transfer_duration_ms_total: u64,
    pub transfer_duration_count: u64,
    pub failed_stages: u64,
    pub source_switches: u64,
    pub job_queue_depth: u64,
    pub backup_successes: u64,
    pub backup_failures: u64,
    pub restore_successes: u64,
    pub store_io: BTreeMap<String, StoreIoSnapshot>,
}

static GLOBAL_METRICS: LazyLock<Arc<Metrics>> = LazyLock::new(|| Arc::new(Metrics::default()));

pub fn global_metrics() -> Arc<Metrics> {
    GLOBAL_METRICS.clone()
}

#[derive(Clone, Copy)]
pub enum MetricTimerKind {
    SubscriptionCheck,
    Transfer,
}

pub struct MetricTimer {
    metrics: Arc<Metrics>,
    kind: MetricTimerKind,
    started: std::time::Instant,
}

impl Drop for MetricTimer {
    fn drop(&mut self) {
        match self.kind {
            MetricTimerKind::SubscriptionCheck => self
                .metrics
                .observe_subscription_check(self.started.elapsed()),
            MetricTimerKind::Transfer => self.metrics.observe_transfer(self.started.elapsed()),
        }
    }
}

impl Metrics {
    pub fn start_timer(self: &Arc<Self>, kind: MetricTimerKind) -> MetricTimer {
        MetricTimer {
            metrics: self.clone(),
            kind,
            started: std::time::Instant::now(),
        }
    }

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

    pub fn observe_subscription_check(&self, duration: std::time::Duration) {
        self.subscription_check_duration_ms_total
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        self.subscription_check_duration_count
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn observe_transfer(&self, duration: std::time::Duration) {
        self.transfer_duration_ms_total
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        self.transfer_duration_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_failed_stage(&self) {
        self.failed_stages.fetch_add(1, Ordering::Relaxed);
    }
    pub fn increment_source_switch(&self) {
        self.source_switches.fetch_add(1, Ordering::Relaxed);
    }
    pub fn set_job_queue_depth(&self, value: u64) {
        self.job_queue_depth.store(value, Ordering::Relaxed);
    }
    pub fn increment_backup_success(&self) {
        self.backup_successes.fetch_add(1, Ordering::Relaxed);
    }
    pub fn increment_backup_failure(&self) {
        self.backup_failures.fetch_add(1, Ordering::Relaxed);
    }
    pub fn increment_restore_success(&self) {
        self.restore_successes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn observe_store_read(
        &self,
        store: &str,
        bytes: u64,
        duration: std::time::Duration,
        success: bool,
    ) {
        let mut metrics = self
            .store_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let item = metrics.entry(store.to_string()).or_default();
        item.current_bytes = bytes;
        item.read_count = item.read_count.saturating_add(1);
        item.read_bytes = item.read_bytes.saturating_add(bytes);
        item.parse_duration_us_total = item
            .parse_duration_us_total
            .saturating_add(duration.as_micros() as u64);
        if !success {
            item.failures = item.failures.saturating_add(1);
        }
    }

    pub fn observe_store_write(
        &self,
        store: &str,
        bytes: u64,
        duration: std::time::Duration,
        success: bool,
    ) {
        let mut metrics = self
            .store_io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let item = metrics.entry(store.to_string()).or_default();
        if success {
            item.current_bytes = bytes;
        }
        item.write_count = item.write_count.saturating_add(1);
        item.write_bytes = item.write_bytes.saturating_add(bytes);
        item.write_duration_us_total = item
            .write_duration_us_total
            .saturating_add(duration.as_micros() as u64);
        if !success {
            item.failures = item.failures.saturating_add(1);
        }
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            subscription_checks: self.subscription_checks.load(Ordering::Relaxed),
            subscription_check_failures: self.subscription_check_failures.load(Ordering::Relaxed),
            transfer_tasks: self.transfer_tasks.load(Ordering::Relaxed),
            push_sent: self.push_sent.load(Ordering::Relaxed),
            push_failed: self.push_failed.load(Ordering::Relaxed),
            subscription_check_duration_ms_total: self
                .subscription_check_duration_ms_total
                .load(Ordering::Relaxed),
            subscription_check_duration_count: self
                .subscription_check_duration_count
                .load(Ordering::Relaxed),
            transfer_duration_ms_total: self.transfer_duration_ms_total.load(Ordering::Relaxed),
            transfer_duration_count: self.transfer_duration_count.load(Ordering::Relaxed),
            failed_stages: self.failed_stages.load(Ordering::Relaxed),
            source_switches: self.source_switches.load(Ordering::Relaxed),
            job_queue_depth: self.job_queue_depth.load(Ordering::Relaxed),
            backup_successes: self.backup_successes.load(Ordering::Relaxed),
            backup_failures: self.backup_failures.load(Ordering::Relaxed),
            restore_successes: self.restore_successes.load(Ordering::Relaxed),
            store_io: self
                .store_io
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_io_metrics_track_size_duration_and_failures() {
        let metrics = Metrics::default();
        metrics.observe_store_read("jobs", 1024, std::time::Duration::from_micros(25), true);
        metrics.observe_store_write("jobs", 800, std::time::Duration::from_micros(40), true);
        metrics.observe_store_read("jobs", 800, std::time::Duration::from_micros(10), false);
        let snapshot = metrics.snapshot();
        let jobs = snapshot.store_io.get("jobs").unwrap();
        assert_eq!(jobs.current_bytes, 800);
        assert_eq!(jobs.read_count, 2);
        assert_eq!(jobs.write_count, 1);
        assert_eq!(jobs.parse_duration_us_total, 35);
        assert_eq!(jobs.write_duration_us_total, 40);
        assert_eq!(jobs.failures, 1);
    }
}
