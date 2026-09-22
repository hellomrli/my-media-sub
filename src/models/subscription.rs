use super::rules::TransferRules;
use super::{MediaMetadata, SourceQuality};
use serde::{Deserialize, Serialize};

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
    /// 换源前的起始集数：回滚时恢复，避免「季中补集」的起始集设置
    /// 被换源抬高后永久丢失。旧数据无此字段时为 None（保持不变）。
    #[serde(default)]
    pub previous_start_episode_number: Option<i32>,
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

    /// 起始季度（含）；多季订阅时与 `season_end` 组成闭区间
    #[serde(default = "default_season")]
    pub season: i32,

    /// 结束季度（含）；`None` 或 ≤ `season` 时表示单季
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season_end: Option<i32>,

    /// 跳季订阅的季度集合（如只订 S1+S3 → `[1, 3]`）。
    ///
    /// `Some(非空)` 时优先于 `season..season_end` 区间语义；`season`/`season_end`
    /// 冗余存储为集合的最小/最大值，旧版本程序读到会把跳季当区间处理（多转
    /// 中间季），升级到本版本后恢复精确集合语义。连续集合在规范化时折叠回
    /// 区间（`season_list` 置 `None`），避免冗余。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season_list: Option<Vec<i32>>,

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

    /// 已转存但尚未成功提交到下载器的文件。
    ///
    /// 转存成功会把文件写进 `transferred_file_keys`，之后的检查不会再选中它；
    /// 如果紧接着的 Aria2 提交失败又不留任何记录，这一集就**永远不会下载到本地**，
    /// 而界面仍显示"已转存"。这里记录待提交项（含网盘 fid），由检查流程周期性
    /// 对账重试——下载直链是会过期的，所以只存 fid，重试时再换新直链。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_downloads: Vec<PendingDownload>,

    /// 进行中的转存意图（调用云端**之前**落盘）。
    ///
    /// 存在的意义是让"云端已成功但本地没记住"的重试变成幂等操作：转存既不可撤销
    /// 也不幂等，如果响应丢失或进程在写回本地记录前被杀，`transferred_file_keys`
    /// 就不会被写入，下一次检查会重新选出同一批文件并再次转存，在用户网盘里留下
    /// 重复副本。
    ///
    /// 注意**不能**简单地把"目标目录已存在同名文件"当作已转存的判据：用户网盘里
    /// 本来就可能存在同名文件（手动转存过、或不同来源的同名剧集），那样会静默
    /// 跳过合法转存。只有「有意图记录」+「目标目录出现同名文件」同时成立，
    /// 才能判定上次转存其实成功了。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_transfers: Vec<PendingTransfer>,

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

/// 一次尚未确认结果的转存尝试。
///
/// 落盘时机是**调用云端之前**；确认成功后立即删除。进程在两者之间崩溃、或云端
/// 已成功但 HTTP 响应丢失时，下次检查会看到这条记录，并用「目标目录是否已出现
/// 同名文件」判断云端其实已经成功——从而避免重复转存。
///
/// 反过来也重要：**没有**意图记录时，目标目录里的同名文件一律不构成"已转存"
/// 的证据（用户可能手动转存过，或存在不同来源的同名剧集），必须照常转存。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingTransfer {
    /// 目标季号（多季/跳季订阅按季分别记录）。
    pub season: i32,
    /// 目标目录，仅用于日志与排查；判据是同名文件而非路径。
    pub target_dir: String,
    /// 本次尝试提交的文件名。
    pub file_names: Vec<String>,
    /// 意图落盘时间，用于诊断与过期清理。
    pub created_at: i64,
}

/// 已转存但尚未成功提交下载的文件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingDownload {
    /// 网盘文件 ID。刻意不存下载直链——直链会过期，重试时需重新换取。
    pub fid: String,
    /// 文件名，用于与 Aria2 任务去重。
    pub file_name: String,
    /// 网盘侧目标目录，仅用于展示与排查。
    #[serde(default)]
    pub target_dir: String,
    /// 所属季号。
    #[serde(default)]
    pub season: i32,
    /// Aria2 侧下载目录。
    #[serde(default)]
    pub download_dir: String,
    /// 已尝试提交次数，用于限制重试并暴露长期失败的项。
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub created_at: i64,
}

