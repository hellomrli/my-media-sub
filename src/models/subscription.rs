use serde::{Deserialize, Serialize};

/// 订阅类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Movie,
    Series,
    Anime,
    Custom(String),
}

/// 订阅状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionStatus {
    Active,
    Completed,
    Expired,
    Paused,
}

/// 订阅
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// 订阅 ID
    pub id: String,
    
    /// 订阅名称
    pub name: String,
    
    /// 媒体类型
    pub media_type: MediaType,
    
    /// 搜索关键词
    pub keywords: Vec<String>,
    
    /// 夸克分享链接
    #[serde(default)]
    pub share_url: Option<String>,
    
    /// 夸克分享密码
    #[serde(default)]
    pub share_pwd: Option<String>,
    
    /// 保存目录
    #[serde(default)]
    pub save_dir: String,
    
    /// 订阅状态
    pub status: SubscriptionStatus,
    
    /// 最后检查时间（Unix 时间戳）
    #[serde(default)]
    pub last_check: Option<i64>,
    
    /// 最后更新时间（Unix 时间戳）
    #[serde(default)]
    pub last_update: Option<i64>,
    
    /// 创建时间（Unix 时间戳）
    pub created_at: i64,
    
    /// 更新时间（Unix 时间戳）
    pub updated_at: i64,
    
    /// 备注
    #[serde(default)]
    pub notes: String,
}

impl Subscription {
    /// 创建新订阅
    pub fn new(name: String, media_type: MediaType, keywords: Vec<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            media_type,
            keywords,
            share_url: None,
            share_pwd: None,
            save_dir: String::new(),
            status: SubscriptionStatus::Active,
            last_check: None,
            last_update: None,
            created_at: now,
            updated_at: now,
            notes: String::new(),
        }
    }
    
    /// 标记为已完成
    pub fn mark_completed(&mut self) {
        self.status = SubscriptionStatus::Completed;
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
    }
    
    /// 标记为已过期
    pub fn mark_expired(&mut self) {
        self.status = SubscriptionStatus::Expired;
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
    }
    
    /// 更新最后检查时间
    pub fn update_check_time(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        self.last_check = Some(now);
        self.updated_at = now;
    }
    
    /// 更新最后更新时间
    pub fn update_last_update(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        self.last_update = Some(now);
        self.updated_at = now;
    }
}

/// 创建订阅请求
#[derive(Debug, Deserialize)]
pub struct CreateSubscriptionRequest {
    pub name: String,
    pub media_type: MediaType,
    pub keywords: Vec<String>,
    #[serde(default)]
    pub share_url: Option<String>,
    #[serde(default)]
    pub share_pwd: Option<String>,
    #[serde(default)]
    pub save_dir: String,
    #[serde(default)]
    pub notes: String,
}

/// 更新订阅请求
#[derive(Debug, Deserialize)]
pub struct UpdateSubscriptionRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
    #[serde(default)]
    pub share_url: Option<String>,
    #[serde(default)]
    pub share_pwd: Option<String>,
    #[serde(default)]
    pub save_dir: Option<String>,
    #[serde(default)]
    pub status: Option<SubscriptionStatus>,
    #[serde(default)]
    pub notes: Option<String>,
}
