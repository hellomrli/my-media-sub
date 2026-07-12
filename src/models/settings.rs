use super::rules::TransferRules;
use serde::{Deserialize, Serialize};

pub const MIN_SUBSCRIPTION_CHECK_INTERVAL_MINUTES: i32 = 5;

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

    /// 是否信任反向代理转发头（X-Forwarded-For）。
    /// 仅当部署在可信反向代理之后时开启；默认 false，防止客户端伪造
    /// X-Forwarded-For 绕过登录失败锁定。
    #[serde(default)]
    pub trust_proxy_headers: bool,

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

    /// 夸克签到 Cookie（可使用移动端 Cookie；留空时回退到 quark_cookie）
    #[serde(default)]
    pub quark_signin_cookie: String,

    /// 夸克转存是否启用
    #[serde(default)]
    pub quark_save_enabled: bool,

    /// 夸克自动签到是否启用
    #[serde(default)]
    pub quark_signin_enabled: bool,

    /// 夸克自动签到小时（0-23）
    #[serde(default = "default_quark_signin_hour")]
    pub quark_signin_hour: i32,

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

    #[serde(default = "default_dashboard_widgets")]
    pub dashboard_widgets: Vec<String>,

    // ===== 订阅调度 =====
    /// 订阅调度器是否启用
    #[serde(default)]
    pub subscription_scheduler_enabled: bool,

    /// 订阅检查间隔（分钟）
    #[serde(default = "default_check_interval")]
    pub subscription_check_interval_minutes: i32,

    /// 批量订阅检查最大并发数。
    #[serde(default = "default_subscription_check_max_concurrency")]
    pub subscription_check_max_concurrency: usize,

    /// 共享外部 API 最大并发数。
    #[serde(default = "default_external_api_max_concurrency")]
    pub external_api_max_concurrency: usize,

    /// 后台 Job 全局最大并发数。
    #[serde(default = "default_job_max_concurrency")]
    pub job_max_concurrency: usize,

    /// 转存类 Job 最大并发数（手动和订阅转存共享）。
    #[serde(default = "default_job_transfer_max_concurrency")]
    pub job_transfer_max_concurrency: usize,

    /// 元数据 Job 最大并发数。
    #[serde(default = "default_job_metadata_max_concurrency")]
    pub job_metadata_max_concurrency: usize,

    /// 推送 Job 最大并发数。
    #[serde(default = "default_job_push_max_concurrency")]
    pub job_push_max_concurrency: usize,

    /// 维护模式下暂停认领新 Job，已运行任务继续收尾。
    #[serde(default)]
    pub job_maintenance_mode: bool,

    /// 单次提交到 Aria2 的最大文件数。
    #[serde(default = "default_aria2_batch_submit_limit")]
    pub aria2_batch_submit_limit: usize,

    /// 自动下载新订阅项
    #[serde(default)]
    pub auto_download_new_subscription_items: bool,

    /// 是否允许自动应用换源。默认关闭；关闭时仍可按策略仅搜索候选。
    #[serde(default)]
    pub auto_source_switch_enabled: bool,

    /// search_only / apply，默认 search_only。
    #[serde(default = "default_source_switch_mode")]
    pub auto_source_switch_mode: String,

    /// 自动应用候选的最低质量分。
    #[serde(default = "default_source_switch_min_score")]
    pub source_switch_min_score: i32,

    /// 候选相对当前来源的最低分差。
    #[serde(default = "default_source_switch_min_score_delta")]
    pub source_switch_min_score_delta: i32,

    /// 连续失效多少次后才允许自动应用。
    #[serde(default = "default_source_switch_failure_threshold")]
    pub source_switch_failure_threshold: i32,

    /// 搜索、应用和失败候选的冷却时间。
    #[serde(default = "default_source_switch_cooldown_hours")]
    pub source_switch_cooldown_hours: i32,

    /// 连续剧/动画新订阅默认重命名模板；为空时使用内置默认模板
    #[serde(default)]
    pub default_rename_template: String,

    /// 订阅规则预设
    #[serde(default = "default_rule_presets")]
    pub rule_presets: Vec<RulePreset>,

    // ===== Aria2 配置 =====
    /// Aria2 RPC URL
    #[serde(default)]
    pub aria2_rpc_url: String,

    /// Aria2 密钥
    #[serde(default)]
    pub aria2_secret: String,

    /// Aria2 电影下载目录
    #[serde(default)]
    pub aria2_movie_dir: String,

    /// Aria2 连续剧下载目录
    #[serde(default)]
    pub aria2_series_dir: String,

    /// Aria2 动画下载目录
    #[serde(default)]
    pub aria2_anime_dir: String,

    // ===== STRM 配置 =====
    /// 是否启用 STRM 文件生成
    #[serde(default)]
    pub strm_enabled: bool,

    /// 本地 STRM 文件输出根目录
    #[serde(default)]
    pub strm_output_dir: String,

    /// HTTPStrm 对外访问根地址
    #[serde(default)]
    pub strm_public_base_url: String,

    /// HTTPStrm 访问 Token
    #[serde(default = "default_strm_access_token")]
    pub strm_access_token: String,

    /// 是否把 HTTPStrm Token 写入生成的 URL query。默认关闭，避免 token 进入访问日志。
    #[serde(default)]
    pub strm_token_in_url: bool,

    #[serde(default)]
    pub media_library_refresh_enabled: bool,
    #[serde(default = "default_media_library_type")]
    pub media_library_type: String,
    #[serde(default)]
    pub media_library_refresh_url: String,
    #[serde(default)]
    pub media_library_token: String,

    // ===== 推送配置 =====
    /// Telegram Bot Token
    #[serde(default)]
    pub telegram_bot_token: String,

    /// Telegram Chat ID（推送目标；也可作为 Bot 允许 chat ID 的兼容回退）。
    #[serde(default)]
    pub telegram_chat_id: String,

    /// Telegram 主动控制模式：disabled / long_polling / webhook。
    #[serde(default = "default_telegram_bot_mode")]
    pub telegram_bot_mode: String,

    /// 允许执行 Bot 命令的 Telegram 数字 user ID。
    #[serde(default)]
    pub telegram_bot_allowed_user_ids: Vec<i64>,

    /// 允许执行 Bot 命令的 Telegram 数字 chat ID。
    #[serde(default)]
    pub telegram_bot_allowed_chat_ids: Vec<i64>,

    /// 默认只允许 private chat，群组即使 ID 在白名单中也会拒绝。
    #[serde(default = "default_true")]
    pub telegram_bot_private_only: bool,

    /// Webhook 对外 HTTPS 根地址，不包含 Bot 的随机路径。
    #[serde(default)]
    pub telegram_bot_webhook_public_url: String,

    /// Webhook URL 随机路径片段。
    #[serde(default)]
    pub telegram_bot_webhook_path_secret: String,

    /// Telegram X-Telegram-Bot-Api-Secret-Token 校验值。
    #[serde(default)]
    pub telegram_bot_webhook_secret: String,

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

    /// Browser Push VAPID private key (PKCS#8 DER, base64url).
    #[serde(default)]
    pub browser_push_vapid_private_key: String,
    #[serde(default)]
    pub browser_push_vapid_public_key: String,
    #[serde(default = "default_browser_push_subject")]
    pub browser_push_subject: String,
    #[serde(default)]
    pub browser_push_subscriptions: Vec<BrowserPushSubscription>,
    #[serde(default)]
    pub webhook_enabled: bool,
    #[serde(default)]
    pub webhook_urls: Vec<String>,
    #[serde(default)]
    pub webhook_secret: String,

    /// Webhook 签名轮换期间保留的上一密钥。
    #[serde(default)]
    pub webhook_previous_secret: String,

    #[serde(default)]
    pub webhook_previous_secret_expires_at: i64,

    /// 事件到渠道 ID 的显式路由；缺失事件回退到全部已配置渠道。
    #[serde(default)]
    pub push_event_routes: std::collections::HashMap<String, Vec<String>>,

    #[serde(default = "default_push_min_level")]
    pub push_min_level: String,

    #[serde(default)]
    pub push_quiet_hours_enabled: bool,
    #[serde(default = "default_push_quiet_start_hour")]
    pub push_quiet_start_hour: u8,
    #[serde(default = "default_push_quiet_end_hour")]
    pub push_quiet_end_hour: u8,
    #[serde(default = "default_true")]
    pub push_quiet_allow_error: bool,

    #[serde(default = "default_push_dedup_window_seconds")]
    pub push_dedup_window_seconds: i64,
    #[serde(default)]
    pub push_digest_enabled: bool,
    #[serde(default = "default_push_digest_window_minutes")]
    pub push_digest_window_minutes: i64,

    #[serde(default = "default_push_title_template")]
    pub push_title_template: String,
    #[serde(default = "default_push_message_template")]
    pub push_message_template: String,

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

    /// 下载完成时推送
    #[serde(default = "default_true")]
    pub push_on_download_completed: bool,

    /// 夸克签到成功时推送
    #[serde(default = "default_true")]
    pub push_on_quark_signin: bool,

    /// 静默推送
    #[serde(default)]
    pub push_silent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserPushSubscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub created_at: i64,
}

