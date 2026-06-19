use crate::error::{AppError, Result};
use reqwest::header::SET_COOKIE;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const QUARK_API_BASE: &str = "https://drive.quark.cn/1/clouddrive";
const QUARK_PC_API_BASE: &str = "https://drive-pc.quark.cn/1/clouddrive";
const QUARK_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) quark-cloud-drive/3.14.2 Chrome/112.0.5615.165 Electron/24.1.3.8 Safari/537.36 Channel/pckk_other_ch";

/// 夸克转存客户端
pub struct QuarkSaveClient {
    client: Client,
    cookie: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    code: i32,
    message: Option<String>,
    msg: Option<String>,
    data: Option<Value>,
}

/// 归一化的文件信息
#[derive(Debug, Clone, Serialize)]
pub struct NormalizedItem {
    pub fid: String,
    pub parent_fid: String,
    pub file_name: String, // 改为 file_name 以匹配前端
    pub file: bool,        // 添加 file 字段
    pub is_dir: bool,
    pub size: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuarkDownloadInfo {
    pub fid: String,
    pub file_name: String,
    pub size: i64,
    pub download_url: String,
    pub headers: Vec<String>,
}

impl QuarkSaveClient {
    pub fn new(cookie: impl Into<String>) -> Self {
        let cookie = cookie.into();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("User-Agent", QUARK_USER_AGENT.parse().unwrap());
        headers.insert(
            "Accept",
            "application/json, text/plain, */*".parse().unwrap(),
        );
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

        Self { client, cookie }
    }

    fn api_error(data: &ApiResponse) -> Option<String> {
        if data.code == 0 {
            return None;
        }
        Some(
            data.message
                .clone()
                .or_else(|| data.msg.clone())
                .unwrap_or_else(|| format!("API 错误: code={}", data.code)),
        )
    }

    fn extract_fid(data: &Value) -> Option<String> {
        // 尝试从 data 中提取 fid
        if let Some(obj) = data.as_object() {
            if let Some(fid) = obj.get("fid").and_then(|v| v.as_str()) {
                return Some(fid.to_string());
            }
            if let Some(file_id) = obj.get("file_id").and_then(|v| v.as_str()) {
                return Some(file_id.to_string());
            }
        }

        // 尝试从数组第一个元素提取
        if let Some(arr) = data.as_array() {
            if let Some(first) = arr.first() {
                if let Some(obj) = first.as_object() {
                    if let Some(fid) = obj.get("fid").and_then(|v| v.as_str()) {
                        return Some(fid.to_string());
                    }
                    if let Some(file_id) = obj.get("file_id").and_then(|v| v.as_str()) {
                        return Some(file_id.to_string());
                    }
                }
            }
        }

        None
    }

    async fn get(&self, path: &str, params: &[(&str, &str)]) -> Result<ApiResponse> {
        let url = format!("{}{}", QUARK_API_BASE, path);
        let mut all_params = vec![("pr", "ucpro"), ("fr", "pc")];
        all_params.extend_from_slice(params);

        let resp = self
            .client
            .get(&url)
            .query(&all_params)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("夸克 GET 请求失败: {}", e)))?;

