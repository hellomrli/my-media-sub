use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::error::Result;
use crate::jobs::JobQueue;
use crate::services::notification::dispatch_push_event;
use crate::services::push::{PushEvent, PushLevel};
use crate::services::{subscription_check::CheckResult, SubscriptionCheckService};
use crate::store::{NotificationStore, SettingsStore};

/// 订阅调度服务
pub struct SubscriptionScheduler {
    scheduler: JobScheduler,
    check_service: Arc<SubscriptionCheckService>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    job_id: Arc<RwLock<Option<uuid::Uuid>>>,
}

impl SubscriptionScheduler {
    /// 创建调度器
    pub async fn new(
        check_service: Arc<SubscriptionCheckService>,
        settings_store: Arc<SettingsStore>,
        notification_store: Arc<NotificationStore>,
        job_queue: Option<Arc<JobQueue>>,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;

        Ok(Self {
            scheduler,
            check_service,
            settings_store,
            notification_store,
            job_queue,
            job_id: Arc::new(RwLock::new(None)),
        })
    }

    /// 启动调度器
    pub async fn start(&self) -> Result<()> {
        info!("启动订阅调度器");

        let settings = self.settings_store.get().await;
        let enabled = settings.subscription_scheduler_enabled;
        let interval_minutes =
            normalize_interval_minutes(settings.subscription_check_interval_minutes);

        if !enabled {
            info!("订阅调度器未启用");
            return Ok(());
        }

        // 移除旧任务
        self.stop().await?;

        // 创建新任务
        let check_service = self.check_service.clone();
        let settings_store = self.settings_store.clone();
        let notification_store = self.notification_store.clone();
        let job_queue = self.job_queue.clone();

        info!("订阅检查周期: 每 {} 分钟", interval_minutes);

        let job = Job::new_repeated_async(
            Duration::from_secs(interval_minutes * 60),
            move |_uuid, _l| {
                let check_service = check_service.clone();
                let settings_store = settings_store.clone();
                let notification_store = notification_store.clone();
                let job_queue = job_queue.clone();

                Box::pin(async move {
                    info!("⏰ 定时检查订阅");

                    let settings = settings_store.get().await;
                    let cookie = settings.quark_cookie.clone();

                    if cookie.is_empty() {
                        error!("未配置夸克 Cookie，跳过订阅检查");
                        return;
                    }

                    match check_service.check_all_subscriptions(&cookie).await {
                        Ok(results) => {
                            let total = results.len();
                            let updated: Vec<_> =
                                results.iter().filter(|r| !r.new_files.is_empty()).collect();

                            if updated.is_empty() {
                                info!("✅ 检查完成，共 {} 个订阅，无更新", total);
                            } else {
                                info!(
                                    "✅ 检查完成，共 {} 个订阅，{} 个有更新",
                                    total,
                                    updated.len()
                                );
                            }
                            dispatch_subscription_check_summary(
                                settings_store.clone(),
                                notification_store.clone(),
                                job_queue.clone(),
                                &results,
                            )
                            .await;
                        }
                        Err(e) => {
                            error!("订阅检查失败: {}", e);
                        }
                    }
                })
            },
        )?;

        let job_uuid = self.scheduler.add(job).await?;
        *self.job_id.write().await = Some(job_uuid);

        self.scheduler.start().await?;

        info!("✅ 订阅调度器已启动 (每 {} 分钟检查一次)", interval_minutes);

        Ok(())
    }

    /// 停止调度器
    pub async fn stop(&self) -> Result<()> {
        let mut job_id = self.job_id.write().await;

        if let Some(uuid) = *job_id {
            if let Err(e) = self.scheduler.remove(&uuid).await {
                error!("移除调度任务失败: {}", e);
            } else {
                info!("已停止订阅调度任务");
            }
            *job_id = None;
        }

        Ok(())
    }

    /// 重新加载配置并重启
    #[allow(dead_code)]
    pub async fn reload(&self) -> Result<()> {
        info!("重新加载订阅调度器配置");
        self.stop().await?;
        self.start().await?;
        Ok(())
    }

    /// 手动触发一次检查
    #[allow(dead_code)]
    pub async fn trigger_manual_check(&self) -> Result<()> {
        info!("手动触发订阅检查");

        let settings = self.settings_store.get().await;
        let cookie = settings.quark_cookie.clone();

        if cookie.is_empty() {
            return Err(crate::error::AppError::Validation(
                "未配置夸克 Cookie".to_string(),
            ));
        }

        let results = self.check_service.check_all_subscriptions(&cookie).await?;

        let total = results.len();
        let updated: Vec<_> = results.iter().filter(|r| !r.new_files.is_empty()).collect();

        info!(
            "手动检查完成，共 {} 个订阅，{} 个有更新",
            total,
            updated.len()
        );

        Ok(())
    }
}

