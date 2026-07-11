use std::collections::HashMap;

use crate::error::AppError;

use super::model::{now, Job, JobErrorClass};
use super::scheduler::JobClass;

pub(crate) const MAX_AUTO_ATTEMPTS: u32 = 3;
pub(crate) const RETRY_BASE_SECONDS: i64 = 5;
pub(crate) const RETRY_MAX_SECONDS: i64 = 300;
pub(crate) const CIRCUIT_FAILURE_THRESHOLD: u32 = 3;
pub(crate) const CIRCUIT_RECOVERY_SECONDS: i64 = 60;
pub(crate) const JOB_STUCK_TIMEOUT_SECONDS: u64 = 30 * 60;
pub(crate) const JOB_BACKLOG_WARNING_THRESHOLD: usize = 100;
pub(crate) const JOB_HISTORY_RETAIN: usize = 300;

pub(crate) fn classify_app_error(error: &AppError) -> JobErrorClass {
    match error {
        AppError::RateLimited(_) => JobErrorClass::RateLimited,
        AppError::Http(_) => JobErrorClass::Transient,
        AppError::Validation(_) => JobErrorClass::Validation,
        AppError::NotFound(_) => JobErrorClass::NotFound,
        AppError::Config(message) if looks_like_authentication(message) => {
            JobErrorClass::Authentication
        }
        AppError::Config(_) => JobErrorClass::Permanent,
        AppError::Database(_) | AppError::Internal(_) => JobErrorClass::Internal,
    }
}

pub(crate) fn classify_message(message: &str) -> JobErrorClass {
    let lower = message.to_ascii_lowercase();
    if lower.contains("429") || lower.contains("rate limit") || message.contains("限流") {
        JobErrorClass::RateLimited
    } else if looks_like_authentication(message) {
        JobErrorClass::Authentication
    } else if lower.contains("timeout")
        || lower.contains("temporar")
        || lower.contains("connection")
        || lower.contains("http error")
        || lower.contains(" 500")
        || lower.contains(" 502")
        || lower.contains(" 503")
        || lower.contains("dns")
        || message.contains("超时")
        || message.contains("网络")
        || message.contains("连接")
    {
        JobErrorClass::Transient
    } else if message.contains("不存在") || lower.contains("not found") {
        JobErrorClass::NotFound
    } else if message.contains("无效") || message.contains("不能为空") {
        JobErrorClass::Validation
    } else {
        JobErrorClass::Permanent
    }
}

fn looks_like_authentication(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("cookie")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("token")
        || message.contains("认证")
        || message.contains("登录")
}

pub(crate) fn is_retryable(class: JobErrorClass) -> bool {
    matches!(
        class,
        JobErrorClass::RateLimited
            | JobErrorClass::Transient
            | JobErrorClass::Internal
            | JobErrorClass::TimedOut
    )
}

pub(crate) fn retry_delay_seconds(job_id: &str, attempt: u32) -> i64 {
    let exponent = attempt.saturating_sub(1).min(6);
    let base = RETRY_BASE_SECONDS.saturating_mul(1_i64 << exponent);
    let digest = md5::compute(format!("{job_id}:{attempt}"));
    let jitter_percent = i64::from(digest.0[0] % 41) - 20; // ±20%，测试可复现。
    (base + base * jitter_percent / 100).clamp(1, RETRY_MAX_SECONDS)
}

#[derive(Debug, Default)]
pub(crate) struct CircuitBreakers {
    states: HashMap<JobClass, CircuitState>,
}

#[derive(Debug, Default)]
struct CircuitState {
    consecutive_failures: u32,
    opened_at: Option<i64>,
    probe_in_flight: bool,
}

impl CircuitBreakers {
    pub(crate) fn allow(&mut self, class: JobClass, timestamp: i64) -> bool {
        let state = self.states.entry(class).or_default();
        let Some(opened_at) = state.opened_at else {
            return true;
        };
        if timestamp - opened_at < CIRCUIT_RECOVERY_SECONDS || state.probe_in_flight {
            return false;
        }
        state.probe_in_flight = true;
        true
    }

    pub(crate) fn record_success(&mut self, class: JobClass) {
        self.states.insert(class, CircuitState::default());
    }

    pub(crate) fn record_failure(&mut self, class: JobClass, error_class: JobErrorClass) {
        let state = self.states.entry(class).or_default();
        if !is_retryable(error_class) {
            // 半开探测已成功到达依赖，只是业务请求本身不可重试，说明依赖已恢复。
            *state = CircuitState::default();
            return;
        }
        state.probe_in_flight = false;
        state.consecutive_failures += 1;
        if state.consecutive_failures >= CIRCUIT_FAILURE_THRESHOLD {
            state.opened_at = Some(now());
        }
    }

    pub(crate) fn release_probe(&mut self, class: JobClass) {
        if let Some(state) = self.states.get_mut(&class) {
            state.probe_in_flight = false;
        }
    }
}

pub(crate) fn job_error_class(job: &Job) -> Option<JobErrorClass> {
    if job.status != super::model::JobStatus::Failed {
        return None;
    }
    Some(job.error_class.unwrap_or_else(|| {
        let details = format!(
            "{} {} {}",
            job.error.as_deref().unwrap_or_default(),
            job.message,
            job.result
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default()
        );
        classify_message(&details)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_separates_retryable_and_permanent_errors() {
        assert_eq!(
            classify_app_error(&AppError::RateLimited("429".into())),
            JobErrorClass::RateLimited
        );
        assert_eq!(
            classify_message("Cookie 已失效"),
            JobErrorClass::Authentication
        );
        assert_eq!(
            classify_message("connection timeout"),
            JobErrorClass::Transient
        );
        assert!(is_retryable(JobErrorClass::Transient));
        assert!(!is_retryable(JobErrorClass::Authentication));
    }

    #[test]
    fn exponential_backoff_is_bounded_and_deterministic() {
        let delays = (1..=8)
            .map(|attempt| retry_delay_seconds("job", attempt))
            .collect::<Vec<_>>();
        assert_eq!(
            delays,
            (1..=8)
                .map(|attempt| retry_delay_seconds("job", attempt))
                .collect::<Vec<_>>()
        );
        assert!(delays
            .iter()
            .all(|delay| (1..=RETRY_MAX_SECONDS).contains(delay)));
        assert!(delays[3] > delays[0]);
    }

    #[test]
    fn circuit_opens_and_allows_one_recovery_probe() {
        let mut circuits = CircuitBreakers::default();
        for _ in 0..CIRCUIT_FAILURE_THRESHOLD {
            circuits.record_failure(JobClass::Push, JobErrorClass::Transient);
        }
        let opened = circuits.states[&JobClass::Push].opened_at.unwrap();
        assert!(!circuits.allow(JobClass::Push, opened + 1));
        assert!(circuits.allow(JobClass::Push, opened + CIRCUIT_RECOVERY_SECONDS));
        assert!(!circuits.allow(JobClass::Push, opened + CIRCUIT_RECOVERY_SECONDS));
        circuits.record_success(JobClass::Push);
        assert!(circuits.allow(JobClass::Push, opened + CIRCUIT_RECOVERY_SECONDS));
    }
}
