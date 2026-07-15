use super::rules::TransferRules;
use super::{MediaMetadata, MediaScheduleOverride, SourceQuality};
use serde::{Deserialize, Serialize};

/// 订阅状态
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionStatus {
    Active,
    Completed,
    Invalid,
}

/// 媒体类型
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Movie,
    Series,
    Anime,
    #[serde(untagged)]
    Custom(String), // custom_* 格式
}

/// 检查历史记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckHistoryItem {
    /// 检查时间
    pub time: i64,

    /// 状态
    pub state: String,

    /// 匹配文件数
    pub matched_count: i32,

    /// 转存文件数
    pub transfer_count: i32,

    /// 扫描文件数
    #[serde(default)]
    pub scanned_count: i32,

    /// 新文件数
    #[serde(default)]
    pub new_count: i32,

    /// 已知文件数
    #[serde(default)]
    pub known_count: i32,

    /// 跳过目录数
    #[serde(default)]
    pub skipped_directory_count: i32,

    /// 跳过非当前季文件数
    #[serde(default)]
    pub skipped_other_season_count: i32,

    /// 跳过起始集数前文件数
    #[serde(default)]
    pub skipped_before_start_count: i32,

    /// 跳过同集重复视频数
    #[serde(default)]
    pub skipped_duplicate_episode_count: i32,

    /// 新文件列表
    pub new_files: Vec<String>,

    /// 新集数列表
    pub new_episodes: Vec<i32>,

    /// 摘要
    pub summary: String,
}

/// 网盘探测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    /// 是否成功
    #[serde(default)]
    pub ok: bool,

    /// 状态
    #[serde(default)]
    pub state: String,

    /// 消息
    #[serde(default)]
    pub message: String,

    /// 文件列表
    #[serde(default)]
    pub files: Vec<ProbeFile>,
}

/// 探测到的文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeFile {
    /// 文件名
    pub name: String,

    /// 是否目录
    #[serde(default)]
    pub is_dir: bool,

    /// 父目录路径（分享内路径，仅用于识别季别和展示）
    #[serde(default)]
    pub parent_path: String,

    /// 文件大小
    #[serde(default)]
    pub size: i64,

    /// 更新时间/上传时间（原始网盘时间字段，可能为空）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,

    /// 文件 key
    #[serde(default)]
    pub file_key: String,
}

/// 换源候选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCandidate {
    /// 候选 ID
    pub id: String,

    /// 来源
    pub source: String,

    /// 分享链接
    pub url: String,

    /// 分享密码
    pub password: String,

    /// 备注信息
    pub note: String,

    /// 发现时间
    pub discovered_at: i64,

    /// 探测信息（可选）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_info: Option<ProbeResult>,

    /// 后端权威资源质量评分；旧候选缺少该字段时使用兼容默认值。
    #[serde(default)]
    pub quality: SourceQuality,
}

/// 单次来源切换或候选失败的审计记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSwitchHistoryItem {
    pub id: String,
    #[serde(default)]
    pub candidate_id: String,
    #[serde(default)]
    pub from_url: String,
    #[serde(default)]
    pub from_password: String,
    #[serde(default)]
    pub to_url: String,
    #[serde(default)]
    pub to_password: String,
    #[serde(default)]
    pub quality: SourceQuality,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub automatic: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rolled_back_at: Option<i64>,
}

/// 订阅提交到 Aria2 的持久化下载记录。
///
/// 该记录属于业务状态，不能只依赖可清理的通知历史。旧数据缺少此字段时由
/// serde 默认成空列表，保持向后兼容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncDownloadRecord {
    pub gid: String,
    pub file_name: String,
    #[serde(default)]
    pub download_dir: String,
    #[serde(default)]
    pub target_dir: String,
    #[serde(default)]
    pub submitted_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

