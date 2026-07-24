use super::ensure_upstream_status;
use crate::clients::http_pool::ObservedRequestBuilder;
use crate::error::{AppError, Result};
use regex::Regex;
use reqwest::header::HeaderValue;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

const QUARK_API_BASE: &str = "https://drive.quark.cn/1/clouddrive";

fn hardcoded_regex(pattern: &str) -> Regex {
    Regex::new(pattern)
        .unwrap_or_else(|error| panic!("invalid hard-coded quark regex `{pattern}`: {error}"))
}

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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_type: Option<i32>,
}

/// 夸克分享探测客户端
pub struct QuarkShareProbe {
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

fn raw_time_field(item: &HashMap<String, serde_json::Value>) -> Option<String> {
    [
        "updated_at",
        "created_at",
        "update_time",
        "updated_time",
        "created_time",
        "create_time",
        "file_update_time",
        "file_create_time",
        "last_update_at",
        "last_update_time",
    ]
    .iter()
    .find_map(|key| {
        item.get(*key).and_then(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .or_else(|| value.as_i64().map(|number| number.to_string()))
                .or_else(|| value.as_u64().map(|number| number.to_string()))
        })
    })
}

fn append_display_path(parent_path: &str, name: &str) -> String {
    let parent_path = parent_path.trim().trim_matches('/');
    let name = name.trim().trim_matches('/');
    if name.is_empty() {
        return parent_path.to_string();
    }
    if parent_path.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent_path, name)
    }
}

