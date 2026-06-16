#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// 标题
    pub title: String,

    /// 分享链接
    pub url: String,

    /// 分享密码
    #[serde(default)]
    pub password: String,

    /// 来源
    #[serde(default)]
    pub source: String,

    /// 云盘类型
    #[serde(default = "default_cloud_type")]
    pub cloud_type: String,

    /// 探测结果（可选）
    #[serde(default)]
    pub probe: Option<ProbeResult>,
}

/// 探测结果
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

    /// 文件大小
    #[serde(default)]
    pub size: i64,

    /// 文件 key
    #[serde(default)]
    pub file_key: String,

    /// 其他字段（用于扩展）
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 搜索会话（内存态）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSession {
    /// 会话 ID（通常是 chat_id）
    pub chat_id: String,

    /// 搜索关键词
    pub keyword: String,

    /// 搜索结果列表
    pub results: Vec<SearchResult>,

    /// 创建时间
    pub created_at: i64,
}

fn default_cloud_type() -> String {
    "quark".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_serialize() {
        let result = SearchResult {
            title: "测试资源".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: "".to_string(),
            source: "某字幕组".to_string(),
            cloud_type: "quark".to_string(),
            probe: None,
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        println!("{}", json);

        // 验证能反序列化
        let _parsed: SearchResult = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_search_session() {
        let session = SearchSession {
            chat_id: "web-session-123".to_string(),
            keyword: "测试关键词".to_string(),
            results: vec![SearchResult {
                title: "资源1".to_string(),
                url: "https://pan.quark.cn/s/1".to_string(),
                password: "".to_string(),
                source: "来源1".to_string(),
                cloud_type: "quark".to_string(),
                probe: None,
            }],
            created_at: 1718323200,
        };

        let json = serde_json::to_string_pretty(&session).unwrap();
        println!("{}", json);

        let parsed: SearchSession = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.results.len(), 1);
    }
}
