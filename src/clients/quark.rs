use crate::error::{AppError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

const QUARK_API_BASE: &str = "https://drive.quark.cn/1/clouddrive";

/// 夸克分享探测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkShareInfo {
    pub ok: bool,
    pub state: String,
    pub message: String,
    pub files: Vec<QuarkFile>,
    pub file_count: usize,
    pub episode_count: usize,
}

/// 夸克文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkFile {
    pub name: String,
    pub fid: String,
    pub share_fid_token: String,
    pub is_dir: bool,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_type: Option<i32>,
}

/// 夸克分享探测客户端
pub struct QuarkShareProbe {
    cookie: String,
    client: Client,
}

#[derive(Deserialize)]
struct TokenResponse {
    code: i32,
    message: Option<String>,
    msg: Option<String>,
    data: Option<TokenData>,
}

#[derive(Deserialize)]
struct TokenData {
    stoken: Option<String>,
}

#[derive(Deserialize)]
struct FileListResponse {
    code: i32,
    message: Option<String>,
    msg: Option<String>,
    data: Option<FileListData>,
}

#[derive(Deserialize)]
struct FileListData {
    list: Option<Vec<HashMap<String, serde_json::Value>>>,
}

impl QuarkShareProbe {
    pub fn new(cookie: impl Into<String>) -> Self {
        let cookie = cookie.into();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"
                .parse()
                .unwrap(),
        );
        headers.insert("Accept", "application/json, text/plain, */*".parse().unwrap());
        headers.insert("Referer", "https://pan.quark.cn/".parse().unwrap());
        headers.insert("Origin", "https://pan.quark.cn".parse().unwrap());

        if !cookie.is_empty() {
            headers.insert("Cookie", cookie.parse().unwrap());
        }

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();

