use serde::{Deserialize, Serialize};

/// 应用设置（与 Python JSON 完全兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    // ===== 应用认证 =====
    /// 应用用户名
    #[serde(default = "default_username")]
    pub app_username: String,

    /// 应用密码
    #[serde(default = "default_password")]
    pub app_password: String,

    // ===== 搜索配置 =====
    /// 支持的云盘类型
    #[serde(default = "default_cloud_types")]
    pub cloud_types: Vec<String>,

    /// 是否检查链接
    #[serde(default = "default_true")]
    pub check_links: bool,

    /// 是否探测夸克文件
    #[serde(default = "default_true")]
    pub probe_quark_files: bool,

    /// 是否过滤失效链接
    #[serde(default = "default_true")]
    pub filter_bad_links: bool,

    /// PanSou API URL
    #[serde(default)]
    pub pansou_api_url: String,

    // ===== 媒体元数据配置 =====
    /// 元数据提供方：tmdb / douban / none
    #[serde(default = "default_metadata_provider")]
    pub metadata_provider: String,

    /// TMDB API Key
    #[serde(default)]
    pub tmdb_api_key: String,

    /// TMDB 返回语言
    #[serde(default = "default_tmdb_language")]
    pub tmdb_language: String,

    // ===== 夸克网盘配置 =====
    /// 夸克 Cookie
    #[serde(default)]
    pub quark_cookie: String,

    /// 夸克转存是否启用
    #[serde(default)]
    pub quark_save_enabled: bool,

    /// 夸克转存根目录
    #[serde(default)]
    pub quark_save_root: String,

    /// 电影保存目录
    #[serde(default = "default_movie_dir")]
    pub quark_save_movie_dir: String,

    /// 连续剧保存目录
    #[serde(default = "default_series_dir")]
    pub quark_save_series_dir: String,

    /// 动画保存目录
    #[serde(default = "default_anime_dir")]
    pub quark_save_anime_dir: String,

    /// 自定义分类
    #[serde(default)]
    pub custom_categories: Vec<CustomCategory>,

    // ===== 订阅调度 =====
    /// 订阅调度器是否启用
    #[serde(default)]
    pub subscription_scheduler_enabled: bool,

    /// 订阅检查间隔（分钟）
    #[serde(default = "default_check_interval")]
    pub subscription_check_interval_minutes: i32,

    /// 自动下载新订阅项
    #[serde(default)]
    pub auto_download_new_subscription_items: bool,

    // ===== Aria2 配置 =====
    /// Aria2 RPC URL
    #[serde(default)]
    pub aria2_rpc_url: String,

    /// Aria2 密钥
    #[serde(default)]
    pub aria2_secret: String,

    /// Aria2 下载目录
    #[serde(default)]
    pub aria2_dir: String,

    // ===== NAS 同步配置 =====
    /// NAS 同步是否启用
    #[serde(default)]
    pub nas_sync_enabled: bool,

    /// NAS 源目录
    #[serde(default)]
    pub nas_sync_source: String,

    /// NAS 目标目录
    #[serde(default)]
    pub nas_sync_target: String,

    // ===== 推送配置 =====
    /// Telegram Bot Token
    #[serde(default)]
    pub telegram_bot_token: String,

    /// Telegram Chat ID
    #[serde(default)]
    pub telegram_chat_id: String,

    /// Bark URL
    #[serde(default)]
    pub bark_url: String,

    /// Gotify URL
    #[serde(default)]
    pub gotify_url: String,

    /// Gotify Token
    #[serde(default)]
    pub gotify_token: String,

    /// PushPlus Token
    #[serde(default)]
    pub pushplus_token: String,

    /// Server酱 Key
    #[serde(default)]
    pub serverchan_key: String,

    /// 企业微信 Bot URL
    #[serde(default)]
    pub wecom_bot_url: String,

    /// WxPusher App Token
    #[serde(default)]
    pub wxpusher_app_token: String,

    /// WxPusher UIDs
    #[serde(default)]
    pub wxpusher_uids: String,

    // ===== 推送场景开关 =====
    /// 订阅更新时推送
    #[serde(default = "default_true")]
    pub push_on_update: bool,

    /// 订阅失败时推送
    #[serde(default = "default_true")]
    pub push_on_failed: bool,

    /// 订阅完成时推送
    #[serde(default = "default_true")]
    pub push_on_completed: bool,

    /// 转存时推送
    #[serde(default = "default_true")]
    pub push_on_save: bool,

    /// 静默推送
    #[serde(default)]
    pub push_silent: bool,
}

