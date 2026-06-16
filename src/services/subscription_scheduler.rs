use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::error::Result;
use crate::services::SubscriptionCheckService;
use crate::store::SettingsStore;

/// 订阅调度服务
pub struct SubscriptionScheduler {
    scheduler: JobScheduler,
    check_service: Arc<SubscriptionCheckService>,
    settings_store: Arc<SettingsStore>,
    job_id: Arc<RwLock<Option<uuid::Uuid>>>,
}

impl SubscriptionScheduler {
    /// 创建调度器
    pub async fn new(
        check_service: Arc<SubscriptionCheckService>,
        settings_store: Arc<SettingsStore>,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;

        Ok(Self {
            scheduler,
            check_service,
            settings_store,
            job_id: Arc::new(RwLock::new(None)),
        })
    }

    /// 启动调度器
    pub async fn start(&self) -> Result<()> {
        info!("启动订阅调度器");

        let settings = self.settings_store.get().await;
        let enabled = settings.subscription_scheduler_enabled;
        let interval_minutes = settings.subscription_check_interval_minutes;

        if !enabled {
            info!("订阅调度器未启用");
            return Ok(());
        }

        // 移除旧任务
        self.stop().await?;

        // 创建新任务
        let check_service = self.check_service.clone();
        let settings_store = self.settings_store.clone();

        // 构建 cron 表达式: 每 N 分钟运行一次
        let cron_expr = format!("0 */{} * * * *", interval_minutes);
        info!("订阅检查周期: 每 {} 分钟", interval_minutes);

        let job = Job::new_async(cron_expr.as_str(), move |_uuid, _l| {
            let check_service = check_service.clone();
            let settings_store = settings_store.clone();

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
                    }
                    Err(e) => {
                        error!("订阅检查失败: {}", e);
                    }
                }
            })
        })?;

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