fn normalize_interval_minutes(minutes: i32) -> u64 {
    minutes.max(5) as u64
}

async fn dispatch_subscription_check_summary(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    results: &[CheckResult],
) {
    let message = subscription_check_summary_message(results);
    let level = if results.iter().any(|result| !result.new_files.is_empty()) {
        PushLevel::Success
    } else {
        PushLevel::Info
    };

    dispatch_push_event(
        settings_store,
        notification_store,
        job_queue,
        PushEvent::SubscriptionUpdated,
        "订阅自动检查完成",
        message,
        level,
    )
    .await;
}

fn subscription_check_summary_message(results: &[CheckResult]) -> String {
    let updated: Vec<&CheckResult> = results
        .iter()
        .filter(|result| !result.new_files.is_empty())
        .collect();
    let unchanged: Vec<&CheckResult> = results
        .iter()
        .filter(|result| {
            result.new_files.is_empty() && !result.became_invalid && !result.became_completed
        })
        .collect();
    let invalid: Vec<&CheckResult> = results
        .iter()
        .filter(|result| result.became_invalid)
        .collect();
    let completed: Vec<&CheckResult> = results
        .iter()
        .filter(|result| result.became_completed)
        .collect();

    let mut lines = vec![format!(
        "本次检查 {} 个订阅，{} 个有更新，{} 个无更新。",
        results.len(),
        updated.len(),
        unchanged.len()
    )];

    append_subscription_section(&mut lines, "有更新", &updated, |result| {
        format!(
            "{}：{} 个新文件{}",
            result.subscription_title,
            result.new_files.len(),
            file_preview_suffix(&result.new_files)
        )
    });
    append_subscription_section(&mut lines, "无更新", &unchanged, |result| {
        result.subscription_title.clone()
    });
    append_subscription_section(&mut lines, "已失效", &invalid, |result| {
        format!("{}：{}", result.subscription_title, result.summary)
    });
    append_subscription_section(&mut lines, "已完结", &completed, |result| {
        result.subscription_title.clone()
    });

    lines.join("\n")
}

fn append_subscription_section<F>(
    lines: &mut Vec<String>,
    title: &str,
    items: &[&CheckResult],
    format_item: F,
) where
    F: Fn(&CheckResult) -> String,
{
    if items.is_empty() {
        return;
    }

    lines.push(format!(
        "{}：{}",
        title,
        format_limited_items(items, format_item)
    ));
}

fn format_limited_items<F>(items: &[&CheckResult], format_item: F) -> String
where
    F: Fn(&CheckResult) -> String,
{
    let mut values: Vec<String> = items
        .iter()
        .take(10)
        .map(|item| format_item(item))
        .collect();
    if items.len() > 10 {
        values.push(format!("另 {} 个", items.len() - 10));
    }
    values.join("、")
}

fn file_preview_suffix(files: &[String]) -> String {
    if files.is_empty() {
        return String::new();
    }

    let mut preview: Vec<String> = files.iter().take(3).cloned().collect();
    if files.len() > 3 {
        preview.push(format!("另 {} 个", files.len() - 3));
    }
    format!("（{}）", preview.join("、"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::subscription_check::CheckDetails;

    fn check_result(title: &str, new_files: Vec<&str>) -> CheckResult {
        CheckResult {
            subscription_id: title.to_string(),
            subscription_title: title.to_string(),
            new_files: new_files.into_iter().map(ToString::to_string).collect(),
            new_episodes: vec![],
            details: CheckDetails::default(),
            became_invalid: false,
            became_completed: false,
            summary: "无更新".to_string(),
        }
    }

    #[test]
    fn normalizes_interval_minutes_for_scheduler() {
        assert_eq!(normalize_interval_minutes(-1), 5);
        assert_eq!(normalize_interval_minutes(0), 5);
        assert_eq!(normalize_interval_minutes(60), 60);
        assert_eq!(normalize_interval_minutes(720), 720);
    }

    #[test]
    fn summary_message_lists_updated_and_unchanged_subscriptions() {
        let message = subscription_check_summary_message(&[
            check_result("庆余年", vec!["S02E01.mkv", "S02E02.mkv"]),
            check_result("孤独摇滚", vec![]),
        ]);

        assert!(message.contains("本次检查 2 个订阅，1 个有更新，1 个无更新。"));
        assert!(message.contains("有更新：庆余年：2 个新文件"));
        assert!(message.contains("无更新：孤独摇滚"));
    }
}
