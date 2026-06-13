use crate::{
    clients::{PanSouClient, QuarkClient},
    error::Result,
    models::{Subscription, Resource, ResourceSource, SubscriptionStatus},
    store::JsonStore,
    config::Config,
};
use std::sync::Arc;
use tracing::{info, warn};

/// 订阅检查服务
pub struct SubscriptionChecker {
    pub config: Arc<Config>,
    pub subscriptions: Arc<JsonStore<Subscription>>,
    pub resources: Arc<JsonStore<Resource>>,
}

impl SubscriptionChecker {
    /// 创建新的检查器
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

    /// 检查单个订阅
    pub async fn check_subscription(&self, subscription_id: &str) -> Result<Vec<Resource>> {
        let subscription = self.subscriptions
            .find(|s| s.id == subscription_id)
            .await
            .ok_or_else(|| crate::error::AppError::NotFound("订阅不存在".to_string()))?;

        if subscription.status != SubscriptionStatus::Active {
            info!("订阅 {} 状态为 {:?}，跳过检查", subscription.id, subscription.status);
            return Ok(Vec::new());
        }

        info!("检查订阅: {} - {}", subscription.id, subscription.name);

        let mut new_resources = Vec::new();

        // 1. 搜索资源
        let search_client = PanSouClient::new(None);
        for keyword in &subscription.keywords {
            let results = search_client.search_quark(keyword, 20).await?;
            info!("关键词 '{}' 找到 {} 个结果", keyword, results.len());

            for result in results {
                // 检查是否已存在
                let exists = self.resources
                    .find(|r| r.share_url == result.url)
                    .await
                    .is_some();

                if exists {
                    continue;
                }

                // 2. 探测分享链接
                let cookie = self.config.quark.cookie.clone();
                let mut quark_client = QuarkClient::new(cookie);
                let share_info = quark_client.probe(&result.url, &result.password, 300).await;

                if !share_info.ok {
                    warn!("分享链接无效: {} - {}", result.url, share_info.message);
                    continue;
                }

                // 3. 创建资源
                let mut resource = Resource::new(
                    result.note.clone(),
                    result.url.clone(),
                    if result.password.is_empty() { None } else { Some(result.password.clone()) },
                    ResourceSource::Search,
                );
                
                resource.subscription_id = Some(subscription.id.clone());
                resource.file_count = Some(share_info.file_count);
                resource.episode_count = Some(share_info.episode_count);

                new_resources.push(resource.clone());
                
                // 保存到数据库
                self.resources.add(resource).await?;
                info!("发现新资源: {} (文件数: {}, 集数: {})", 
                    result.note, share_info.file_count, share_info.episode_count);
            }
        }

        // 4. 更新订阅检查时间
        self.subscriptions
            .update(
                |s| s.id == subscription_id,
                |s| s.update_check_time(),
            )
            .await?;

        Ok(new_resources)
    }

    /// 检查所有活跃订阅
    pub async fn check_all(&self) -> Result<usize> {
        let subscriptions = self.subscriptions
            .filter(|s| s.status == SubscriptionStatus::Active)
            .await;

        info!("开始检查 {} 个活跃订阅", subscriptions.len());

        let mut total_new = 0;
        for subscription in subscriptions {
            match self.check_subscription(&subscription.id).await {
                Ok(resources) => {
                    total_new += resources.len();
                }
                Err(e) => {
                    warn!("检查订阅 {} 失败: {}", subscription.id, e);
                }
            }
        }

        info!("检查完成，共发现 {} 个新资源", total_new);
        Ok(total_new)
    }
}
