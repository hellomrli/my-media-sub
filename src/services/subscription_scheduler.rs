use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Mutex, RwLock};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::error::Result;
use crate::jobs::JobQueue;
use crate::models::settings::normalize_check_interval_minutes;
use crate::services::notification::dispatch_push_event;
use crate::services::push::{PushEvent, PushLevel};
use crate::services::{subscription_check::CheckResult, SubscriptionCheckService};
use crate::store::{NotificationStore, SettingsStore};

/// 只在 ticker 尚未启动时调用 `JobScheduler::start()`。
///
/// `tokio-cron-scheduler` 0.13 的 `JobScheduler::start()` **不是幂等的**：内层
/// `Scheduler::ticking` 一旦置真就永不复位，第二次调用必然返回
/// `JobSchedulerError::StartScheduler`（内层 `TickError` 被包装成该变体）。
///
/// 旧实现在每次 `reload()` 里无条件 `start()`，而 `stop()` 只摘除 job、不关闭
/// ticker，因此**首次启动之后每次保存设置都会返回错误**，尽管配置已经写盘、
/// 新任务也已挂上。这里用一次性标志把 ticker 启动与任务增删解耦：
/// `reload()` 只需替换 job，ticker 保持运行。
pub(crate) async fn ensure_ticker_started(
    scheduler: &JobScheduler,
    started: &AtomicBool,
) -> Result<()> {
    // compare_exchange 保证并发 reload 时只有一个调用真正 start；失败时复位，
    // 让下一次 reload 还能重试。
    if started
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }
    if let Err(error) = scheduler.start().await {
        started.store(false, Ordering::SeqCst);
        return Err(error.into());
    }
    Ok(())
}

/// 订阅调度服务
pub struct SubscriptionScheduler {
    scheduler: JobScheduler,
    check_service: Arc<SubscriptionCheckService>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    job_id: Arc<RwLock<Option<uuid::Uuid>>>,
    ticker_started: AtomicBool,
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
            ticker_started: AtomicBool::new(false),
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
        let running_guard = Arc::new(Mutex::new(()));

        info!("订阅检查周期: 每 {} 分钟", interval_minutes);

        let job = Job::new_repeated_async(
            Duration::from_secs(interval_minutes * 60),
            move |_uuid, _l| {
                let check_service = check_service.clone();
                let settings_store = settings_store.clone();
                let notification_store = notification_store.clone();
                let job_queue = job_queue.clone();
                let running_guard = running_guard.clone();

                Box::pin(async move {
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
                })
            },
        )?;

        let job_uuid = self.scheduler.add(job).await?;
        *self.job_id.write().await = Some(job_uuid);

        ensure_ticker_started(&self.scheduler, &self.ticker_started).await?;

        info!("✅ 订阅调度器已启动 (每 {} 分钟检查一次)", interval_minutes);

        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        self.job_id.read().await.is_some()
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
    pub async fn reload(&self) -> Result<()> {
        info!("重新加载订阅调度器配置");
        self.stop().await?;
        self.start().await?;
        Ok(())
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

    /// 记录本模块存在的根因：crate 的 `start()` 不幂等。
    /// 这个断言一旦失败，说明上游修好了幂等性，`ensure_ticker_started` 可以简化。
    #[tokio::test]
    async fn crate_job_scheduler_start_is_not_idempotent() {
        let scheduler = JobScheduler::new().await.expect("构造调度器");
        assert!(scheduler.start().await.is_ok(), "首次 start 应当成功");
        assert!(
            scheduler.start().await.is_err(),
            "第二次 start 必然失败（ticking 不复位）——这正是 reload 报错的根因"
        );
    }

    /// 回归：`reload()` 反复调用必须一直成功。
    /// 旧实现每次 reload 都无条件 `scheduler.start()`，于是首次启动之后
    /// 每次保存设置都会返回错误，而设置其实已经生效。
    #[tokio::test]
    async fn ticker_is_started_only_once_across_repeated_reloads() {
        let scheduler = JobScheduler::new().await.expect("构造调度器");
        let started = AtomicBool::new(false);
        for round in 0..4 {
            ensure_ticker_started(&scheduler, &started)
                .await
                .unwrap_or_else(|error| panic!("第 {} 次启动不应失败: {}", round + 1, error));
        }
        assert!(started.load(Ordering::SeqCst));
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
