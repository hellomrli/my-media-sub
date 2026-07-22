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
    job_store_update_failures: AtomicU64,
    backup_successes: AtomicU64,
    backup_failures: AtomicU64,
    restore_successes: AtomicU64,
    store_io: Mutex<BTreeMap<String, StoreIoSnapshot>>,
    http_requests: Mutex<BTreeMap<String, RequestMetricSnapshot>>,
    external_dependencies: Mutex<BTreeMap<String, DependencyMetricSnapshot>>,
    slow_operations: Mutex<BTreeMap<String, SlowOperationSnapshot>>,
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

#[derive(Debug, Clone, Default, Serialize)]
pub struct RequestMetricSnapshot {
    pub count: u64,
    pub duration_ms_total: u64,
    pub duration_ms_max: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DependencyMetricSnapshot {
    pub count: u64,
    pub failures: u64,
    pub duration_ms_total: u64,
    pub duration_ms_max: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SlowOperationSnapshot {
    pub count: u64,
    pub last_duration_ms: u64,
    pub max_duration_ms: u64,
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
    pub job_store_update_failures: u64,
    pub backup_successes: u64,
    pub backup_failures: u64,
    pub restore_successes: u64,
    pub store_io: BTreeMap<String, StoreIoSnapshot>,
    pub http_requests: BTreeMap<String, RequestMetricSnapshot>,
    pub external_dependencies: BTreeMap<String, DependencyMetricSnapshot>,
    pub slow_operations: BTreeMap<String, SlowOperationSnapshot>,
    pub slow_operation_threshold_ms: u64,
}

static GLOBAL_METRICS: LazyLock<Arc<Metrics>> = LazyLock::new(|| Arc::new(Metrics::default()));
static SLOW_OPERATION_THRESHOLD_MS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SLOW_OPERATION_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_000)
        .clamp(100, 300_000)
});

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
        self.observe_slow_operation("subscription_check", duration);
    }

    pub fn observe_transfer(&self, duration: std::time::Duration) {
        self.transfer_duration_ms_total
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
        self.transfer_duration_count.fetch_add(1, Ordering::Relaxed);
        self.observe_slow_operation("transfer", duration);
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
    pub fn increment_job_store_update_failure(&self) {
        self.job_store_update_failures
            .fetch_add(1, Ordering::Relaxed);
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

    pub fn observe_http_request(&self, method: &str, status: u16, duration: std::time::Duration) {
        let duration_ms = duration.as_millis() as u64;
        let key = format!("{} {}", method, status);
        let mut metrics = self.http_requests.lock().unwrap_or_else(|p| p.into_inner());
        let item = metrics.entry(key).or_default();
        item.count = item.count.saturating_add(1);
        item.duration_ms_total = item.duration_ms_total.saturating_add(duration_ms);
        item.duration_ms_max = item.duration_ms_max.max(duration_ms);
        drop(metrics);
        self.observe_slow_operation("http_request", duration);
    }

    pub fn observe_external_dependency(
        &self,
        service: &str,
        duration: std::time::Duration,
        success: bool,
    ) {
        let duration_ms = duration.as_millis() as u64;
        let mut metrics = self
            .external_dependencies
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let item = metrics.entry(service.to_string()).or_default();
        item.count = item.count.saturating_add(1);
        item.failures = item.failures.saturating_add(u64::from(!success));
        item.duration_ms_total = item.duration_ms_total.saturating_add(duration_ms);
        item.duration_ms_max = item.duration_ms_max.max(duration_ms);
        drop(metrics);
        self.observe_slow_operation(&format!("external:{service}"), duration);
    }

    pub fn observe_slow_operation(&self, operation: &str, duration: std::time::Duration) {
        let duration_ms = duration.as_millis() as u64;
        if duration_ms < *SLOW_OPERATION_THRESHOLD_MS {
            return;
        }
        let mut metrics = self
            .slow_operations
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let item = metrics.entry(operation.to_string()).or_default();
        item.count = item.count.saturating_add(1);
        item.last_duration_ms = duration_ms;
        item.max_duration_ms = item.max_duration_ms.max(duration_ms);
        tracing::warn!(
            operation,
            duration_ms,
            threshold_ms = *SLOW_OPERATION_THRESHOLD_MS,
            "slow operation detected"
        );
    }

    pub fn prometheus(&self) -> String {
        let snapshot = self.snapshot();
        let mut output = String::from("# HELP my_media_sub_info Application information.\n# TYPE my_media_sub_info gauge\nmy_media_sub_info 1\n");
        macro_rules! metric {
            ($name:literal, $kind:literal, $help:literal, $value:expr) => {
                output.push_str(concat!(
                    "# HELP ",
                    $name,
                    " ",
                    $help,
                    "\n# TYPE ",
                    $name,
                    " ",
                    $kind,
                    "\n"
                ));
                output.push_str(&format!(concat!($name, " {}\n"), $value));
            };
        }
        metric!(
            "my_media_sub_subscription_checks_total",
            "counter",
            "Subscription checks.",
            snapshot.subscription_checks
        );
        metric!(
            "my_media_sub_subscription_check_failures_total",
            "counter",
            "Failed subscription checks.",
            snapshot.subscription_check_failures
        );
        metric!(
            "my_media_sub_transfer_tasks_total",
            "counter",
            "Transfer tasks submitted.",
            snapshot.transfer_tasks
        );
        metric!(
            "my_media_sub_job_queue_depth",
            "gauge",
            "Queued and running jobs.",
            snapshot.job_queue_depth
        );
        metric!(
            "my_media_sub_job_store_update_failures_total",
            "counter",
            "Job store update failures in worker reliability paths.",
            snapshot.job_store_update_failures
        );
        metric!(
            "my_media_sub_push_sent_total",
            "counter",
            "Push messages sent.",
            snapshot.push_sent
        );
        metric!(
            "my_media_sub_push_failed_total",
            "counter",
            "Push message failures.",
            snapshot.push_failed
        );
        for (key, item) in snapshot.http_requests {
            let (method, status) = key.split_once(' ').unwrap_or(("unknown", "0"));
            output.push_str(&format!(
                "my_media_sub_http_requests_total{{method=\"{}\",status=\"{}\"}} {}\n",
                prometheus_escape(method),
                prometheus_escape(status),
                item.count
            ));
            output.push_str(&format!("my_media_sub_http_request_duration_seconds_sum{{method=\"{}\",status=\"{}\"}} {:.6}\n", prometheus_escape(method), prometheus_escape(status), item.duration_ms_total as f64 / 1000.0));
            output.push_str(&format!("my_media_sub_http_request_duration_seconds_count{{method=\"{}\",status=\"{}\"}} {}\n", prometheus_escape(method), prometheus_escape(status), item.count));
        }
        for (service, item) in snapshot.external_dependencies {
            let service = prometheus_escape(&service);
            output.push_str(&format!(
                "my_media_sub_external_requests_total{{service=\"{service}\"}} {}\n",
                item.count
            ));
            output.push_str(&format!(
                "my_media_sub_external_request_failures_total{{service=\"{service}\"}} {}\n",
                item.failures
            ));
            output.push_str(&format!("my_media_sub_external_request_duration_seconds_sum{{service=\"{service}\"}} {:.6}\n", item.duration_ms_total as f64 / 1000.0));
            output.push_str(&format!("my_media_sub_external_request_duration_seconds_count{{service=\"{service}\"}} {}\n", item.count));
        }
        for (operation, item) in snapshot.slow_operations {
            output.push_str(&format!(
                "my_media_sub_slow_operations_total{{operation=\"{}\"}} {}\n",
                prometheus_escape(&operation),
                item.count
            ));
        }
        output
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
            job_store_update_failures: self.job_store_update_failures.load(Ordering::Relaxed),
            backup_successes: self.backup_successes.load(Ordering::Relaxed),
            backup_failures: self.backup_failures.load(Ordering::Relaxed),
            restore_successes: self.restore_successes.load(Ordering::Relaxed),
            store_io: self
                .store_io
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
            http_requests: self
                .http_requests
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
            external_dependencies: self
                .external_dependencies
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
            slow_operations: self
                .slow_operations
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
            slow_operation_threshold_ms: *SLOW_OPERATION_THRESHOLD_MS,
        }
    }
}

fn prometheus_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
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

    #[test]
    fn job_store_update_failure_metric_increments() {
        let metrics = Metrics::default();
        metrics.increment_job_store_update_failure();
        metrics.increment_job_store_update_failure();
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.job_store_update_failures, 2);
        assert!(metrics
            .prometheus()
            .contains("my_media_sub_job_store_update_failures_total 2"));
    }
}
