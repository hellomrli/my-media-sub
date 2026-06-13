use serde::{Deserialize, Serialize};

/// 应用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 夸克网盘设置
    pub quark: QuarkSettings,
    
    /// 推送设置
    pub push: PushSettings,
    
    /// 自定义分类
    #[serde(default)]
    pub custom_categories: Vec<CustomCategory>,
}

/// 夸克网盘设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkSettings {
    /// Cookie
    pub cookie: String,
    
    /// 保存根目录路径
    #[serde(default)]
    pub save_root: String,
    
    /// 保存根目录 fid
    #[serde(default)]
    pub save_root_fid: String,
}

/// 推送设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushSettings {
    /// 是否启用推送
    #[serde(default)]
    pub enabled: bool,
    
    /// 静默模式
    #[serde(default)]
    pub silent_mode: bool,
    
    /// 推送场景
    pub scenarios: PushScenarios,
    
    /// 推送渠道
    pub channels: PushChannels,
}

/// 推送场景配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushScenarios {
    /// 订阅更新推送
    #[serde(default = "default_true")]
    pub subscription_update: bool,
    
    /// 订阅失效推送
    #[serde(default = "default_true")]
    pub subscription_expired: bool,
    
    /// 订阅完结推送
    #[serde(default = "default_true")]
    pub subscription_completed: bool,
    
    /// 转存成功推送
    #[serde(default = "default_true")]
    pub transfer_success: bool,
}

fn default_true() -> bool {
    true
}

/// 推送渠道配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushChannels {
    /// Telegram
    #[serde(default)]
    pub telegram: Option<TelegramChannel>,
    
    /// Bark
    #[serde(default)]
    pub bark: Option<BarkChannel>,
    
    /// WxPusher
    #[serde(default)]
    pub wxpusher: Option<WxPusherChannel>,
    
    /// Gotify
    #[serde(default)]
    pub gotify: Option<GotifyChannel>,
    
    /// PushPlus
    #[serde(default)]
    pub pushplus: Option<PushPlusChannel>,
    
    /// Server酱
    #[serde(default)]
    pub serverchan: Option<ServerChanChannel>,
    
    /// 企业微信
    #[serde(default)]
    pub wecom: Option<WecomChannel>,
}

/// Telegram 推送渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChannel {
    pub enabled: bool,
    pub bot_token: String,
    pub chat_id: String,
}

/// Bark 推送渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarkChannel {
    pub enabled: bool,
    pub device_key: String,
    #[serde(default)]
    pub server_url: Option<String>,
}

/// WxPusher 推送渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WxPusherChannel {
    pub enabled: bool,
    pub app_token: String,
    pub topic_ids: Vec<String>,
}

/// Gotify 推送渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotifyChannel {
    pub enabled: bool,
    pub server_url: String,
    pub token: String,
}

/// PushPlus 推送渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushPlusChannel {
    pub enabled: bool,
    pub token: String,
}

/// Server酱 推送渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerChanChannel {
    pub enabled: bool,
    pub send_key: String,
}

/// 企业微信推送渠道
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomChannel {
    pub enabled: bool,
    pub webhook_url: String,
}

/// 自定义分类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCategory {
    /// 分类名称
    pub name: String,
    
    /// 保存目录
    pub dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quark: QuarkSettings {
                cookie: String::new(),
                save_root: String::new(),
                save_root_fid: String::new(),
            },
            push: PushSettings {
                enabled: false,
                silent_mode: false,
                scenarios: PushScenarios {
                    subscription_update: true,
                    subscription_expired: true,
                    subscription_completed: true,
                    transfer_success: true,
                },
                channels: PushChannels {
                    telegram: None,
                    bark: None,
                    wxpusher: None,
                    gotify: None,
                    pushplus: None,
                    serverchan: None,
                    wecom: None,
                },
            },
            custom_categories: Vec::new(),
        }
    }
}
