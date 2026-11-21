use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::error::Result;
use crate::jobs::JobQueue;
use crate::models::settings::normalize_check_interval_minutes;
use crate::services::notification::dispatch_push_event;
use crate::services::push::{PushEvent, PushLevel};
use crate::services::timer::Timer;
use crate::services::{subscription_check::CheckResult, SubscriptionCheckService};
use crate::store::{NotificationStore, SettingsStore};

/// 订阅调度服务
pub struct SubscriptionScheduler {
    check_service: Arc<SubscriptionCheckService>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    /// 当前生效的定时器；`None` 表示调度未启用或已停止。
    timer: Mutex<Option<Timer>>,
    /// 重载定时器时仍与尚未结束的上一轮共用执行守卫。
    running_guard: Arc<Mutex<()>>,
}

impl SubscriptionScheduler {
    /// 创建调度器
    pub fn new(
        check_service: Arc<SubscriptionCheckService>,
        settings_store: Arc<SettingsStore>,
        notification_store: Arc<NotificationStore>,
        job_queue: Option<Arc<JobQueue>>,
    ) -> Self {
        Self {
            check_service,
            settings_store,
            notification_store,
            job_queue,
            timer: Mutex::new(None),
            running_guard: Arc::new(Mutex::new(())),
        }
    }

    /// 启动调度器
    pub async fn start(&self) -> Result<()> {
        info!("启动订阅调度器");

        // 读取配置、停止旧循环和安装新循环串行执行，避免并发保存/停止后留下旧任务。
        let mut active = self.timer.lock().await;
        let settings = self.settings_store.get().await;
        let enabled = settings.subscription_scheduler_enabled;
        let interval_minutes =
            normalize_interval_minutes(settings.subscription_check_interval_minutes);

        if let Some(timer) = active.take() {
            timer.stop().await;
        }
        if !enabled {
            info!("订阅调度器未启用");
            return Ok(());
        }

        // 创建新任务
        let check_service = self.check_service.clone();
        let settings_store = self.settings_store.clone();
        let notification_store = self.notification_store.clone();
        let job_queue = self.job_queue.clone();
        let running_guard = self.running_guard.clone();

        info!("订阅检查周期: 每 {} 分钟", interval_minutes);

        let timer = Timer::every(
            "订阅自动检查",
            Duration::from_secs(interval_minutes * 60),
            move || {
                let check_service = check_service.clone();
                let settings_store = settings_store.clone();
                let notification_store = notification_store.clone();
                let job_queue = job_queue.clone();
                let running_guard = running_guard.clone();

                async move {
                    info!("⏰ 定时检查订阅");
                    let Ok(_guard) = running_guard.try_lock() else {
                        info!("上一次订阅自动检查仍在执行，跳过本次调度");
                        return;
                    };

                    let settings = settings_store.get().await;
                    let cookie = settings.quark_cookie.clone();

                    if cookie.is_empty() {
                        error!("未配置夸克 Cookie，跳过订阅检查");
                        return;
                    }

                    match check_service.check_due_subscriptions(&cookie).await {
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
                }
            },
        );
        *active = Some(timer);

        info!("✅ 订阅调度器已启动 (每 {} 分钟检查一次)", interval_minutes);

        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        self.timer
            .lock()
            .await
            .as_ref()
            .is_some_and(Timer::is_running)
    }

    /// 停止调度器。已经开始的那一轮检查会继续跑完，只是不再安排下一轮。
    pub async fn stop(&self) -> Result<()> {
        if let Some(timer) = self.timer.lock().await.take() {
            timer.stop().await;
            info!("已停止订阅调度任务");
        }

        Ok(())
    }

    /// 重新加载配置并重启
    pub async fn reload(&self) -> Result<()> {
        info!("重新加载订阅调度器配置");
        self.start().await
    }
}

fn normalize_interval_minutes(minutes: i32) -> u64 {
    normalize_check_interval_minutes(i64::from(minutes)) as u64
}

async fn dispatch_subscription_check_summary(
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    results: &[CheckResult],
) {
    if !should_dispatch_subscription_check_summary(results) {
        return;
    }

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

fn should_dispatch_subscription_check_summary(results: &[CheckResult]) -> bool {
    results.iter().any(|result| {
        !result.new_files.is_empty() || result.became_invalid || result.became_completed
    })
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
    use crate::store::SubscriptionStore;

    #[tokio::test]
    async fn start_reads_current_settings_after_acquiring_the_lifecycle_lock() {
        let dir =
            std::env::temp_dir().join(format!("subscription-scheduler-{}", uuid::Uuid::new_v4()));
        let settings = Arc::new(SettingsStore::new(dir.join("settings.json")));
        settings.load().await.unwrap();
        settings
            .update(|value| value.subscription_scheduler_enabled = true)
            .await
            .unwrap();
        let notifications = Arc::new(NotificationStore::new(dir.join("notifications.json")));
        notifications.load().await.unwrap();
        let subscriptions = Arc::new(SubscriptionStore::new(dir.join("subscriptions.json")));
        subscriptions.load().await.unwrap();
        let service = Arc::new(SubscriptionCheckService::new(
            subscriptions,
            settings.clone(),
            notifications.clone(),
        ));
        let scheduler = SubscriptionScheduler::new(service, settings.clone(), notifications, None);

        let lock = scheduler.timer.lock().await;
        let mut starting = Box::pin(scheduler.start());
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(starting.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        // start 正在等生命周期锁；期间设置已经关闭，不能安装先前读到的启用配置。
        settings
            .update(|value| value.subscription_scheduler_enabled = false)
            .await
            .unwrap();
        drop(lock);
        starting.await.unwrap();
        assert!(!scheduler.is_running().await);
        let _ = std::fs::remove_dir_all(dir);
    }

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

    #[test]
    fn summary_dispatches_only_for_noteworthy_results() {
        assert!(!should_dispatch_subscription_check_summary(&[
            check_result("孤独摇滚", vec![])
        ]));
        assert!(should_dispatch_subscription_check_summary(&[check_result(
            "庆余年",
            vec!["S02E01.mkv"]
        )]));

        let mut invalid = check_result("凡人修仙传", vec![]);
        invalid.became_invalid = true;
        assert!(should_dispatch_subscription_check_summary(&[invalid]));
    }
}
