use serde::{Deserialize, Serialize};

/// 转存规则（与 Python 完整兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRules {
    /// 目标目录
    #[serde(default)]
    pub target_dir: String,

    /// 自动创建目标目录
    #[serde(default = "default_true")]
    pub auto_create_target_dir: bool,

    /// 跳过已转存的文件
    #[serde(default = "default_true")]
    pub skip_existing_transferred: bool,

    /// 同一集出现多个视频时的保留策略：highest_quality / latest_upload / largest_size / first
    #[serde(default = "default_duplicate_episode_strategy")]
    pub duplicate_episode_strategy: String,

    /// 包含关键词
    #[serde(default)]
    pub include_keywords: Vec<String>,

    /// 排除关键词
    #[serde(default = "default_excludes")]
    pub exclude_keywords: Vec<String>,

    /// 匹配正则
    #[serde(default)]
    pub match_regex: String,

    /// 忽略扩展名
    #[serde(default)]
    pub ignore_extensions: bool,

    /// 重命名正则
    #[serde(default)]
    pub rename_regex: String,

    /// 重命名替换
    #[serde(default)]
    pub rename_replacement: String,

    /// 重命名模板
    #[serde(default)]
    pub rename_template: String,

    /// 仅处理最新一集
    #[serde(default)]
    pub only_latest: bool,

    /// 更新时通知
    #[serde(default = "default_true")]
    pub notify_on_update: bool,

    /// 失效时通知
    #[serde(default = "default_true")]
    pub notify_on_invalid: bool,

    /// 检查间隔（分钟）
    #[serde(default = "default_check_interval")]
    pub check_interval_minutes: i32,

    /// 检查星期（可选）
    #[serde(default)]
    pub check_weekdays: Vec<i32>,

    /// 完成于第几集（可选）
    #[serde(default)]
    pub finish_after_episode: Option<i32>,
}

fn default_true() -> bool {
    true
}

fn default_duplicate_episode_strategy() -> String {
    "highest_quality".to_string()
}

fn default_excludes() -> Vec<String> {
    vec![
        "预告".to_string(),
        "花絮".to_string(),
        "解说".to_string(),
        "彩蛋".to_string(),
        "trailer".to_string(),
        "preview".to_string(),
    ]
}

fn default_check_interval() -> i32 {
    60
}

impl Default for TransferRules {
    fn default() -> Self {
        Self {
            target_dir: String::new(),
            auto_create_target_dir: true,
            skip_existing_transferred: true,
            duplicate_episode_strategy: default_duplicate_episode_strategy(),
            include_keywords: vec![],
            exclude_keywords: default_excludes(),
            match_regex: String::new(),
            ignore_extensions: false,
            rename_regex: String::new(),
            rename_replacement: String::new(),
            rename_template: String::new(),
            only_latest: false,
            notify_on_update: true,
            notify_on_invalid: true,
            check_interval_minutes: 60,
            check_weekdays: vec![],
            finish_after_episode: None,
        }
    }
}
