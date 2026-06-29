use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, FixedOffset, Timelike, Utc};
use serde_json::json;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::clients::{QuarkSaveClient, QuarkSigninResult};
use crate::error::{AppError, Result};
use crate::jobs::JobQueue;
use crate::models::Notification;
use crate::services::notification::{
    add_notification, dispatch_push_event_for_notification, PushDispatchRequest,
};
use crate::services::push::{PushEvent, PushLevel};
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
    scheduler: JobScheduler,
    service: Arc<QuarkSigninService>,
    settings_store: Arc<SettingsStore>,
    job_id: Arc<RwLock<Option<uuid::Uuid>>>,
}

impl QuarkSigninScheduler {
    pub async fn new(
        service: Arc<QuarkSigninService>,
        settings_store: Arc<SettingsStore>,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;
        Ok(Self {
            scheduler,
            service,
            settings_store,
            job_id: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn start(&self) -> Result<()> {
        let settings = self.settings_store.get().await;
        if !settings.quark_signin_enabled {
            info!("夸克自动签到未启用");
            return Ok(());
        }

        self.stop().await?;

        let hour = normalize_hour(settings.quark_signin_hour);
        let cron_expr = format!("0 0 {} * * *", hour);
        let service = self.service.clone();

        let job = Job::new_async_tz(cron_expr.as_str(), signin_timezone(), move |_uuid, _l| {
            let service = service.clone();
            Box::pin(async move {
                info!("定时执行夸克签到");
                match service.signin_with_failure_notice().await {
                    Ok(result) => {
                        if result.signed {
                            info!("夸克签到成功: {}", signin_message(&result));
                        } else {
                            info!("夸克今日已签到: {}", signin_message(&result));
                        }
                    }
                    Err(err) => {
                        error!("夸克签到失败: {}", err);
                    }
                }
            })
        })?;

        let job_uuid = self.scheduler.add(job).await?;
        *self.job_id.write().await = Some(job_uuid);
        self.scheduler.start().await?;
        info!("夸克自动签到已启动，每天北京时间 {}:00 执行", hour);
        self.spawn_startup_catchup_if_needed(hour).await;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut job_id = self.job_id.write().await;
        if let Some(uuid) = *job_id {
            if let Err(err) = self.scheduler.remove(&uuid).await {
                error!("移除夸克签到任务失败: {}", err);
            }
            *job_id = None;
        }
        Ok(())
    }

    pub async fn reload(&self) -> Result<()> {
        self.stop().await?;
        self.start().await
    }

    async fn spawn_startup_catchup_if_needed(&self, hour: i32) {
        let now = Utc::now().with_timezone(&signin_timezone());
        if !should_run_startup_catchup(hour, now) {
            return;
        }

        let notifications = self.service.notification_store.list(true).await;
        if successful_signin_recorded_today(&notifications, unix_now()) {
            info!("今日已有夸克签到成功记录，跳过启动补签");
            return;
        }

        let service = self.service.clone();
        tokio::spawn(async move {
            info!("今日夸克签到时间已过且无成功记录，立即执行补签");
            match service.signin_with_failure_notice().await {
                Ok(result) => {
                    if result.signed {
                        info!("夸克补签成功: {}", signin_message(&result));
                    } else {
                        info!("夸克今日已签到: {}", signin_message(&result));
                    }
                }
                Err(err) => {
                    error!("夸克补签失败: {}", err);
                }
            }
        });
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

fn signin_timezone() -> FixedOffset {
    FixedOffset::east_opt(SIGNIN_TIMEZONE_OFFSET_SECONDS).expect("valid signin timezone")
}

fn should_run_startup_catchup(hour: i32, now: DateTime<FixedOffset>) -> bool {
    now.hour() >= normalize_hour(hour) as u32
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
    (timestamp + i64::from(SIGNIN_TIMEZONE_OFFSET_SECONDS)) / 86_400
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
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = size.max(0) as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < units.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", size as i64, units[unit])
    } else {
        format!("{:.2} {}", size, units[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::store::{NotificationStore, SettingsStore};

    #[test]
    fn test_normalize_hour() {
        assert_eq!(normalize_hour(-1), 0);
        assert_eq!(normalize_hour(8), 8);
        assert_eq!(normalize_hour(24), 23);
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
        let now = DateTime::parse_from_rfc3339("2026-06-29T08:00:00+08:00")
            .unwrap()
            .with_timezone(&signin_timezone());
        assert!(should_run_startup_catchup(8, now));

        let before = DateTime::parse_from_rfc3339("2026-06-29T07:59:59+08:00")
            .unwrap()
            .with_timezone(&signin_timezone());
        assert!(!should_run_startup_catchup(8, before));
    }

    #[test]
    fn detects_today_successful_signin_record() {
        let now = DateTime::parse_from_rfc3339("2026-06-29T09:00:00+08:00")
            .unwrap()
            .timestamp();
        let yesterday = DateTime::parse_from_rfc3339("2026-06-28T23:59:59+08:00")
            .unwrap()
            .timestamp();
        let today = DateTime::parse_from_rfc3339("2026-06-29T00:00:01+08:00")
            .unwrap()
            .timestamp();

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
