use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::clients::{QuarkSaveClient, QuarkSigninResult};
use crate::error::{AppError, Result};
use crate::jobs::JobQueue;
use crate::models::Notification;
use crate::services::notification::{
    add_notification, dispatch_push_event_for_notification, PushDispatchRequest,
};
use crate::services::push::{PushEvent, PushLevel};
use crate::services::timer::Timer;
use crate::store::{NotificationStore, SettingsStore};
use crate::utils::unix_now;

const SIGNIN_TIMEZONE_OFFSET_SECONDS: i32 = 8 * 60 * 60;

pub struct QuarkSigninService {
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
}

impl QuarkSigninService {
    pub fn new(
        settings_store: Arc<SettingsStore>,
        notification_store: Arc<NotificationStore>,
        job_queue: Option<Arc<JobQueue>>,
    ) -> Self {
        Self {
            settings_store,
            notification_store,
            job_queue,
        }
    }

    pub async fn signin(&self) -> Result<QuarkSigninResult> {
        let settings = self.settings_store.get().await;
        let mut cookie = settings.quark_signin_cookie.trim().to_string();
        if cookie.is_empty() {
            cookie = settings.quark_cookie.trim().to_string();
        }
        if cookie.is_empty() {
            return Err(AppError::Validation("未配置夸克签到 Cookie".to_string()));
        }

        let result = QuarkSaveClient::new(cookie).signin().await?;
        let notification = self.record_success(&result).await?;
        if result.signed {
            self.dispatch_success_push(&result, Some(notification.id))
                .await;
        }
        Ok(result)
    }

    pub async fn signin_with_failure_notice(&self) -> Result<QuarkSigninResult> {
        match self.signin().await {
            Ok(result) => Ok(result),
            Err(err) => {
                let notification_id = match self.record_failure(&err).await {
                    Ok(notification) => Some(notification.id),
                    Err(record_err) => {
                        error!("记录夸克签到失败通知失败: {}", record_err);
                        None
                    }
                };
                self.dispatch_failure_push(&err, notification_id).await;
                Err(err)
            }
        }
    }

    async fn record_success(&self, result: &QuarkSigninResult) -> Result<Notification> {
        let title = if result.already_signed {
            "夸克今日已签到"
        } else {
            "夸克签到成功"
        };
        let message = signin_message(result);
        add_notification(
            &self.notification_store,
            "success",
            PushEvent::QuarkSignin.as_str(),
            title,
            &message,
            HashMap::from([
                ("signed".to_string(), json!(result.signed)),
                ("already_signed".to_string(), json!(result.already_signed)),
                (
                    "daily_reward_bytes".to_string(),
                    json!(result.daily_reward_bytes),
                ),
                (
                    "total_capacity_bytes".to_string(),
                    json!(result.total_capacity_bytes),
                ),
                (
                    "used_capacity_bytes".to_string(),
                    json!(result.used_capacity_bytes),
                ),
                (
                    "sign_reward_bytes".to_string(),
                    json!(result.sign_reward_bytes),
                ),
                ("member_type".to_string(), json!(result.member_type)),
                ("sign_progress".to_string(), json!(result.sign_progress)),
                ("sign_target".to_string(), json!(result.sign_target)),
            ]),
        )
        .await
    }

    async fn record_failure(&self, err: &AppError) -> Result<Notification> {
        let message = signin_failure_message(err);
        add_notification(
            &self.notification_store,
            "error",
            PushEvent::QuarkSignin.as_str(),
            "夸克签到失败",
            &message,
            HashMap::from([("error".to_string(), json!(message))]),
        )
        .await
    }

    async fn dispatch_success_push(
        &self,
        result: &QuarkSigninResult,
        notification_id: Option<String>,
    ) {
        dispatch_push_event_for_notification(
            self.settings_store.clone(),
            self.notification_store.clone(),
            self.job_queue.clone(),
            PushDispatchRequest {
                notification_id,
                subscription_id: None,
                event: PushEvent::QuarkSignin,
                title: "夸克签到成功".to_string(),
                message: signin_message(result),
                level: PushLevel::Success,
            },
        )
        .await;
    }

    async fn dispatch_failure_push(&self, err: &AppError, notification_id: Option<String>) {
        dispatch_push_event_for_notification(
            self.settings_store.clone(),
            self.notification_store.clone(),
            self.job_queue.clone(),
            PushDispatchRequest {
                notification_id,
                subscription_id: None,
                event: PushEvent::QuarkSignin,
                title: "夸克签到失败".to_string(),
                message: signin_failure_message(err),
                level: PushLevel::Error,
            },
        )
        .await;
    }
}