impl Subscription {
    /// 订阅起始季（至少为 1）
    pub fn season_start(&self) -> i32 {
        self.season.max(1)
    }

    /// 订阅结束季（含）；未设置或小于起始季时等于起始季
    pub fn season_end_inclusive(&self) -> i32 {
        self.season_end
            .unwrap_or_else(|| self.season_start())
            .max(self.season_start())
    }

    /// 是否覆盖多个季度
    pub fn is_multi_season(&self) -> bool {
        self.season_numbers().len() > 1
    }

    /// 订阅覆盖的全部季号（升序、去重）。
    ///
    /// 集合语义（`season_list` 非空）返回集合；否则返回 `season..=season_end`
    /// 连续区间。跳季订阅（如只订 S1+S3）在集合语义下不会放行中间的 S2。
    pub fn season_numbers(&self) -> Vec<i32> {
        if let Some(list) = self.season_list.as_ref().filter(|list| !list.is_empty()) {
            let mut seasons = list.clone();
            seasons.sort_unstable();
            seasons.dedup();
            return seasons;
        }
        (self.season_start()..=self.season_end_inclusive()).collect()
    }

    /// 订阅是否覆盖指定季号。
    pub fn covers_season(&self, season: i32) -> bool {
        match self.season_list.as_ref().filter(|list| !list.is_empty()) {
            Some(list) => list.contains(&season),
            None => {
                let start = self.season_start();
                season >= start && season <= self.season_end_inclusive()
            }
        }
    }

    /// 规范化 season / season_end / season_list 字段。
    ///
    /// 集合列表排序去重并 clamp 到 1–99；恰好构成连续区间的列表折叠为
    /// 区间语义（`season_list = None`），避免冗余存储。
    pub fn normalize_season_range(&mut self) {
        self.season = self.season.max(1);
        if let Some(end) = self.season_end {
            let end = end.max(1);
            if end <= self.season {
                self.season_end = None;
            } else {
                self.season_end = Some(end.min(99));
            }
        }

        if let Some(list) = self.season_list.clone() {
            let (season, season_end, season_list) = normalize_season_list(list);
            self.season = season;
            self.season_end = season_end;
            self.season_list = season_list;
        }
    }

    pub fn season_label(&self) -> String {
        if let Some(list) = self.season_list.as_ref().filter(|list| list.len() > 1) {
            let numbers = list
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            return format!("第 {numbers} 季");
        }
        let start = self.season_start();
        let end = self.season_end_inclusive();
        if end > start {
            format!("第 {start}-{end} 季")
        } else {
            format!("第 {start} 季")
        }
    }

    pub fn status_key(&self) -> &'static str {
        if self.status == "invalid" || self.invalid_since.is_some() {
            "invalid"
        } else if self.status == "completed" || self.completed {
            "completed"
        } else {
            "active"
        }
    }

    pub fn status_label(&self) -> &'static str {
        match self.status_key() {
            "invalid" => "已失效",
            "completed" => "已完结",
            _ => "追更中",
        }
    }

    pub fn progress_total_episodes(&self) -> i32 {
        self.total_episode_number
            .or(self.rules.finish_after_episode)
            .unwrap_or(0)
            .max(0)
    }

    pub fn progress_percent(&self) -> f64 {
        let total = self.progress_total_episodes();
        if total <= 0 {
            return 0.0;
        }
        let current = f64::from(self.current_episode_number.max(0));
        ((current / f64::from(total)) * 100.0).clamp(0.0, 100.0)
    }

    pub fn progress_label(&self) -> String {
        let current = self.current_episode_number.max(0);
        let total = self.progress_total_episodes();
        if total > 0 {
            format!("{current}/{total} 集")
        } else {
            format!("{current}/- 集")
        }
    }
}

