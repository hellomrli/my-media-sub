use crate::error::{AppError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DEFAULT_PANSOU_URL: &str = "https://pansou.lxf87.com.cn";

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// 分享链接
    pub url: String,
    /// 提取码
    #[serde(default)]
    pub password: String,
    /// 标题/备注
    pub note: String,
    /// 时间
    #[serde(default)]
    pub datetime: String,
    /// 来源
    pub source: String,
}

/// PanSou API 响应
#[derive(Debug, Deserialize)]
struct PanSouResponse {
    code: i32,
    #[serde(default)]
    data: PanSouData,
}

#[derive(Debug, Default, Deserialize)]
struct PanSouData {
    #[serde(default)]
    merged_by_type: HashMap<String, Vec<SearchResult>>,
}

/// PanSou 搜索客户端
pub struct PanSouClient {
    client: Client,
    base_url: String,
}

impl PanSouClient {
    /// 创建新的 PanSou 客户端
    pub fn new(base_url: Option<String>) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()
            .unwrap();
        
        let base_url = base_url
            .unwrap_or_else(|| DEFAULT_PANSOU_URL.to_string())
            .trim_end_matches('/').to_string();

        Self { client, base_url }
    }

    /// 搜索
    pub async fn search(
        &self,
        keyword: &str,
        cloud_type: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let api_url = format!("{}/api/search", self.base_url);
        
        let mut params = HashMap::new();
        params.insert("kw", keyword);
        params.insert("res", "merge");
        params.insert("src", "all");

        let resp = self.client
            .get(&api_url)
            .query(&params)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| AppError::Http(format!("搜索请求失败: {}", e)))?;

        let data: PanSouResponse = resp.json()
            .await
            .map_err(|e| AppError::Http(format!("解析搜索结果失败: {}", e)))?;

        if data.code != 0 {
            return Ok(Vec::new());
        }

        let results = data.data.merged_by_type
            .get(cloud_type)
            .cloned()
            .unwrap_or_default();

        Ok(results.into_iter().take(limit).collect())
    }

    /// 搜索夸克网盘
    pub async fn search_quark(&self, keyword: &str, limit: usize) -> Result<Vec<SearchResult>> {
        self.search(keyword, "quark", limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_search() {
        let client = PanSouClient::new(None);
        let results = client.search_quark("测试", 10).await.unwrap();
        println!("Found {} results", results.len());
        for result in results.iter().take(3) {
            println!("- {}: {}", result.note, result.url);
        }
    }
}
