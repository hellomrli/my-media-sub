use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::clients::{QuarkSaveClient, QuarkSigninResult};
use crate::error::{AppError, Result};
use crate::jobs::JobQueue;
use crate::services::notification::{add_notification, dispatch_push_event};
use crate::services::push::{PushEvent, PushLevel};
use crate::store::{NotificationStore, SettingsStore};

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
        let cookie = settings.quark_cookie.trim().to_string();
        if cookie.is_empty() {
            return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
        }

        let result = QuarkSaveClient::new(cookie).signin().await?;
        self.record_success(&result).await?;
        if result.signed {
            self.dispatch_success_push(&result).await;
        }
        Ok(result)
    }

    async fn record_success(&self, result: &QuarkSigninResult) -> Result<()> {
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
                    "sign_reward_bytes".to_string(),
                    json!(result.sign_reward_bytes),
                ),
                ("member_type".to_string(), json!(result.member_type)),
                ("sign_progress".to_string(), json!(result.sign_progress)),
                ("sign_target".to_string(), json!(result.sign_target)),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn dispatch_success_push(&self, result: &QuarkSigninResult) {
        dispatch_push_event(
            self.settings_store.clone(),
            self.notification_store.clone(),
            self.job_queue.clone(),
            PushEvent::QuarkSignin,
            "夸克签到成功",
            signin_message(result),
            PushLevel::Success,
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

        let job = Job::new_async(cron_expr.as_str(), move |_uuid, _l| {
            let service = service.clone();
            Box::pin(async move {
                info!("定时执行夸克签到");
                match service.signin().await {
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
        info!("夸克自动签到已启动，每天 {}:00 执行", hour);
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

fn normalize_hour(hour: i32) -> i32 {
    hour.clamp(0, 23)
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
            sign_reward_bytes: 10 * 1024 * 1024,
            member_type: "SUPER_VIP".to_string(),
            sign_progress: 2,
            sign_target: 7,
        });

        assert!(message.contains("今日签到 +5.00 MB"));
        assert!(message.contains("连签进度 2/7"));
        assert!(message.contains("SVIP 总空间 1.00 GB"));
    }
}