        resp.json()
            .await
            .map_err(|e| AppError::Http(format!("解析夸克响应失败: {}", e)))
    }

    async fn post(&self, path: &str, payload: &Value) -> Result<ApiResponse> {
        self.post_with_base(QUARK_API_BASE, path, payload).await
    }

    async fn post_with_base(&self, base: &str, path: &str, payload: &Value) -> Result<ApiResponse> {
        let (data, _) = self
            .post_with_base_capture_cookies(base, path, payload)
            .await?;
        Ok(data)
    }

    async fn post_with_base_capture_cookies(
        &self,
        base: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(ApiResponse, Vec<String>)> {
        let url = format!("{}{}", base, path);

        let resp = self
            .client
            .post(&url)
            .query(&[("pr", "ucpro"), ("fr", "pc")])
            .json(payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("夸克 POST 请求失败: {}", e)))?;

        let set_cookies = resp
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| value.split(';').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect();

        let data = resp
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析夸克响应失败: {}", e)))?;

        Ok((data, set_cookies))
    }

    fn download_headers(&self, extra_cookies: &[String]) -> Vec<String> {
        let mut headers = vec![
            format!("User-Agent: {}", QUARK_USER_AGENT),
            "Referer: https://pan.quark.cn/".to_string(),
        ];
        let cookie = self.download_cookie(extra_cookies);
        if !cookie.is_empty() {
            headers.push(format!("Cookie: {}", cookie));
        }
        headers
    }

    fn download_cookie(&self, extra_cookies: &[String]) -> String {
        let mut cookies = Vec::new();
        let base_cookie = self.cookie.trim().trim_end_matches(';').trim();
        if !base_cookie.is_empty() {
            cookies.push(base_cookie.to_string());
        }
        cookies.extend(
            extra_cookies
                .iter()
                .map(|cookie| cookie.trim().trim_end_matches(';').trim())
                .filter(|cookie| !cookie.is_empty())
                .map(ToString::to_string),
        );
        cookies.join("; ")
    }

    // ── 目录管理 ──────────────────────────────────────────

    /// 列出目录内容
    pub async fn list_dir(&self, parent_fid: &str) -> Result<Vec<NormalizedItem>> {
        const PAGE_SIZE: usize = 200;
        const MAX_PAGES: usize = 50;

        let mut items = Vec::new();
        let mut seen = HashSet::new();
        for page in 1..=MAX_PAGES {
            let page_items = self.list_dir_page(parent_fid, page, PAGE_SIZE).await?;
            let count = page_items.len();
            for item in page_items {
                if seen.insert(item.fid.clone()) {
                    items.push(item);
                }
            }

            if count < PAGE_SIZE {
                break;
            }
        }

        Ok(items)
    }

    async fn list_dir_page(
        &self,
        parent_fid: &str,
        page: usize,
        page_size: usize,
    ) -> Result<Vec<NormalizedItem>> {
        let page = page.to_string();
        let page_size = page_size.to_string();
        let data = self
            .get(
                "/file/sort",
                &[
                    ("pdir_fid", parent_fid),
                    ("_page", page.as_str()),
                    ("_size", page_size.as_str()),
                    ("_fetch_total", "1"),
                    ("fetch_risk_file_name", "1"),
                    ("_sort", "file_type:asc,file_name:asc"),
                ],
            )
            .await?;

        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }

        let list = data
            .data
            .and_then(|d| d.get("list").cloned())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_object())
            .map(|item| {
                let map: HashMap<String, Value> =
                    item.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                let mut normalized = Self::normalize_item(&map);
                if normalized.parent_fid.is_empty() {
                    normalized.parent_fid = parent_fid.to_string();
                }
                normalized
            })
            .collect();

        Ok(list)
    }

    /// 归一化文件信息
    pub fn normalize_item(item: &HashMap<String, Value>) -> NormalizedItem {
        let fid = item
            .get("fid")
            .or_else(|| item.get("file_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let parent_fid = item
            .get("pdir_fid")
            .or_else(|| item.get("parent_fid"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let file_name = item
            .get("file_name")
            .or_else(|| item.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let file = item.get("file").and_then(|v| v.as_bool()).unwrap_or(true);
        let is_dir = item.get("dir").and_then(|v| v.as_bool()).unwrap_or(false)
            || (item.get("file").and_then(|v| v.as_bool()) == Some(false))
            || (item.get("file_type").and_then(|v| v.as_i64()) == Some(0));
        let size = item.get("size").and_then(|v| v.as_i64()).unwrap_or(0);
        let updated_at = item
            .get("updated_at")
            .or_else(|| item.get("last_update_at"))
            .or_else(|| item.get("created_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        NormalizedItem {
            fid,
            parent_fid,
            file_name,
            file,
            is_dir,
            size,
            updated_at,
        }
    }

    pub async fn download_infos(&self, fids: &[String]) -> Result<Vec<QuarkDownloadInfo>> {
        let fids: Vec<String> = fids
            .iter()
            .map(|fid| fid.trim().to_string())
            .filter(|fid| !fid.is_empty())
            .collect();
        if fids.is_empty() {
            return Err(AppError::Validation("未选择要下载的文件".to_string()));
        }

        let payload = serde_json::json!({
            "fids": fids,
        });
        let (data, set_cookies) = self
            .post_with_base_capture_cookies(QUARK_PC_API_BASE, "/file/download", &payload)
            .await?;

        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }

        let Some(data) = data.data else {
            return Err(AppError::Http("夸克下载接口响应为空".to_string()));
        };
        let headers = self.download_headers(&set_cookies);
        let infos = Self::extract_download_infos(&data, &headers, &fids);
        if infos.is_empty() {
            return Err(AppError::Http("未能获取夸克文件下载链接".to_string()));
        }

        Ok(infos)
    }

    fn extract_download_infos(
        data: &Value,
        headers: &[String],
        requested_fids: &[String],
    ) -> Vec<QuarkDownloadInfo> {
        let values: Vec<&Value> = if let Some(list) = data.get("list").and_then(Value::as_array) {
            list.iter().collect()
        } else if let Some(list) = data.get("file_list").and_then(Value::as_array) {
            list.iter().collect()
        } else if let Some(list) = data.as_array() {
            list.iter().collect()
        } else {
            vec![data]
        };

        values
            .into_iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let download_url = item
                    .get("download_url")
                    .or_else(|| item.get("downloadUrl"))
                    .or_else(|| item.get("dlink"))
                    .or_else(|| item.get("url"))
                    .and_then(Value::as_str)
                    .filter(|url| !url.trim().is_empty())?
                    .to_string();

                let fid = item
                    .get("fid")
                    .or_else(|| item.get("file_id"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .or_else(|| requested_fids.get(index).cloned())
                    .unwrap_or_default();
                let file_name = item
                    .get("file_name")
                    .or_else(|| item.get("name"))
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .map(ToString::to_string)
                    .unwrap_or_else(|| fid.clone());
                let size = item.get("size").and_then(Value::as_i64).unwrap_or(0);

                Some(QuarkDownloadInfo {
                    fid,
                    file_name,
                    size,
                    download_url,
                    headers: headers.to_vec(),
                })
            })
            .collect()
    }

    /// 创建目录
    pub async fn create_dir(&self, parent_fid: &str, name: &str) -> Result<String> {
        let payload = serde_json::json!({
            "pdir_fid": parent_fid,
            "file_name": name,
            "dir_path": "",
            "dir_init_lock": false,
        });

        let data = self.post("/file", &payload).await?;

        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }

        Self::extract_fid(&data.data.unwrap_or(Value::Null))
            .ok_or_else(|| AppError::Http("无法从响应中提取 fid".to_string()))
    }

    /// 确保目录路径存在（递归创建）
    pub async fn ensure_dir_path(&self, path: &str) -> Result<String> {
        let mut parent_fid = "0".to_string();

        for part in path.trim_matches('/').split('/').filter(|p| !p.is_empty()) {
            let items = self.list_dir(&parent_fid).await?;
            let mut found = None;

            for item in items {
                if item.is_dir && item.file_name == part {
                    found = Some(item.fid.clone());
                    break;
                }
            }

            parent_fid = if let Some(fid) = found {
                fid
            } else {
                self.create_dir(&parent_fid, part).await?
            };
        }

        Ok(parent_fid)
    }

    // ── 文件操作 ──────────────────────────────────────────

    /// 删除文件
    #[allow(dead_code)]
    pub async fn delete_items(&self, fids: &[String]) -> Result<()> {
        let payload = serde_json::json!({
            "action_type": 1,
            "exclude_fids": [],
            "filelist": fids,
        });

        let data = self.post("/file/delete", &payload).await?;

        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }

        Ok(())
    }

    /// 重命名文件
    pub async fn rename_item(&self, fid: &str, name: &str, parent_fid: Option<&str>) -> Result<()> {
        let mut payload = serde_json::json!({
            "fid": fid,
            "file_name": name,
        });
        if let Some(parent_fid) = parent_fid.filter(|fid| !fid.trim().is_empty()) {
            payload["pdir_fid"] = serde_json::json!(parent_fid);
        }

        let data = self.post("/file/rename", &payload).await?;

        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }

        Ok(())
    }

    /// 移动文件
    #[allow(dead_code)]
    pub async fn move_items(&self, fids: &[String], target_fid: &str) -> Result<()> {
        let payload = serde_json::json!({
            "action_type": 1,
            "exclude_fids": [],
            "filelist": fids,
            "to_pdir_fid": target_fid,
        });

        let data = self.post("/file/move", &payload).await?;

        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }

        Ok(())
    }

    /// 复制文件
    #[allow(dead_code)]
    pub async fn copy_items(&self, fids: &[String], target_fid: &str) -> Result<()> {
        let payload = serde_json::json!({
            "action_type": 1,
            "exclude_fids": [],
            "filelist": fids,
            "to_pdir_fid": target_fid,
        });

        let data = self.post("/file/copy", &payload).await?;

        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }

        Ok(())
    }

    // ── 转存分享文件 ──────────────────────────────────────────

    /// 转存分享文件到自己的网盘
    pub async fn save_share_files(
        &self,
        pwd_id: &str,
        stoken: &str,
        fid_list: &[String],
        fid_token_list: &[String],
        to_pdir_fid: &str,
    ) -> Result<()> {
        let payload = serde_json::json!({
            "fid_list": fid_list,
            "fid_token_list": fid_token_list,
            "to_pdir_fid": to_pdir_fid,
            "pwd_id": pwd_id,
            "stoken": stoken,
        });

        let data = self.post("/share/sharepage/save", &payload).await?;

        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }

        Ok(())
    }

    /// 转存整个分享链接的所有顶层文件
    #[allow(dead_code)]
    pub async fn save_entire_share(
        &self,
        pwd_id: &str,
        stoken: &str,
        top_files: &[super::quark::QuarkFile],
        to_pdir_fid: &str,
    ) -> Result<()> {
        let mut fid_list = Vec::new();
        let mut fid_token_list = Vec::new();

        for f in top_files {
            if !f.fid.is_empty() && !f.share_fid_token.is_empty() {
                fid_list.push(f.fid.clone());
                fid_token_list.push(f.share_fid_token.clone());
            }
        }

        if fid_list.is_empty() {
            return Err(AppError::Validation("没有可转存的文件".to_string()));
        }

        self.save_share_files(pwd_id, stoken, &fid_list, &fid_token_list, to_pdir_fid)
            .await
    }
}