/// 规范化创建/更新请求中的季范围
/// 由季度列表构造规范化的 `(season, season_end, season_list)`。
///
/// 排序去重、clamp 到 1–99；单季或连续集合折叠为区间语义
/// （`season_list = None`）；跳季集合保留列表，`season`/`season_end`
/// 冗余存储为最小/最大值保持旧字段可读。
pub fn normalize_season_list(list: Vec<i32>) -> (i32, Option<i32>, Option<Vec<i32>>) {
    let mut list: Vec<i32> = list
        .into_iter()
        .filter(|season| (1..=99).contains(season))
        .collect();
    list.sort_unstable();
    list.dedup();
    if list.is_empty() {
        return (1, None, None);
    }
    let start = list[0];
    let end = list[list.len() - 1];
    let contiguous = list.len() as i64 == (end as i64 - start as i64 + 1);
    if contiguous {
        (start, (end > start).then_some(end), None)
    } else {
        (start, Some(end), Some(list))
    }
}

/// 解析季号输入为完整季号列表（升序去重）：
/// `"1"` → `[1]`，`"1-4"` → `[1,2,3,4]`，`"1,3"` → `[1,3]`，`"1,3-5"` → `[1,3,4,5]`。
pub fn parse_season_spec_list(value: &str) -> Vec<i32> {
    let raw = value.trim();
    if raw.is_empty() {
        return vec![1];
    }

    let mut seasons = Vec::new();
    for part in raw.split([',', '，', ' ', '\t']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut range_parsed = false;
        for sep in ["-", "~", "～", "到", "至"] {
            if let Some((left, right)) = part.split_once(sep) {
                if !left.trim().is_empty() && !right.trim().is_empty() {
                    if let (Some(a), Some(b)) =
                        (parse_positive_season(left), parse_positive_season(right))
                    {
                        let start = a.min(b).clamp(1, 99);
                        let end = a.max(b).clamp(1, 99);
                        seasons.extend(start..=end);
                        range_parsed = true;
                        break;
                    }
                }
            }
        }
        if !range_parsed {
            if let Some(season) = parse_positive_season(part) {
                seasons.push(season.clamp(1, 99));
            }
        }
    }
    seasons.sort_unstable();
    seasons.dedup();
    if seasons.is_empty() {
        vec![1]
    } else {
        seasons
    }
}

pub fn normalize_season_bounds(start: i32, end: Option<i32>) -> (i32, Option<i32>) {
    let start = start.clamp(1, 99);
    let end = end
        .map(|value| value.clamp(1, 99))
        .filter(|value| *value > start);
    (start, end)
}

fn parse_positive_season(value: &str) -> Option<i32> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<i32>().ok().filter(|value| *value > 0)
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
mod season_list_tests {
    use super::*;

    fn sub_with_seasons(
        season: i32,
        season_end: Option<i32>,
        list: Option<Vec<i32>>,
    ) -> Subscription {
        Subscription {
            id: "sub".to_string(),
            title: "Show".to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season,
            season_end,
            season_list: list,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            tags: vec![],
            metadata: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            pending_transfers: Vec::new(),
            pending_downloads: Vec::new(),
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            sync_downloads: vec![],
            enabled: true,
            completed: false,
            rules: TransferRules::default(),
            rule_preset_id: String::new(),
            created_at: 1,
            updated_at: 1,
            last_checked_at: 1,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
            source_candidates: vec![],
            last_source_search_time: None,
            previous_share_links: vec![],
            source_failure_count: 0,
            last_source_switch_at: None,
            source_switch_history: vec![],
        }
    }

