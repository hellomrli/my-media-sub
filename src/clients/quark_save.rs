use crate::error::{AppError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

const QUARK_API_BASE: &str = "https://drive.quark.cn/1/clouddrive";

/// 夸克转存客户端
pub struct QuarkSaveClient {
    cookie: String,
    client: Client,
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
    pub file_name: String,  // 改为 file_name 以匹配前端
    pub file: bool,         // 添加 file 字段
    pub is_dir: bool,
    pub size: i64,
    pub updated_at: String,
}

impl QuarkSaveClient {
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
        let url = format!("{}{}", QUARK_API_BASE, path);

        let resp = self
            .client
            .post(&url)
            .query(&[("pr", "ucpro"), ("fr", "pc")])
            .json(payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("夸克 POST 请求失败: {}", e)))?;

        resp.json()
            .await
            .map_err(|e| AppError::Http(format!("解析夸克响应失败: {}", e)))
    }

    // ── 目录管理 ──────────────────────────────────────────

    /// 列出目录内容
    pub async fn list_dir(&self, parent_fid: &str) -> Result<Vec<NormalizedItem>> {
        let data = self
            .get(
                "/file/sort",
                &[
                    ("pdir_fid", parent_fid),
                    ("_page", "1"),
                    ("_size", "200"),
                    ("_fetch_total", "1"),
                    ("fetch_all_file", "1"),
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
                let map: HashMap<String, Value> = item.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                Self::normalize_item(&map)
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
            file_name,
            file,
            is_dir,
            size,
            updated_at,
        }
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
    pub async fn rename_item(&self, fid: &str, name: &str) -> Result<()> {
        let payload = serde_json::json!({
            "fid": fid,
            "file_name": name,
        });

        let data = self.post("/file/rename", &payload).await?;

        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }

        Ok(())
    }

    /// 移动文件
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
    fn test_client_creation() {
        let client = QuarkSaveClient::new("test_cookie");
        assert!(!client.cookie.is_empty());
    }

    #[test]
    fn test_normalize_item() {
        let mut item = HashMap::new();
        item.insert("fid".to_string(), Value::String("123".to_string()));
        item.insert("file_name".to_string(), Value::String("test.mkv".to_string()));
        item.insert("size".to_string(), Value::Number(1024.into()));
        item.insert("dir".to_string(), Value::Bool(false));

        let normalized = QuarkSaveClient::normalize_item(&item);
        assert_eq!(normalized.fid, "123");
        assert_eq!(normalized.name, "test.mkv");
        assert_eq!(normalized.size, 1024);
        assert!(!normalized.is_dir);
    }
}
