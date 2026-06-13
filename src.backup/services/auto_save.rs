use crate::{
    clients::{QuarkClient, QuarkSaveClient},
    error::Result,
    models::{Resource, ResourceStatus},
    store::JsonStore,
    config::Config,
};
use std::sync::Arc;
use tracing::{info, warn};

/// 自动转存服务
pub struct AutoSaveService {
    pub config: Arc<Config>,
    pub resources: Arc<JsonStore<Resource>>,
}

impl AutoSaveService {
    /// 创建新的自动转存服务
    pub fn new(config: Arc<Config>, resources: Arc<JsonStore<Resource>>) -> Self {
        Self { config, resources }
    }

    /// 转存单个资源
    pub async fn save_resource(&self, resource_id: &str) -> Result<()> {
        let resource = self.resources
            .find(|r| r.id == resource_id)
            .await
            .ok_or_else(|| crate::error::AppError::NotFound("资源不存在".to_string()))?;

        if resource.status != ResourceStatus::Pending {
            info!("资源 {} 状态为 {:?}，跳过转存", resource.id, resource.status);
            return Ok(());
        }

        info!("开始转存资源: {} - {}", resource.id, resource.title);

        let cookie = self.config.quark.cookie.clone();

        // 1. 获取分享 token
        let mut probe_client = QuarkClient::new(cookie.clone());
        let pwd_id = QuarkClient::extract_pwd_id(&resource.share_url)
            .ok_or_else(|| crate::error::AppError::Validation("无效的分享链接".to_string()))?;

        let passcode = resource.share_pwd.as_deref().unwrap_or("");
        let stoken = probe_client.get_share_token(&pwd_id, passcode).await?;
        let files = probe_client.list_files(&pwd_id, &stoken, "0").await?;

        if files.is_empty() {
            warn!("资源 {} 没有可转存的文件", resource.id);
            self.resources
                .update(
                    |r| r.id == resource_id,
                    |r| r.status = ResourceStatus::Failed,
                )
                .await?;
            return Ok(());
        }

        // 2. 确保目标目录存在
        let save_client = QuarkSaveClient::new(cookie);
        let target_dir = resource.save_path
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("/");

        let target_fid = if !target_dir.is_empty() && target_dir != "/" {
            save_client.ensure_dir_path(target_dir).await?
        } else {
            self.config.quark.save_root_fid.clone()
        };

        // 3. 转存文件
        let fid_list: Vec<String> = files.iter().map(|f| f.fid.clone()).collect();
        let fid_token_list: Vec<String> = files.iter().map(|f| f.share_fid_token.clone()).collect();

        save_client.save_share_files(&pwd_id, &stoken, fid_list, fid_token_list, &target_fid).await?;

        // 4. 更新资源状态
        self.resources
            .update(
                |r| r.id == resource_id,
                |r| r.mark_transferred(target_dir.to_string()),
            )
            .await?;

        info!("资源 {} 转存成功", resource.id);
        Ok(())
    }

    /// 转存所有待处理资源
    pub async fn save_all_pending(&self) -> Result<usize> {
        let pending_resources = self.resources
            .filter(|r| r.status == ResourceStatus::Pending)
            .await;

        info!("开始转存 {} 个待处理资源", pending_resources.len());

        let mut success_count = 0;
        for resource in pending_resources {
            match self.save_resource(&resource.id).await {
                Ok(_) => {
                    success_count += 1;
                }
                Err(e) => {
                    warn!("转存资源 {} 失败: {}", resource.id, e);
                }
            }
        }

        info!("转存完成，成功 {} 个", success_count);
        Ok(success_count)
    }
}
