use crate::{
    error::Result,
    services::{SubscriptionChecker, AutoSaveService},
    config::Config,
    models::{Subscription, Resource},
    store::JsonStore,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{info, error};

/// 定时任务调度器
pub struct Scheduler {
    config: Arc<Config>,
    subscriptions: Arc<JsonStore<Subscription>>,
    resources: Arc<JsonStore<Resource>>,
}

impl Scheduler {
    /// 创建新的调度器
    pub fn new(
        config: Arc<Config>,
        subscriptions: Arc<JsonStore<Subscription>>,
        resources: Arc<JsonStore<Resource>>,
    ) -> Self {
        Self {
            config,
            subscriptions,
            resources,
        }
    }

    /// 启动定时任务
    pub async fn start(self: Arc<Self>) {
        info!("启动定时任务调度器");

        // 启动订阅检查任务（每 30 分钟）
        let scheduler = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(30 * 60));
            interval.tick().await; // 跳过第一次立即触发
            
            loop {
                interval.tick().await;
                info!("定时任务：开始检查订阅");
                
                let checker = SubscriptionChecker::new(
                    scheduler.config.clone(),
                    scheduler.subscriptions.clone(),
                    scheduler.resources.clone(),
                );
                
                match checker.check_all().await {
                    Ok(count) => {
                        info!("定时任务：检查完成，发现 {} 个新资源", count);
                    }
                    Err(e) => {
                        error!("定时任务：检查订阅失败: {}", e);
                    }
                }
            }
        });

        // 启动自动转存任务（每 5 分钟）
        let scheduler = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(5 * 60));
            interval.tick().await;
            
            loop {
                interval.tick().await;
                info!("定时任务：开始自动转存");
                
                let service = AutoSaveService::new(
                    scheduler.config.clone(),
                    scheduler.resources.clone(),
                );
                
                match service.save_all_pending().await {
                    Ok(count) => {
                        info!("定时任务：转存完成，成功 {} 个", count);
                    }
                    Err(e) => {
                        error!("定时任务：自动转存失败: {}", e);
                    }
                }
            }
        });

        info!("定时任务调度器启动完成");
    }

    /// 手动触发订阅检查
    pub async fn trigger_check(&self) -> Result<usize> {
        let checker = SubscriptionChecker::new(
            self.config.clone(),
            self.subscriptions.clone(),
            self.resources.clone(),
        );
        checker.check_all().await
    }

    /// 手动触发自动转存
    pub async fn trigger_save(&self) -> Result<usize> {
        let service = AutoSaveService::new(
            self.config.clone(),
            self.resources.clone(),
        );
        service.save_all_pending().await
    }
}