impl Default for QuarkSaveClient {
    fn default() -> Self {
        Self::new("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_item() {
        let mut item = HashMap::new();
        item.insert("fid".to_string(), Value::String("123".to_string()));
        item.insert(
            "file_name".to_string(),
            Value::String("test.mkv".to_string()),
        );
        item.insert("size".to_string(), Value::Number(1024.into()));
        item.insert("dir".to_string(), Value::Bool(false));

        let normalized = QuarkSaveClient::normalize_item(&item);
        assert_eq!(normalized.fid, "123");
        assert_eq!(normalized.file_name, "test.mkv");
        assert_eq!(normalized.size, 1024);
        assert!(!normalized.is_dir);
    }

    #[test]
    fn test_extract_download_infos_from_list() {
        let data = serde_json::json!({
            "list": [
                {
                    "fid": "fid1",
                    "file_name": "EP01.mkv",
                    "size": 1024,
                    "download_url": "https://download.example.com/ep01"
                }
            ]
        });
        let headers = vec!["Cookie: test".to_string()];
        let infos =
            QuarkSaveClient::extract_download_infos(&data, &headers, &["fallback".to_string()]);

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].fid, "fid1");
        assert_eq!(infos[0].file_name, "EP01.mkv");
        assert_eq!(infos[0].download_url, "https://download.example.com/ep01");
        assert_eq!(infos[0].headers, headers);
    }

    #[test]
    fn test_extract_download_infos_from_single_object() {
        let data = serde_json::json!({
            "name": "movie.mp4",
            "url": "https://download.example.com/movie"
        });
        let infos =
            QuarkSaveClient::extract_download_infos(&data, &[], &["requested-fid".to_string()]);

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].fid, "requested-fid");
        assert_eq!(infos[0].file_name, "movie.mp4");
    }

    #[test]
    fn test_download_headers_include_temporary_cookies() {
        let client = QuarkSaveClient::new("k=v; base=1;");
        let headers = client.download_headers(&["__puus=temp".to_string()]);

        assert!(headers
            .iter()
            .any(|header| header == "Cookie: k=v; base=1; __puus=temp"));
    }
}
