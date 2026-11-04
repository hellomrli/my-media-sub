use serde::{Deserialize, Serialize};

/// 一次性保存的资源选择，不产生订阅或定时检查。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MediaSaveOptions {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub preferred_resolution: String,
    /// 分享探测返回的文件 ID；电影用它精确选择正片版本。
    #[serde(default)]
    pub file_ids: Vec<String>,
    #[serde(default)]
    pub target_dir: String,
    #[serde(default)]
    pub sync_download_enabled: bool,
    #[serde(default)]
    pub sync_download_dir: String,
}
