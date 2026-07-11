use super::http_pool;
use crate::error::{AppError, Result};
use crate::models::SourceQuality;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const DEFAULT_PANSOU_URL: &str = "https://pansou.lxf87.com.cn";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// PanSou API 响应
#[derive(Debug, Deserialize)]
struct PanSouResponse {
    code: i32,
    data: Option<PanSouData>,
}

#[derive(Debug, Deserialize)]
struct PanSouData {
    merged_by_type: Option<MergedByType>,
}

#[derive(Debug, Deserialize)]
struct MergedByType {
    quark: Option<Vec<PanSouItem>>,
}

#[derive(Debug, Deserialize)]
struct PanSouItem {
    url: String,
    password: Option<String>,
    note: Option<String>,
    source: Option<String>,
    datetime: Option<String>,
    images: Option<Vec<String>>,
}

/// 搜索结果（统一格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub unique_id: String,
    pub note: String,
    pub url: String,
    pub password: String,
    pub source: String,
    pub datetime: String,
    pub images: Vec<String>,
    pub cloud_type: String,
    /// 探测到的文件列表信息（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe_info: Option<crate::clients::quark::QuarkShareInfo>,
    /// 链接是否有效（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_valid: Option<bool>,
    /// 后端权威资源质量评分。
    #[serde(default)]
    pub quality: SourceQuality,
}

/// Remote PanSou 客户端
pub struct PanSouClient {
    base_url: String,
    client: Client,
}

impl PanSouClient {
    pub fn new(base_url: Option<String>) -> Self {
        let client = http_pool::short_client();

        Self {
            base_url: base_url.unwrap_or_else(|| DEFAULT_PANSOU_URL.to_string()),
            client,
        }
    }

    /// 搜索资源（仅支持夸克）
    pub async fn search(
        &self,
        keyword: &str,
        cloud_types: &[String],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        // 只支持夸克
        if !cloud_types.contains(&"quark".to_string()) {
            return Ok(vec![]);
        }

        let api_url = format!("{}/api/search", self.base_url);
        let mut last_error = String::new();
        let mut response = None;
        for attempt in 0..3u32 {
            match self
                .client
                .get(&api_url)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .query(&[("kw", keyword), ("res", "merge"), ("src", "all")])
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    response = Some(resp);
                    break;
                }
                Ok(resp) => {
                    last_error = format!("PanSou 返回错误: {}", resp.status());
                    if !resp.status().is_server_error() && resp.status().as_u16() != 429 {
                        break;
                    }
                }
                Err(error) => last_error = format!("PanSou 请求失败: {error}"),
            }
            if attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(200 * (1u64 << attempt))).await;
            }
        }
        let resp = response.ok_or_else(|| AppError::Http(last_error))?;

        let data: PanSouResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析 PanSou 响应失败: {}", e)))?;

        if data.code != 0 {
            return Ok(vec![]);
        }

        let quark_results = data
            .data
            .and_then(|d| d.merged_by_type)
            .and_then(|m| m.quark)
            .unwrap_or_default();

        let mut results = Vec::new();
        for item in quark_results {
            let url = item.url;
            let unique_id = format!("pansou:{}", md5_short(&url));
            results.push(SearchResult {
                unique_id,
                note: item.note.unwrap_or_default(),
                url: url.clone(),
                password: item.password.unwrap_or_default(),
                source: item.source.unwrap_or_else(|| "pansou".to_string()),
                datetime: item
                    .datetime
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                images: item.images.unwrap_or_default(),
                cloud_type: "quark".to_string(),
                probe_info: None,
                is_valid: None,
                quality: SourceQuality::default(),
            });

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }
}

impl Default for PanSouClient {
    fn default() -> Self {
        Self::new(None)
    }
}

/// 简单的 MD5 摘要（前 12 位）
fn md5_short(s: &str) -> String {
    format!("{:x}", md5::compute(s.as_bytes()))
        .chars()
        .take(12)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md5_short() {
        let result = md5_short("https://pan.quark.cn/s/test");
        assert_eq!(result.len(), 12);
    }

    #[tokio::test]
    async fn test_pansou_client_creation() {
        let client = PanSouClient::new(None);
        assert_eq!(client.base_url, DEFAULT_PANSOU_URL);
    }

    // 真实 API 测试（需要网络）
    #[tokio::test]
    #[ignore] // 默认跳过，手动运行: cargo test -- --ignored
    async fn test_pansou_search_real() {
        let client = PanSouClient::new(None);
        let cloud_types = vec!["quark".to_string()];
        let results = client.search("测试", &cloud_types, 5).await.unwrap();
        println!("找到 {} 个结果", results.len());
        for (i, r) in results.iter().enumerate() {
            println!("  [{}] {} - {}", i + 1, r.note, r.url);
        }
    }
}