pub struct QuarkSigninScheduler {
    service: Arc<QuarkSigninService>,
    settings_store: Arc<SettingsStore>,
    /// 当前生效的定时器；`None` 表示自动签到未启用或已停止。
    timer: Mutex<Option<Timer>>,
    running_guard: Arc<Mutex<()>>,
}

impl QuarkSigninScheduler {
    pub fn new(service: Arc<QuarkSigninService>, settings_store: Arc<SettingsStore>) -> Self {
        Self {
            service,
            settings_store,
            timer: Mutex::new(None),
            running_guard: Arc::new(Mutex::new(())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut active = self.timer.lock().await;
        let settings = self.settings_store.get().await;
        if let Some(timer) = active.take() {
            timer.stop().await;
        }
        if !settings.quark_signin_enabled {
            info!("夸克自动签到未启用");
            return Ok(());
        }

        let hour = normalize_hour(settings.quark_signin_hour);
        let service = self.service.clone();
        let running_guard = self.running_guard.clone();

        let timer = Timer::daily_at(
            "夸克自动签到",
            hour as u8,
            SIGNIN_TIMEZONE_OFFSET_SECONDS,
            move || {
                let service = service.clone();
                let running_guard = running_guard.clone();
                async move {
                    info!("定时执行夸克签到");
                    run_automatic_signin(&service, &running_guard).await;
                }
            },
        );
        *active = Some(timer);
        info!("夸克自动签到已启动，每天北京时间 {}:00 执行", hour);
        self.spawn_startup_catchup_if_needed(hour).await;
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        self.timer
            .lock()
            .await
            .as_ref()
            .is_some_and(Timer::is_running)
    }

    pub async fn stop(&self) -> Result<()> {
        if let Some(timer) = self.timer.lock().await.take() {
            timer.stop().await;
        }
        Ok(())
    }

    pub async fn reload(&self) -> Result<()> {
        self.start().await
    }

    async fn spawn_startup_catchup_if_needed(&self, hour: i32) {
        if !should_run_startup_catchup(hour, unix_now()) {
            return;
        }

        let notifications = self.service.notification_store.list(true).await;
        if successful_signin_recorded_today(&notifications, unix_now()) {
            info!("今日已有夸克签到成功记录，跳过启动补签");
            return;
        }

        let service = self.service.clone();
        let running_guard = self.running_guard.clone();
        tokio::spawn(async move {
            info!("今日夸克签到时间已过且无成功记录，立即执行补签");
            run_automatic_signin(&service, &running_guard).await;
        });
    }
}

async fn run_automatic_signin(service: &QuarkSigninService, running_guard: &Mutex<()>) {
    let Ok(_guard) = running_guard.try_lock() else {
        info!("上一轮夸克自动签到仍在执行，跳过重复触发");
        return;
    };
    let notifications = service.notification_store.list(true).await;
    if successful_signin_recorded_today(&notifications, unix_now()) {
        info!("今日已有夸克签到成功记录，跳过自动签到");
        return;
    }
    match service.signin_with_failure_notice().await {
        Ok(result) => info!("夸克自动签到完成: {}", signin_message(&result)),
        Err(err) => error!("夸克自动签到失败: {}", err),
    }
}

pub fn signin_message(result: &QuarkSigninResult) -> String {
    let action = if result.already_signed {
        "今日已签到"
    } else {
        "今日签到"
    };
    format!(
        "{} +{}，连签进度 {}/{}，{} 总空间 {}，签到累计获得 {}",
        action,
        format_bytes(result.daily_reward_bytes),
        result.sign_progress,
        result.sign_target,
        member_type_label(&result.member_type),
        format_bytes(result.total_capacity_bytes),
        format_bytes(result.sign_reward_bytes)
    )
}

pub fn signin_failure_message(err: &AppError) -> String {
    format!("夸克签到失败：{}", err)
}

fn normalize_hour(hour: i32) -> i32 {
    hour.clamp(0, 23)
}

/// 北京时间此刻是否已经过了设定的签到整点。`now` 为 Unix 秒。
fn should_run_startup_catchup(hour: i32, now: i64) -> bool {
    let seconds_today =
        (now.rem_euclid(86_400) + i64::from(SIGNIN_TIMEZONE_OFFSET_SECONDS)).rem_euclid(86_400);
    seconds_today / 3_600 >= i64::from(normalize_hour(hour))
}

fn successful_signin_recorded_today(notifications: &[Notification], now: i64) -> bool {
    let today = shanghai_day_index(now);
    notifications.iter().any(|notification| {
        notification.event == PushEvent::QuarkSignin.as_str()
            && notification.level == "success"
            && shanghai_day_index(notification.created_at) == today
    })
}

fn shanghai_day_index(timestamp: i64) -> i64 {
    (i128::from(timestamp) + i128::from(SIGNIN_TIMEZONE_OFFSET_SECONDS)).div_euclid(86_400) as i64
}

fn member_type_label(value: &str) -> &str {
    match value {
        "NORMAL" => "普通用户",
        "EXP_SVIP" => "88VIP",
        "SUPER_VIP" => "SVIP",
        "Z_VIP" => "SVIP+",
        _ => value,
    }
}

fn format_bytes(size: i64) -> String {
    crate::utils::format_bytes(size.max(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::store::{NotificationStore, SettingsStore};

    #[tokio::test]
    async fn start_reads_current_settings_after_acquiring_the_lifecycle_lock() {
        let dir = std::env::temp_dir().join(format!("quark-scheduler-{}", uuid::Uuid::new_v4()));
        let settings = Arc::new(SettingsStore::new(dir.join("settings.json")));
        settings.load().await.unwrap();
        settings
            .update(|value| value.quark_signin_enabled = true)
            .await
            .unwrap();
        let notifications = Arc::new(NotificationStore::new(dir.join("notifications.json")));
        notifications.load().await.unwrap();
        let service = Arc::new(QuarkSigninService::new(
            settings.clone(),
            notifications,
            None,
        ));
        let scheduler = QuarkSigninScheduler::new(service, settings.clone());

        let lock = scheduler.timer.lock().await;
        let mut starting = Box::pin(scheduler.start());
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(starting.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        settings
            .update(|value| value.quark_signin_enabled = false)
            .await
            .unwrap();
        drop(lock);
        starting.await.unwrap();
        assert!(!scheduler.is_running().await);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn overlapping_automatic_signin_does_not_create_another_attempt() {
        let dir =
            std::env::temp_dir().join(format!("quark-signin-overlap-{}", uuid::Uuid::new_v4()));
        let settings = Arc::new(SettingsStore::new(dir.join("settings.json")));
        settings.load().await.unwrap();
        let notifications = Arc::new(NotificationStore::new(dir.join("notifications.json")));
        notifications.load().await.unwrap();
        let service = QuarkSigninService::new(settings, notifications.clone(), None);
        let guard = Mutex::new(());
        let _running = guard.lock().await;
        run_automatic_signin(&service, &guard).await;
        assert!(
            notifications.list(true).await.is_empty(),
            "无 Cookie 的重复尝试也不应写出失败记录"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn beijing_day_boundaries_work_before_the_epoch_and_with_extreme_records() {
        assert_eq!(shanghai_day_index(-28_801), -1);
        assert_eq!(shanghai_day_index(-28_800), 0);
        assert_eq!(shanghai_day_index(86_400 - 28_801), 0);
        assert_eq!(shanghai_day_index(86_400 - 28_800), 1);
        assert!(shanghai_day_index(i64::MAX) > 0);
        assert!(shanghai_day_index(i64::MIN) < 0);
    }

    #[test]
    fn test_normalize_hour() {
        assert_eq!(normalize_hour(-1), 0);
        assert_eq!(normalize_hour(8), 8);
        assert_eq!(normalize_hour(24), 23);
    }

    fn unix(value: &str) -> i64 {
        value
            .parse::<jiff::Timestamp>()
            .expect("valid RFC 3339 timestamp")
            .as_second()
    }

    /// 定时器用整数运算算出下一次触发时刻；这里用日历库独立核对它确实落在
    /// 北京时间的整点，而且是 `now` 之后最近的那一个。
    #[test]
    fn signin_timer_targets_keep_beijing_hours() {
        let beijing = jiff::tz::Offset::from_seconds(SIGNIN_TIMEZONE_OFFSET_SECONDS).unwrap();
        for now in [
            unix("2026-06-29T07:59:59+08:00"),
            unix("2026-12-31T23:59:59+08:00"),
            unix("2028-02-29T12:00:00+08:00"),
        ] {
            for hour in [0u8, 8, 23] {
                let next = crate::services::timer::next_daily_occurrence(
                    now,
                    hour,
                    SIGNIN_TIMEZONE_OFFSET_SECONDS,
                );
                assert!(next > now && next - now <= 86_400);
                let local = beijing.to_datetime(jiff::Timestamp::from_second(next).unwrap());
                assert_eq!(
                    (local.hour(), local.minute(), local.second()),
                    (hour as i8, 0, 0)
                );
            }
        }
    }

    #[test]
    fn test_signin_message() {
        let message = signin_message(&QuarkSigninResult {
            signed: true,
            already_signed: false,
            daily_reward_bytes: 5 * 1024 * 1024,
            total_capacity_bytes: 1024 * 1024 * 1024,
            used_capacity_bytes: Some(256 * 1024 * 1024),
            sign_reward_bytes: 10 * 1024 * 1024,
            member_type: "SUPER_VIP".to_string(),
            sign_progress: 2,
            sign_target: 7,
        });

        assert!(message.contains("今日签到 +5.00 MB"));
        assert!(message.contains("连签进度 2/7"));
        assert!(message.contains("SVIP 总空间 1.00 GB"));
    }

    #[test]
    fn startup_catchup_runs_after_configured_hour() {
        assert!(should_run_startup_catchup(
            8,
            unix("2026-06-29T08:00:00+08:00")
        ));
        assert!(!should_run_startup_catchup(
            8,
            unix("2026-06-29T07:59:59+08:00")
        ));
        // 北京时间的凌晨仍是 UTC 的前一天，不能按 UTC 的小时判断。
        assert!(!should_run_startup_catchup(
            8,
            unix("2026-06-29T00:30:00+08:00")
        ));
        assert!(should_run_startup_catchup(
            0,
            unix("2026-06-29T00:00:00+08:00")
        ));
        assert!(should_run_startup_catchup(
            23,
            unix("2026-06-29T23:59:59+08:00")
        ));
    }

    #[test]
    fn detects_today_successful_signin_record() {
        let now = unix("2026-06-29T09:00:00+08:00");
        let yesterday = unix("2026-06-28T23:59:59+08:00");
        let today = unix("2026-06-29T00:00:01+08:00");

        let notifications = vec![
            Notification {
                id: "old".to_string(),
                level: "success".to_string(),
                event: PushEvent::QuarkSignin.as_str().to_string(),
                title: "夸克签到成功".to_string(),
                message: String::new(),
                meta: HashMap::new(),
                read: false,
                created_at: yesterday,
            },
            Notification {
                id: "today".to_string(),
                level: "success".to_string(),
                event: PushEvent::QuarkSignin.as_str().to_string(),
                title: "夸克签到成功".to_string(),
                message: String::new(),
                meta: HashMap::new(),
                read: false,
                created_at: today,
            },
        ];

        assert!(successful_signin_recorded_today(&notifications, now));
    }

    #[test]
    fn test_signin_failure_message() {
        let message =
            signin_failure_message(&AppError::Validation("未配置夸克签到 Cookie".to_string()));
        assert_eq!(
            message,
            "夸克签到失败：Validation error: 未配置夸克签到 Cookie"
        );
    }

    #[tokio::test]
    async fn signin_with_failure_notice_records_notification() {
        let base = std::env::temp_dir().join(format!(
            "my_media_sub_signin_failure_{}",
            uuid::Uuid::new_v4()
        ));
        let settings_path = base.join("settings.json");
        let notifications_path = base.join("notifications.json");

        let settings_store = Arc::new(SettingsStore::new(&settings_path));
        settings_store.load().await.unwrap();
        let notification_store = Arc::new(NotificationStore::new(&notifications_path));
        notification_store.load().await.unwrap();
        let service = QuarkSigninService::new(settings_store, notification_store.clone(), None);

        let err = service.signin_with_failure_notice().await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));

        let notifications = notification_store.list(true).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].level, "error");
        assert_eq!(notifications[0].event, PushEvent::QuarkSignin.as_str());
        assert_eq!(notifications[0].title, "夸克签到失败");
        assert!(notifications[0].message.contains("未配置夸克签到 Cookie"));

        let _ = std::fs::remove_dir_all(base);
    }
}