        Self { cookie, client }
    }

    /// 从分享链接提取 pwd_id
    pub fn extract_pwd_id(url: &str) -> Option<String> {
        let re = regex::Regex::new(r"/s/([A-Za-z0-9_-]+)").ok()?;
        re.captures(url)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }

    /// 获取分享 token
    async fn get_share_token(
        &self,
        pwd_id: &str,
        passcode: &str,
    ) -> Result<(Option<String>, Option<String>)> {
        let url = format!("{}/share/sharepage/token", QUARK_API_BASE);
        let payload = serde_json::json!({
            "pwd_id": pwd_id,
            "passcode": passcode,
        });

        let resp = self
            .client
            .post(&url)
            .query(&[("pr", "ucpro"), ("fr", "pc")])
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("请求夸克 token 失败: {}", e)))?;

        let data: TokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析夸克 token 响应失败: {}", e)))?;

        if data.code != 0 {
            let msg = data.message.or(data.msg).unwrap_or_else(|| "未知错误".to_string());
            return Ok((None, Some(msg)));
        }

        let token = data.data.and_then(|d| d.stoken);
        Ok((token, None))
    }

    /// 列出分享文件
    async fn list_files(
        &self,
        pwd_id: &str,
        stoken: &str,
        pdir_fid: &str,
    ) -> Result<(Vec<HashMap<String, serde_json::Value>>, Option<String>)> {
        let url = format!("{}/share/sharepage/detail", QUARK_API_BASE);

        let resp = self
            .client
            .get(&url)
            .query(&[
                ("pr", "ucpro"),
                ("fr", "pc"),
                ("pwd_id", pwd_id),
                ("stoken", stoken),
                ("pdir_fid", pdir_fid),
                ("force", "0"),
                ("_page", "1"),
                ("_size", "100"),
                ("_fetch_total", "1"),
                ("_fetch_sub_dirs", "0"),
                ("_sort", "file_type:asc,file_name:asc"),
            ])
            .send()
            .await
            .map_err(|e| AppError::Http(format!("请求夸克文件列表失败: {}", e)))?;

        let data: FileListResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析夸克文件列表失败: {}", e)))?;

        if data.code != 0 {
            let msg = data.message.or(data.msg).unwrap_or_else(|| "未知错误".to_string());
            return Ok((vec![], Some(msg)));
        }

        let list = data.data.and_then(|d| d.list).unwrap_or_default();
        Ok((list, None))
    }

    /// 探测分享链接
    pub async fn probe(
        &self,
        url: &str,
        passcode: &str,
        max_files: usize,
    ) -> QuarkShareInfo {
        let pwd_id = match Self::extract_pwd_id(url) {
            Some(id) => id,
            None => {
                return QuarkShareInfo {
                    ok: false,
                    state: "invalid_url".to_string(),
                    message: "不是有效的夸克分享链接".to_string(),
                    files: vec![],
                    file_count: 0,
                    episode_count: 0,
                }
            }
        };

        // 获取 token
        let (stoken, err) = match self.get_share_token(&pwd_id, passcode).await {
            Ok(result) => result,
            Err(e) => {
                return QuarkShareInfo {
                    ok: false,
                    state: "error".to_string(),
                    message: e.to_string(),
                    files: vec![],
                    file_count: 0,
                    episode_count: 0,
                }
            }
        };

        if let Some(err_msg) = err {
            let state = if err_msg.contains("提取码") || err_msg.contains("密码") || err_msg.to_lowercase().contains("pass") {
                "locked"
            } else {
                "bad"
            };
            return QuarkShareInfo {
                ok: false,
                state: state.to_string(),
                message: err_msg,
                files: vec![],
                file_count: 0,
                episode_count: 0,
            };
        }

        let stoken = match stoken {
            Some(t) => t,
            None => {
                return QuarkShareInfo {
                    ok: false,
                    state: "bad".to_string(),
                    message: "未能获取分享 token".to_string(),
                    files: vec![],
                    file_count: 0,
                    episode_count: 0,
                }
            }
        };

        // 获取文件列表
        let (raw, err) = match self.list_files(&pwd_id, &stoken, "0").await {
            Ok(result) => result,
            Err(e) => {
                return QuarkShareInfo {
                    ok: false,
                    state: "error".to_string(),
                    message: e.to_string(),
                    files: vec![],
                    file_count: 0,
                    episode_count: 0,
                }
            }
        };

        if let Some(err_msg) = err {
            return QuarkShareInfo {
                ok: false,
                state: "bad".to_string(),
                message: err_msg,
                files: vec![],
                file_count: 0,
                episode_count: 0,
            };
        }

        // 递归遍历文件夹
        let mut files = Vec::new();
        let mut queue = raw;

        while !queue.is_empty() && files.len() < max_files {
            let item = queue.remove(0);
            let fid = item
                .get("fid")
                .or_else(|| item.get("file_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = item
                .get("file_name")
                .or_else(|| item.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let is_dir = item.get("dir").and_then(|v| v.as_bool()).unwrap_or(false)
                || (item.get("file").and_then(|v| v.as_bool()) == Some(false))
                || (item.get("file_type").and_then(|v| v.as_i64()) == Some(0)
                    && item.get("format_type").is_none()
                    && item.get("size").and_then(|v| v.as_i64()).unwrap_or(0) == 0);

            files.push(QuarkFile {
                name: name.clone(),
                fid: fid.clone(),
                share_fid_token: item
                    .get("share_fid_token")
                    .or_else(|| item.get("file_token"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                is_dir,
                size: item.get("size").and_then(|v| v.as_i64()).unwrap_or(0),
                category: item.get("category").and_then(|v| v.as_str()).map(|s| s.to_string()),
                format_type: item.get("format_type").or_else(|| item.get("file_type")).and_then(|v| v.as_i64()).map(|n| n as i32),
            });

            // 如果是目录且未达上限，递归获取
            if is_dir && !fid.is_empty() && files.len() < max_files {
                if let Ok((children, None)) = self.list_files(&pwd_id, &stoken, &fid).await {
                    queue.extend(children);
                }
            }
        }

        let episode_count = count_episodes(&files);

        QuarkShareInfo {
            ok: true,
            state: "ok".to_string(),
            message: "链接可访问".to_string(),
            file_count: files.len(),
            episode_count,
            files,
        }
    }
}

impl Default for QuarkShareProbe {
    fn default() -> Self {
        Self::new("")
    }
}

/// 统计集数（简化版）
fn count_episodes(files: &[QuarkFile]) -> usize {
    use regex::Regex;
    let patterns = vec![
        Regex::new(r"(?i)(?:^|[^A-Za-z])S\d{1,2}E\d{1,3}(?:[^A-Za-z]|$)").unwrap(),
        Regex::new(r"(?:第\s*\d{1,3}\s*[集话])").unwrap(),
        Regex::new(r"(?i)(?:^|[^\d])E\d{1,3}(?:[^\d]|$)").unwrap(),
        Regex::new(r"(?i)(?:^|[^\d])\d{1,3}\s*\.\s*(?:mkv|mp4|avi|ts|mov|wmv)$").unwrap(),
    ];

    files
        .iter()
        .filter(|f| {
            let lower = f.name.to_lowercase();
            let is_video = [".mkv", ".mp4", ".avi", ".ts", ".mov", ".wmv", ".flv", ".m4v"]
                .iter()
                .any(|ext| lower.ends_with(ext));
            is_video && patterns.iter().any(|p| p.is_match(&f.name))
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pwd_id() {
        let url = "https://pan.quark.cn/s/abc123def456";
        assert_eq!(QuarkShareProbe::extract_pwd_id(url), Some("abc123def456".to_string()));

        let url2 = "https://pan.quark.cn/s/test_ID-789";
        assert_eq!(QuarkShareProbe::extract_pwd_id(url2), Some("test_ID-789".to_string()));

        assert_eq!(QuarkShareProbe::extract_pwd_id("invalid"), None);
    }

    #[test]
    fn test_count_episodes() {
        let files = vec![
            QuarkFile {
                name: "第01集.mkv".to_string(),
                fid: "1".to_string(),
                share_fid_token: "".to_string(),
                is_dir: false,
                size: 1000,
                category: None,
                format_type: None,
            },
            QuarkFile {
                name: "S01E02.mp4".to_string(),
                fid: "2".to_string(),
                share_fid_token: "".to_string(),
                is_dir: false,
                size: 2000,
                category: None,
                format_type: None,
            },
            QuarkFile {
                name: "预告.mp4".to_string(),
                fid: "3".to_string(),
                share_fid_token: "".to_string(),
                is_dir: false,
                size: 500,
                category: None,
                format_type: None,
            },
        ];

        assert_eq!(count_episodes(&files), 2);
    }
}