    #[test]
    fn season_list_enables_skip_season_selection() {
        // 跳季订阅 [1,3]：S2 被排除，S1/S3 保留；区间语义不变。
        let sub = sub_with_seasons(1, Some(3), Some(vec![1, 3]));
        assert_eq!(sub.season_numbers(), vec![1, 3]);
        assert!(sub.covers_season(1));
        assert!(!sub.covers_season(2));
        assert!(sub.covers_season(3));
        assert!(sub.is_multi_season());
        assert_eq!(sub.season_label(), "第 1,3 季");
        // covers_season 集合语义：S2 不在订阅范围内
        assert!(sub.covers_season(1));
        assert!(!sub.covers_season(2));
        assert!(sub.covers_season(3));
    }

    #[test]
    fn contiguous_season_list_folds_back_to_interval() {
        let mut sub = sub_with_seasons(1, Some(4), Some(vec![1, 2, 3, 4]));
        sub.normalize_season_range();
        assert_eq!(sub.season_list, None);
        assert_eq!((sub.season, sub.season_end), (1, Some(4)));
        assert_eq!(sub.season_numbers(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn parse_season_spec_list_expands_ranges_and_sets() {
        assert_eq!(parse_season_spec_list("1"), vec![1]);
        assert_eq!(parse_season_spec_list("1-4"), vec![1, 2, 3, 4]);
        assert_eq!(parse_season_spec_list("1,3"), vec![1, 3]);
        assert_eq!(parse_season_spec_list("1, 3-5"), vec![1, 3, 4, 5]);
        assert_eq!(parse_season_spec_list("3,1,3"), vec![1, 3]);
        assert_eq!(parse_season_spec_list(""), vec![1]);
        // 倒序区间与中文分隔符（承接已删除的 parse_season_spec 覆盖）
        assert_eq!(parse_season_spec_list("4到1"), vec![1, 2, 3, 4]);
        assert_eq!(parse_season_spec_list("2至4"), vec![2, 3, 4]);
        assert_eq!(parse_season_spec_list("1～3"), vec![1, 2, 3]);
    }

    #[test]
    fn normalize_season_list_normalizes_and_folds() {
        assert_eq!(
            normalize_season_list(vec![3, 1, 3]),
            (1, Some(3), Some(vec![1, 3]))
        );
        assert_eq!(normalize_season_list(vec![2]), (2, None, None));
        assert_eq!(normalize_season_list(vec![1, 2]), (1, Some(2), None));
        assert_eq!(normalize_season_list(vec![]), (1, None, None));
    }
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
            season_end: None,
            season_list: None,
            start_episode_number: Some(5),
            current_episode_number: 12,
            total_episode_number: Some(24),
            source_group: "某字幕组".to_string(),
            tags: vec![],
            metadata: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: "".to_string(),
            known_files: vec!["第01集.mkv".to_string()],
            known_file_keys: vec!["key1".to_string()],
            known_episodes: vec![1, 2, 3],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            pending_transfers: Vec::new(),
            pending_downloads: Vec::new(),
            last_probe: None,
            last_plan_summary: "".to_string(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            sync_downloads: vec![],
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

    #[test]
    fn legacy_manual_schedule_is_ignored_and_not_persisted_again() {
        // 手动排期整体下线（见 v2.2.24 之后的清理）。历史 subscriptions.json 里仍带
        // 这个字段，必须能无害反序列化且不再回写；日历回落到元数据/推断排期。
        let json = r#"{
            "id": "abc123",
            "title": "测试",
            "url": "https://pan.quark.cn/s/test",
            "created_at": 1718236800,
            "updated_at": 1718323200,
            "last_checked_at": 1718323200,
            "manual_schedule": {
                "start_date": "2026-07-06",
                "weekdays": [1, 4],
                "air_time": "20:30",
                "interval_weeks": 1,
                "first_episode_number": 1,
                "total_episodes": 12
            }
        }"#;

        let sub: Subscription = serde_json::from_str(json).unwrap();
        assert_eq!(sub.id, "abc123");

        let serialized = serde_json::to_value(sub).unwrap();
        assert!(serialized.get("manual_schedule").is_none());
    }
}