/// 自定义分类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCategory {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub dir: String,
}

// 默认值函数
fn default_username() -> String {
    "admin".to_string()
}

fn default_password() -> String {
    "change-me".to_string()
}

fn default_cloud_types() -> Vec<String> {
    vec!["quark".to_string()]
}

fn default_true() -> bool {
    true
}

fn default_movie_dir() -> String {
    "/电影".to_string()
}

fn default_series_dir() -> String {
    "/连续剧".to_string()
}

fn default_anime_dir() -> String {
    "/动画".to_string()
}

fn default_check_interval() -> i32 {
    60
}

fn default_metadata_provider() -> String {
    "tmdb".to_string()
}

fn default_tmdb_language() -> String {
    "zh-CN".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            app_username: default_username(),
            app_password: default_password(),
            cloud_types: default_cloud_types(),
            check_links: true,
            probe_quark_files: true,
            filter_bad_links: true,
            pansou_api_url: String::new(),
            metadata_provider: default_metadata_provider(),
            tmdb_api_key: String::new(),
            tmdb_language: default_tmdb_language(),
            quark_cookie: String::new(),
            quark_save_enabled: false,
            quark_save_root: String::new(),
            quark_save_movie_dir: default_movie_dir(),
            quark_save_series_dir: default_series_dir(),
            quark_save_anime_dir: default_anime_dir(),
            custom_categories: vec![],
            subscription_scheduler_enabled: false,
            subscription_check_interval_minutes: default_check_interval(),
            auto_download_new_subscription_items: false,
            aria2_rpc_url: String::new(),
            aria2_secret: String::new(),
            aria2_dir: String::new(),
            nas_sync_enabled: false,
            nas_sync_source: String::new(),
            nas_sync_target: String::new(),
            telegram_bot_token: String::new(),
            telegram_chat_id: String::new(),
            bark_url: String::new(),
            gotify_url: String::new(),
            gotify_token: String::new(),
            pushplus_token: String::new(),
            serverchan_key: String::new(),
            wecom_bot_url: String::new(),
            wxpusher_app_token: String::new(),
            wxpusher_uids: String::new(),
            push_on_update: true,
            push_on_failed: true,
            push_on_completed: true,
            push_on_save: true,
            push_silent: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert_eq!(settings.app_username, "admin");
        assert_eq!(settings.cloud_types, vec!["quark"]);
        assert_eq!(settings.check_links, true);
        assert_eq!(settings.metadata_provider, "tmdb");
        assert_eq!(settings.tmdb_language, "zh-CN");
    }

    #[test]
    fn test_settings_serialize() {
        let settings = Settings::default();
        let json = serde_json::to_string_pretty(&settings).unwrap();
        println!("{}", json);

        // 验证能反序列化
        let _parsed: Settings = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_settings_deserialize_partial() {
        // 测试部分 JSON（其他用默认值）
        let json = r#"{
            "app_username": "test",
            "quark_cookie": "test_cookie"
        }"#;

        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.app_username, "test");
        assert_eq!(settings.quark_cookie, "test_cookie");
        assert_eq!(settings.app_password, "change-me"); // 默认值
        assert_eq!(settings.check_links, true); // 默认值
    }
}
