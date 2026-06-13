use serde::{Deserialize, Serialize};

/// 资源来源
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ResourceSource {
    /// 搜索发现
    Search,
    /// 手动添加
    Manual,
    /// 订阅更新
    Subscription,
}

/// 资源状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ResourceStatus {
    /// 待处理
    Pending,
    /// 转存中
    Transferring,
    /// 已转存
    Transferred,
    /// 失败
    Failed,
}

/// 资源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// 资源 ID
    pub id: String,
    
    /// 关联的订阅 ID
    #[serde(default)]
    pub subscription_id: Option<String>,
    
    /// 资源标题
    pub title: String,
    
    /// 夸克分享链接
    pub share_url: String,
    
    /// 分享密码
    #[serde(default)]
    pub share_pwd: Option<String>,
    
    /// 文件数量
    #[serde(default)]
    pub file_count: Option<usize>,
    
    /// 疑似集数
    #[serde(default)]
    pub episode_count: Option<usize>,
    
    /// 资源来源
    pub source: ResourceSource,
    
    /// 资源状态
    pub status: ResourceStatus,
    
    /// 保存路径
    #[serde(default)]
    pub save_path: Option<String>,
    
    /// 夸克任务 ID
    #[serde(default)]
    pub task_id: Option<String>,
    
    /// 发现时间（Unix 时间戳）
    pub discovered_at: i64,
    
    /// 转存时间（Unix 时间戳）
    #[serde(default)]
    pub transferred_at: Option<i64>,
    
    /// 备注
    #[serde(default)]
    pub notes: String,
}

impl Resource {
    /// 创建新资源
    pub fn new(
        title: String,
        share_url: String,
        share_pwd: Option<String>,
        source: ResourceSource,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            subscription_id: None,
            title,
            share_url,
            share_pwd,
            file_count: None,
            episode_count: None,
            source,
            status: ResourceStatus::Pending,
            save_path: None,
            task_id: None,
            discovered_at: now,
            transferred_at: None,
            notes: String::new(),
        }
    }
    
    /// 标记为转存中
    pub fn mark_transferring(&mut self, task_id: String) {
        self.status = ResourceStatus::Transferring;
        self.task_id = Some(task_id);
    }
    
    /// 标记为已转存
    pub fn mark_transferred(&mut self, save_path: String) {
        self.status = ResourceStatus::Transferred;
        self.save_path = Some(save_path);
        self.transferred_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
        );
    }
    
    /// 标记为失败
    pub fn mark_failed(&mut self, reason: String) {
        self.status = ResourceStatus::Failed;
        self.notes = reason;
    }
}

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// 标题
    pub title: String,
    
    /// 分享链接
    pub url: String,
    
    /// 分享密码
    #[serde(default)]
    pub pwd: Option<String>,
    
    /// 来源
    pub source: String,
    
    /// 文件大小
    #[serde(default)]
    pub size: Option<String>,
    
    /// 发布时间
    #[serde(default)]
    pub published_at: Option<String>,
}

/// 手动添加资源请求
#[derive(Debug, Deserialize)]
pub struct ManualResourceRequest {
    pub title: String,
    pub share_url: String,
    #[serde(default)]
    pub share_pwd: Option<String>,
    #[serde(default)]
    pub subscription_id: Option<String>,
}
