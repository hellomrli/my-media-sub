#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// 后端权威的资源质量分析结果。
///
/// 所有字段都提供兼容默认值，旧版搜索结果或订阅候选反序列化时不会失败。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceQuality {
    #[serde(default)]
    pub score: u8,
    #[serde(default = "default_grade")]
    pub grade: String,
    #[serde(default = "default_tone")]
    pub tone: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default = "default_resolution")]
    pub resolution: String,
    #[serde(default)]
    pub file_count: usize,
    #[serde(default)]
    pub video_count: usize,
    #[serde(default)]
    pub episode_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_start: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_end: Option<i32>,
    #[serde(default)]
    pub total_size: i64,
    /// RFC3339 时间；无法识别时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub recommendation_reasons: Vec<String>,
}

impl Default for SourceQuality {
    fn default() -> Self {
        Self {
            score: 0,
            grade: default_grade(),
            tone: default_tone(),
            tags: Vec::new(),
            risks: Vec::new(),
            resolution: default_resolution(),
            file_count: 0,
            video_count: 0,
            episode_count: 0,
            episode_start: None,
            episode_end: None,
            total_size: 0,
            updated_at: None,
            recommendation_reasons: Vec::new(),
        }
    }
}

fn default_grade() -> String {
    "谨慎".to_string()
}

fn default_tone() -> String {
    "danger".to_string()
}

fn default_resolution() -> String {
    "未知清晰度".to_string()
}