fn default_browser_push_subject() -> String {
    "mailto:admin@localhost".to_string()
}
fn default_push_min_level() -> String {
    "info".to_string()
}
fn default_push_quiet_start_hour() -> u8 {
    23
}
fn default_push_quiet_end_hour() -> u8 {
    8
}
fn default_push_dedup_window_seconds() -> i64 {
    300
}
fn default_push_digest_window_minutes() -> i64 {
    15
}
fn default_push_title_template() -> String {
    "{{title}}".to_string()
}
fn default_push_message_template() -> String {
    "{{message}}".to_string()
}

/// 自定义分类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCategory {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub aria2_dir: String,
}

/// 订阅规则预设
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePreset {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub rules: TransferRules,
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

fn default_source_switch_mode() -> String {
    "search_only".to_string()
}

fn default_source_switch_min_score() -> i32 {
    70
}

fn default_source_switch_min_score_delta() -> i32 {
    10
}

fn default_source_switch_failure_threshold() -> i32 {
    2
}

fn default_source_switch_cooldown_hours() -> i32 {
    24
}

fn default_check_interval() -> i32 {
    60
}

fn default_rule_presets() -> Vec<RulePreset> {
    let standard = TransferRules {
        rename_template: "{title}.S{season}E{episode}.{ext}".to_string(),
        ..Default::default()
    };

    let episode_only = TransferRules {
        rename_template: "{episode}.{ext}".to_string(),
        ..Default::default()
    };

    let original_keep = TransferRules {
        rename_template: "{original}.{ext}".to_string(),
        duplicate_episode_strategy: "latest_upload".to_string(),
        ..Default::default()
    };

    let movie_title = TransferRules {
        rename_template: "{title}.{ext}".to_string(),
        duplicate_episode_strategy: "largest_size".to_string(),
        exclude_keywords: vec![
            "预告".to_string(),
            "花絮".to_string(),
            "解说".to_string(),
            "彩蛋".to_string(),
            "trailer".to_string(),
            "preview".to_string(),
            "sample".to_string(),
        ],
        ..Default::default()
    };

    vec![
        RulePreset {
            id: "standard_tv".to_string(),
            name: "标准剧集".to_string(),
            description: "S01E01 风格，适合电视剧和动画".to_string(),
            media_type: "series".to_string(),
            rules: standard,
        },
        RulePreset {
            id: "episode_only".to_string(),
            name: "仅集数".to_string(),
            description: "生成 01.mp4 / 02.mkv，适合短目录".to_string(),
            media_type: "series".to_string(),
            rules: episode_only,
        },
        RulePreset {
            id: "original_keep".to_string(),
            name: "保留原名".to_string(),
            description: "尽量不改文件名，只做过滤和去重".to_string(),
            media_type: "series".to_string(),
            rules: original_keep,
        },
        RulePreset {
            id: "movie_title".to_string(),
            name: "电影标题".to_string(),
            description: "电影直接使用标题和扩展名".to_string(),
            media_type: "movie".to_string(),
            rules: movie_title,
        },
    ]
}

fn default_subscription_check_max_concurrency() -> usize {
    4
}
fn default_external_api_max_concurrency() -> usize {
    8
}
fn default_job_max_concurrency() -> usize {
    4
}
fn default_job_transfer_max_concurrency() -> usize {
    2
}
fn default_job_metadata_max_concurrency() -> usize {
    2
}
fn default_job_push_max_concurrency() -> usize {
    4
}
fn default_aria2_batch_submit_limit() -> usize {
    20
}

pub fn normalize_subscription_check_max_concurrency(value: i64) -> usize {
    value.clamp(1, 32) as usize
}
pub fn normalize_external_api_max_concurrency(value: i64) -> usize {
    value.clamp(1, 64) as usize
}
pub fn normalize_job_max_concurrency(value: i64) -> usize {
    value.clamp(1, 32) as usize
}
pub fn normalize_job_class_max_concurrency(value: i64) -> usize {
    value.clamp(1, 32) as usize
}
pub fn normalize_aria2_batch_submit_limit(value: i64) -> usize {
    value.clamp(1, 100) as usize
}

pub fn normalize_check_interval_minutes(minutes: i64) -> i32 {
    minutes.clamp(
        MIN_SUBSCRIPTION_CHECK_INTERVAL_MINUTES as i64,
        i32::MAX as i64,
    ) as i32
}

fn default_quark_signin_hour() -> i32 {
    8
}

fn default_metadata_provider() -> String {
    "tmdb".to_string()
}

fn default_tmdb_language() -> String {
    "zh-CN".to_string()
}

fn default_dashboard_widgets() -> Vec<String> {
    ["quick_actions", "hero", "kpis", "library", "operations"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn default_strm_access_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_media_library_type() -> String {
    "webhook".to_string()
}

fn default_telegram_bot_mode() -> String {
    "disabled".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            app_username: default_username(),
            app_password: default_password(),
            trust_proxy_headers: false,
            cloud_types: default_cloud_types(),
            check_links: true,
            probe_quark_files: true,
            filter_bad_links: true,
            pansou_api_url: String::new(),
            metadata_provider: default_metadata_provider(),
            tmdb_api_key: String::new(),
            tmdb_language: default_tmdb_language(),
            quark_cookie: String::new(),
            quark_signin_cookie: String::new(),
            quark_save_enabled: false,
            quark_signin_enabled: false,
            quark_signin_hour: default_quark_signin_hour(),
            quark_save_root: String::new(),
            quark_save_movie_dir: default_movie_dir(),
            quark_save_series_dir: default_series_dir(),
            quark_save_anime_dir: default_anime_dir(),
            custom_categories: vec![],
            dashboard_widgets: default_dashboard_widgets(),
            subscription_scheduler_enabled: false,
            subscription_check_interval_minutes: default_check_interval(),
            subscription_check_max_concurrency: default_subscription_check_max_concurrency(),
            external_api_max_concurrency: default_external_api_max_concurrency(),
            job_max_concurrency: default_job_max_concurrency(),
            job_transfer_max_concurrency: default_job_transfer_max_concurrency(),
            job_metadata_max_concurrency: default_job_metadata_max_concurrency(),
            job_push_max_concurrency: default_job_push_max_concurrency(),
            job_maintenance_mode: false,
            aria2_batch_submit_limit: default_aria2_batch_submit_limit(),
            auto_download_new_subscription_items: false,
            auto_source_switch_enabled: false,
            auto_source_switch_mode: default_source_switch_mode(),
            source_switch_min_score: default_source_switch_min_score(),
            source_switch_min_score_delta: default_source_switch_min_score_delta(),
            source_switch_failure_threshold: default_source_switch_failure_threshold(),
            source_switch_cooldown_hours: default_source_switch_cooldown_hours(),
            default_rename_template: String::new(),
            rule_presets: default_rule_presets(),
            aria2_rpc_url: String::new(),
            aria2_secret: String::new(),
            aria2_movie_dir: String::new(),
            aria2_series_dir: String::new(),
            aria2_anime_dir: String::new(),
            strm_enabled: false,
            strm_output_dir: String::new(),
            strm_public_base_url: String::new(),
            strm_access_token: default_strm_access_token(),
            strm_token_in_url: false,
            media_library_refresh_enabled: false,
            media_library_type: default_media_library_type(),
            media_library_refresh_url: String::new(),
            media_library_token: String::new(),
            telegram_bot_token: String::new(),
            telegram_chat_id: String::new(),
            telegram_bot_mode: default_telegram_bot_mode(),
            telegram_bot_allowed_user_ids: vec![],
            telegram_bot_allowed_chat_ids: vec![],
            telegram_bot_private_only: true,
            telegram_bot_webhook_public_url: String::new(),
            telegram_bot_webhook_path_secret: String::new(),
            telegram_bot_webhook_secret: String::new(),
            bark_url: String::new(),
            gotify_url: String::new(),
            gotify_token: String::new(),
            pushplus_token: String::new(),
            serverchan_key: String::new(),
            wecom_bot_url: String::new(),
            wxpusher_app_token: String::new(),
            wxpusher_uids: String::new(),
            browser_push_vapid_private_key: String::new(),
            browser_push_vapid_public_key: String::new(),
            browser_push_subject: default_browser_push_subject(),
            browser_push_subscriptions: vec![],
            webhook_enabled: false,
            webhook_urls: vec![],
            webhook_secret: String::new(),
            webhook_previous_secret: String::new(),
            webhook_previous_secret_expires_at: 0,
            push_event_routes: std::collections::HashMap::new(),
            push_min_level: default_push_min_level(),
            push_quiet_hours_enabled: false,
            push_quiet_start_hour: default_push_quiet_start_hour(),
            push_quiet_end_hour: default_push_quiet_end_hour(),
            push_quiet_allow_error: true,
            push_dedup_window_seconds: default_push_dedup_window_seconds(),
            push_digest_enabled: false,
            push_digest_window_minutes: default_push_digest_window_minutes(),
            push_title_template: default_push_title_template(),
            push_message_template: default_push_message_template(),
            push_on_update: true,
            push_on_failed: true,
            push_on_completed: true,
            push_on_save: true,
            push_on_download_completed: true,
            push_on_quark_signin: true,
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
        assert!(settings.check_links);
        assert_eq!(settings.metadata_provider, "tmdb");
        assert_eq!(settings.tmdb_language, "zh-CN");
        assert!(settings.quark_signin_cookie.is_empty());
        assert!(settings.push_on_download_completed);
        assert!(settings.push_on_quark_signin);
        assert_eq!(settings.quark_signin_hour, 8);
        assert!(settings.default_rename_template.is_empty());
        assert!(!settings.rule_presets.is_empty());
        assert_eq!(settings.job_max_concurrency, 4);
        assert_eq!(settings.job_transfer_max_concurrency, 2);
    }

    #[test]
    fn test_normalize_check_interval_minutes() {
        assert_eq!(normalize_check_interval_minutes(-1), 5);
        assert_eq!(normalize_check_interval_minutes(0), 5);
        assert_eq!(normalize_check_interval_minutes(5), 5);
        assert_eq!(normalize_check_interval_minutes(15), 15);
        assert_eq!(normalize_check_interval_minutes(30), 30);
        assert_eq!(normalize_check_interval_minutes(60), 60);
        assert_eq!(normalize_check_interval_minutes(120), 120);
        assert_eq!(normalize_check_interval_minutes(360), 360);
        assert_eq!(normalize_check_interval_minutes(720), 720);
        assert_eq!(normalize_check_interval_minutes(i64::MAX), i32::MAX);
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
        assert!(!settings.trust_proxy_headers); // 默认不信任代理头
        assert!(settings.check_links); // 默认值
    }

    #[test]
    fn trust_proxy_headers_defaults_false_and_deserializes() {
        assert!(!Settings::default().trust_proxy_headers);
        let settings: Settings = serde_json::from_str(r#"{"trust_proxy_headers": true}"#).unwrap();
        assert!(settings.trust_proxy_headers);
    }

    #[test]
    fn legacy_unused_nas_fields_are_ignored_and_not_persisted_again() {
        let json = r#"{
            "app_username": "legacy",
            "nas_sync_enabled": true,
            "nas_sync_source": "/old/source",
            "nas_sync_target": "/old/target"
        }"#;

        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.app_username, "legacy");

        let serialized = serde_json::to_value(settings).unwrap();
        assert!(serialized.get("nas_sync_enabled").is_none());
        assert!(serialized.get("nas_sync_source").is_none());
        assert!(serialized.get("nas_sync_target").is_none());
    }
}