/// 订阅（与 Python JSON 完全兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// 订阅 ID
    pub id: String,

    /// 标题
    pub title: String,

    /// 源标题
    #[serde(default)]
    pub source_title: String,

    /// 媒体类型
    #[serde(default)]
    pub media_type: String,

    /// 季度
    #[serde(default = "default_season")]
    pub season: i32,

    /// 起始转存集数；低于该集数的剧集文件会记为已知但不触发转存
    #[serde(default)]
    pub start_episode_number: Option<i32>,

    /// 当前集数
    #[serde(default)]
    pub current_episode_number: i32,

    /// 总集数（可选）
    #[serde(default)]
    pub total_episode_number: Option<i32>,

    /// 来源组
    #[serde(default)]
    pub source_group: String,

    #[serde(default)]
    pub tags: Vec<String>,

    /// 刮削到的媒体元数据
    #[serde(default)]
    pub metadata: Option<MediaMetadata>,

    /// 手动播出排期；存在时优先于元数据和推断排期。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_schedule: Option<MediaScheduleOverride>,

    /// 云盘类型
    #[serde(default = "default_cloud_type")]
    pub cloud_type: String,

    /// 分享链接
    pub url: String,

    /// 分享密码
    #[serde(default)]
    pub password: String,

    /// 已知文件列表
    #[serde(default)]
    pub known_files: Vec<String>,

    /// 已知文件 key 列表
    #[serde(default)]
    pub known_file_keys: Vec<String>,

    /// 已知集数列表
    #[serde(default)]
    pub known_episodes: Vec<i32>,

    /// 已转存文件列表
    #[serde(default)]
    pub transferred_files: Vec<String>,

    /// 已转存文件 key 列表
    #[serde(default)]
    pub transferred_file_keys: Vec<String>,

    /// 最近一次探测结果
    #[serde(default)]
    pub last_probe: Option<ProbeResult>,

    /// 最近规划摘要
    #[serde(default)]
    pub last_plan_summary: String,

    /// 仅通知（不自动转存）
    #[serde(default)]
    pub notify_only: bool,

    /// 自动转存后同步提交到 Aria2 下载
    #[serde(default)]
    pub sync_download_enabled: bool,

    /// Aria2 同步下载目录；为空时按媒体类型使用系统 Aria2 目录
    #[serde(default)]
    pub sync_download_dir: String,

    /// 已提交到 Aria2 的下载任务及其完成状态。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sync_downloads: Vec<SyncDownloadRecord>,

    /// 自动转存后生成 STRM 文件
    #[serde(default)]
    pub strm_enabled: bool,

    /// 是否启用
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 是否已完成
    #[serde(default)]
    pub completed: bool,

    /// 转存规则
    #[serde(default)]
    pub rules: TransferRules,

    /// 规则预设 ID
    #[serde(default)]
    pub rule_preset_id: String,

    /// 创建时间
    pub created_at: i64,

    /// 更新时间
    pub updated_at: i64,

    /// 最后检查时间
    pub last_checked_at: i64,

    /// 最近新增文件
    #[serde(default)]
    pub last_new_files: Vec<String>,

    /// 最近新增集数
    #[serde(default)]
    pub last_new_episodes: Vec<i32>,

    /// 最近检查摘要
    #[serde(default)]
    pub last_check_summary: String,

    /// 检查历史（最近 30 条）
    #[serde(default)]
    pub check_history: Vec<CheckHistoryItem>,

    /// 状态
    #[serde(default = "default_status")]
    pub status: String,

    /// 失效时间（可选）
    #[serde(default)]
    pub invalid_since: Option<i64>,

    /// 最后错误
    #[serde(default)]
    pub last_error: String,

    /// 规则摘要（视图字段，由 Python 动态生成）
    #[serde(default)]
    pub rule_summary: String,

    /// 换源候选列表（链接失效时自动搜索并填充）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_candidates: Vec<SourceCandidate>,

    /// 上次搜索换源时间
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_source_search_time: Option<i64>,

    /// 历史分享链接（换源时保存旧链接）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_share_links: Vec<String>,

    /// 连续来源失效次数；成功检查后清零。
    #[serde(default)]
    pub source_failure_count: u32,

    /// 最近一次成功换源时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_source_switch_at: Option<i64>,

    /// 换源与候选失败审计历史。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_switch_history: Vec<SourceSwitchHistoryItem>,
}

// 默认值辅助函数
fn default_season() -> i32 {
    1
}

fn default_cloud_type() -> String {
    "quark".to_string()
}

fn default_true() -> bool {
    true
}

fn default_status() -> String {
    "active".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_serialize() {
        let sub = Subscription {
            id: "abc123".to_string(),
            title: "测试动画".to_string(),
            source_title: "【某字幕组】测试动画".to_string(),
            media_type: "anime".to_string(),
            season: 1,
            start_episode_number: Some(5),
            current_episode_number: 12,
            total_episode_number: Some(24),
            source_group: "某字幕组".to_string(),
            tags: vec![],
            metadata: None,
            manual_schedule: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: "".to_string(),
            known_files: vec!["第01集.mkv".to_string()],
            known_file_keys: vec!["key1".to_string()],
            known_episodes: vec![1, 2, 3],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: "".to_string(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            sync_downloads: vec![],
            strm_enabled: false,
            enabled: true,
            completed: false,
            rules: TransferRules::default(),
            rule_preset_id: String::new(),
            created_at: 1718236800,
            updated_at: 1718323200,
            last_checked_at: 1718323200,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: "".to_string(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: "".to_string(),
            rule_summary: "".to_string(),
            source_candidates: vec![],
            last_source_search_time: None,
            previous_share_links: vec![],
            source_failure_count: 0,
            last_source_switch_at: None,
            source_switch_history: vec![],
        };

        let json = serde_json::to_string_pretty(&sub).unwrap();
        println!("{}", json);

        // 验证能反序列化
        let _parsed: Subscription = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_subscription_deserialize_minimal() {
        // 测试最小 JSON（必需字段）
        let json = r#"{
            "id": "abc123",
            "title": "测试",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1718236800,
            "updated_at": 1718323200,
            "last_checked_at": 1718323200
        }"#;

        let sub: Subscription = serde_json::from_str(json).unwrap();
        assert_eq!(sub.id, "abc123");
        assert_eq!(sub.season, 1); // 默认值：第 1 季
        assert_eq!(sub.start_episode_number, None); // 默认值：不限制起始集数
        assert_eq!(sub.cloud_type, "quark"); // 默认值
        assert!(sub.enabled); // 默认值
        assert_eq!(sub.status, "active"); // 默认值
        assert!(sub.metadata.is_none());
    }
}