impl QuarkShareProbe {
    pub fn new(cookie: impl Into<String>) -> Self {
        let cookie = cookie.into();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "User-Agent",
            HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/json, text/plain, */*"),
        );
        headers.insert("Referer", HeaderValue::from_static("https://pan.quark.cn/"));
        headers.insert("Origin", HeaderValue::from_static("https://pan.quark.cn"));

        if !cookie.is_empty() {
            match HeaderValue::from_str(&cookie) {
                Ok(value) => {
                    headers.insert("Cookie", value);
                }
                Err(error) => {
                    tracing::warn!("夸克 Cookie 包含非法 HTTP header 字符，已跳过: {}", error);
                }
            }
        }

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_else(|error| {
                tracing::warn!("创建夸克探测 HTTP 客户端失败，使用默认客户端: {}", error);
                Client::new()
            });

        Self { client }
    }

    /// 从分享链接提取 pwd_id
    pub fn extract_pwd_id(url: &str) -> Option<String> {
        static SHARE_ID_RE: LazyLock<Regex> =
            LazyLock::new(|| hardcoded_regex(r"/s/([A-Za-z0-9_-]+)"));
        SHARE_ID_RE
            .captures(url)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }

    /// 获取分享 token（公开方法，用于转存功能）
    pub async fn get_share_token(
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
            .send_observed("quark")
            .await
            .map_err(|e| AppError::Http(format!("请求夸克 token 失败: {}", e)))?;
        ensure_upstream_status(&resp, "请求夸克 token")?;

        let data: TokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析夸克 token 响应失败: {}", e)))?;

        if data.code != 0 {
            let msg = data
                .message
                .or(data.msg)
                .unwrap_or_else(|| "未知错误".to_string());
            return Ok((None, Some(msg)));
        }

        let token = data.data.and_then(|d| d.stoken);
        Ok((token, None))
    }

    /// 列出分享文件（公开方法，用于转存功能）
    pub async fn list_share_files(
        &self,
        pwd_id: &str,
        stoken: &str,
        pdir_fid: &str,
    ) -> Result<(Vec<HashMap<String, serde_json::Value>>, Option<String>)> {
        self.list_files(pwd_id, stoken, pdir_fid).await
    }

    /// 列出分享文件
    async fn list_files(
        &self,
        pwd_id: &str,
        stoken: &str,
        pdir_fid: &str,
    ) -> Result<(Vec<HashMap<String, serde_json::Value>>, Option<String>)> {
        let url = format!("{}/share/sharepage/detail", QUARK_API_BASE);
        const PAGE_SIZE: usize = 100;
        // 单目录分页安全上限，防止异常响应导致无限翻页
        const MAX_PAGES: usize = 20;

        // 目录内容按页拉取：单页 100 项，翻页直到取完。
        // 返回 Some(错误) 时列表可能不完整（首页失败则为空），由调用方标记 partial。
        let mut all: Vec<HashMap<String, serde_json::Value>> = Vec::new();
        for page in 1..=MAX_PAGES {
            let page_param = page.to_string();
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
                    ("_page", page_param.as_str()),
                    ("_size", "100"),
                    ("_fetch_total", "1"),
                    ("_fetch_sub_dirs", "0"),
                    ("_sort", "file_type:asc,file_name:asc"),
                ])
                .send_observed("quark")
                .await
                .map_err(|e| AppError::Http(format!("请求夸克文件列表失败: {}", e)));
            let resp = match resp {
                Ok(resp) => resp,
                Err(error) if page > 1 => return Ok((all, Some(error.to_string()))),
                Err(error) => return Err(error),
            };
            if let Err(error) = ensure_upstream_status(&resp, "请求夸克文件列表") {
                // 限流错误始终向上传播，交给统一退避处理
                if page > 1 && !matches!(error, AppError::RateLimited(_)) {
                    return Ok((all, Some(error.to_string())));
                }
                return Err(error);
            }

            let data: Result<FileListResponse> = resp
                .json()
                .await
                .map_err(|e| AppError::Http(format!("解析夸克文件列表失败: {}", e)));
            let data = match data {
                Ok(data) => data,
                Err(error) if page > 1 => return Ok((all, Some(error.to_string()))),
                Err(error) => return Err(error),
            };

            if data.code != 0 {
                let msg = data
                    .message
                    .or(data.msg)
                    .unwrap_or_else(|| "未知错误".to_string());
                return Ok((all, Some(msg)));
            }

            let list = data.data.and_then(|d| d.list).unwrap_or_default();
            let fetched = list.len();
            all.extend(list);
            if fetched < PAGE_SIZE {
                break;
            }
        }
        Ok((all, None))
    }

    /// 探测分享链接
    pub async fn probe(&self, url: &str, passcode: &str, max_files: usize) -> QuarkShareInfo {
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
                let (state, message) = match e {
                    AppError::RateLimited(message) => ("rate_limited", message),
                    error => ("error", error.to_string()),
                };
                return QuarkShareInfo {
                    ok: false,
                    state: state.to_string(),
                    message,
                    files: vec![],
                    file_count: 0,
                    episode_count: 0,
                };
            }
        };

        if let Some(err_msg) = err {
            let state = if err_msg.contains("提取码")
                || err_msg.contains("密码")
                || err_msg.to_lowercase().contains("pass")
            {
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
                let (state, message) = match e {
                    AppError::RateLimited(message) => ("rate_limited", message),
                    error => ("error", error.to_string()),
                };
                return QuarkShareInfo {
                    ok: false,
                    state: state.to_string(),
                    message,
                    files: vec![],
                    file_count: 0,
                    episode_count: 0,
                };
            }
        };

        // 根目录分页部分失败时保留已取得的文件并标记 partial；完全失败仍视为 bad。
        let mut partial_failure = false;
        if let Some(err_msg) = err {
            if raw.is_empty() {
                return QuarkShareInfo {
                    ok: false,
                    state: "bad".to_string(),
                    message: err_msg,
                    files: vec![],
                    file_count: 0,
                    episode_count: 0,
                };
            }
            partial_failure = true;
            tracing::warn!("分享根目录分页读取部分失败，文件树可能不完整: {}", err_msg);
        }

        // 递归遍历文件夹
        let mut files = Vec::new();
        let mut truncated = false;
        let mut queue: std::collections::VecDeque<_> =
            raw.into_iter().map(|item| (item, String::new())).collect();

        while let Some((item, parent_path)) = queue.pop_front() {
            if files.len() >= max_files {
                truncated = true;
                break;
            }
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
                    && !item.contains_key("format_type")
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
                parent_path: parent_path.clone(),
                updated_at: raw_time_field(&item),
                category: item
                    .get("category")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                format_type: item
                    .get("format_type")
                    .or_else(|| item.get("file_type"))
                    .and_then(|v| v.as_i64())
                    .map(|n| n as i32),
            });

            // 如果是目录，递归获取（达到上限则记为截断）
            if is_dir && !fid.is_empty() {
                if files.len() < max_files {
                    match self.list_files(&pwd_id, &stoken, &fid).await {
                        Ok((children, child_err)) => {
                            if let Some(err_msg) = child_err {
                                partial_failure = true;
                                tracing::warn!(
                                    "列举子目录 {} (fid={}) 部分失败，文件树可能不完整: {}",
                                    name,
                                    fid,
                                    err_msg
                                );
                            }
                            let child_parent_path = append_display_path(&parent_path, &name);
                            queue.extend(
                                children
                                    .into_iter()
                                    .map(|child| (child, child_parent_path.clone())),
                            );
                        }
                        Err(e) => {
                            partial_failure = true;
                            tracing::warn!(
                                "列举子目录 {} (fid={}) 请求失败，文件树可能不完整: {}",
                                name,
                                fid,
                                e
                            );
                        }
                    }
                } else {
                    truncated = true;
                }
            }
        }

        let episode_count = count_episodes(&files);

        let state = if partial_failure || truncated {
            "partial".to_string()
        } else {
            "ok".to_string()
        };
        let message = if truncated {
            format!("链接可访问，但文件数超过探测上限 {max_files}，列表已截断，可能有内容未被发现")
        } else if partial_failure {
            "链接可访问，但部分目录读取失败，文件列表可能不完整".to_string()
        } else {
            "链接可访问".to_string()
        };
        QuarkShareInfo {
            ok: true,
            state,
            message,
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
    static EPISODE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![
            hardcoded_regex(r"(?i)(?:^|[^A-Za-z])S\d{1,2}E\d{1,3}(?:[^A-Za-z]|$)"),
            hardcoded_regex(r"(?:第\s*\d{1,3}\s*[集话])"),
            hardcoded_regex(r"(?i)(?:^|[^\d])E\d{1,3}(?:[^\d]|$)"),
            hardcoded_regex(r"(?i)(?:^|[^\d])\d{1,3}\s*\.\s*(?:mkv|mp4|avi|ts|mov|wmv)$"),
        ]
    });

    files
        .iter()
        .filter(|f| {
            let lower = f.name.to_lowercase();
            let is_video = [
                ".mkv", ".mp4", ".avi", ".ts", ".mov", ".wmv", ".flv", ".m4v",
            ]
            .iter()
            .any(|ext| lower.ends_with(ext));
            is_video && EPISODE_PATTERNS.iter().any(|p| p.is_match(&f.name))
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pwd_id() {
        let url = "https://pan.quark.cn/s/abc123def456";
        assert_eq!(
            QuarkShareProbe::extract_pwd_id(url),
            Some("abc123def456".to_string())
        );

        let url2 = "https://pan.quark.cn/s/test_ID-789";
        assert_eq!(
            QuarkShareProbe::extract_pwd_id(url2),
            Some("test_ID-789".to_string())
        );

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
                parent_path: String::new(),
                updated_at: None,
                category: None,
                format_type: None,
            },
            QuarkFile {
                name: "S01E02.mp4".to_string(),
                fid: "2".to_string(),
                share_fid_token: "".to_string(),
                is_dir: false,
                size: 2000,
                parent_path: String::new(),
                updated_at: None,
                category: None,
                format_type: None,
            },
            QuarkFile {
                name: "预告.mp4".to_string(),
                fid: "3".to_string(),
                share_fid_token: "".to_string(),
                is_dir: false,
                size: 500,
                parent_path: String::new(),
                updated_at: None,
                category: None,
                format_type: None,
            },
        ];

        assert_eq!(count_episodes(&files), 2);
    }
}
