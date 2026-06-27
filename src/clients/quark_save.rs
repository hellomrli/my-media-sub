use crate::error::{AppError, Result};
use reqwest::header::{HeaderValue, SET_COOKIE};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const QUARK_API_BASE: &str = "https://drive.quark.cn/1/clouddrive";
const QUARK_PC_API_BASE: &str = "https://drive-pc.quark.cn/1/clouddrive";
const QUARK_MOBILE_API_BASE: &str = "https://drive-m.quark.cn/1/clouddrive";
const QUARK_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) quark-cloud-drive/3.14.2 Chrome/112.0.5615.165 Electron/24.1.3.8 Safari/537.36 Channel/pckk_other_ch";

/// 夸克转存客户端
pub struct QuarkSaveClient {
    client: Client,
    mobile_client: Client,
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

#[derive(Debug, Clone, Serialize)]
pub struct QuarkSigninResult {
    pub signed: bool,
    pub already_signed: bool,
    pub daily_reward_bytes: i64,
    pub total_capacity_bytes: i64,
    pub sign_reward_bytes: i64,
    pub member_type: String,
    pub sign_progress: i64,
    pub sign_target: i64,
}

#[derive(Debug, Clone)]
struct QuarkMobileParams {
    kps: String,
    sign: String,
    vcode: String,
}

#[derive(Debug, Clone)]
struct QuarkGrowthInfo {
    total_capacity_bytes: i64,
    sign_reward_bytes: i64,
    member_type: String,
    sign_daily: bool,
    sign_daily_reward_bytes: i64,
    sign_progress: i64,
    sign_target: i64,
}

impl QuarkSaveClient {
    pub fn new(cookie: impl Into<String>) -> Self {
        let cookie = cookie.into();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("User-Agent", HeaderValue::from_static(QUARK_USER_AGENT));
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
                tracing::warn!("创建夸克转存 HTTP 客户端失败，使用默认客户端: {}", error);
                Client::new()
            });

        let mut mobile_headers = reqwest::header::HeaderMap::new();
        mobile_headers.insert("User-Agent", HeaderValue::from_static(QUARK_USER_AGENT));
        mobile_headers.insert("content-type", HeaderValue::from_static("application/json"));
        let mobile_client = Client::builder()
            .default_headers(mobile_headers)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_else(|error| {
                tracing::warn!("创建夸克移动端 HTTP 客户端失败，使用默认客户端: {}", error);
                Client::new()
            });

        Self {
            client,
            mobile_client,
            cookie,
        }
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

    fn mobile_params(&self) -> Option<QuarkMobileParams> {
        let kps = mobile_param_value(&self.cookie, "kps")?;
        let sign = mobile_param_value(&self.cookie, "sign")?;
        let vcode = mobile_param_value(&self.cookie, "vcode")?;
        Some(QuarkMobileParams { kps, sign, vcode })
    }

    async fn mobile_get(&self, path: &str, params: &QuarkMobileParams) -> Result<ApiResponse> {
        let url = format!("{}{}", QUARK_MOBILE_API_BASE, path);
        self.mobile_client
            .get(&url)
            .query(&[
                ("pr", "ucpro"),
                ("fr", "android"),
                ("kps", params.kps.as_str()),
                ("sign", params.sign.as_str()),
                ("vcode", params.vcode.as_str()),
            ])
            .send()
            .await
            .map_err(|e| AppError::Http(format!("夸克移动端 GET 请求失败: {}", e)))?
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析夸克移动端响应失败: {}", e)))
    }

    async fn mobile_post(
        &self,
        path: &str,
        params: &QuarkMobileParams,
        payload: &Value,
    ) -> Result<ApiResponse> {
        let url = format!("{}{}", QUARK_MOBILE_API_BASE, path);
        self.mobile_client
            .post(&url)
            .query(&[
                ("pr", "ucpro"),
                ("fr", "android"),
                ("kps", params.kps.as_str()),
                ("sign", params.sign.as_str()),
                ("vcode", params.vcode.as_str()),
            ])
            .json(payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("夸克移动端 POST 请求失败: {}", e)))?
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析夸克移动端响应失败: {}", e)))
    }

    pub async fn signin(&self) -> Result<QuarkSigninResult> {
        let params = self.mobile_params().ok_or_else(|| {
            AppError::Validation("夸克 Cookie 缺少移动端签到参数 kps/sign/vcode".to_string())
        })?;

        let data = self.mobile_get("/capacity/growth/info", &params).await?;
        if let Some(err) = Self::api_error(&data) {
            return Err(AppError::Http(err));
        }
        let info = data
            .data
            .as_ref()
            .and_then(parse_growth_info)
            .ok_or_else(|| AppError::Http("读取夸克签到进度失败".to_string()))?;

        if info.sign_daily {
            return Ok(QuarkSigninResult {
                signed: false,
                already_signed: true,
                daily_reward_bytes: info.sign_daily_reward_bytes,
                total_capacity_bytes: info.total_capacity_bytes,
                sign_reward_bytes: info.sign_reward_bytes,
                member_type: info.member_type,
                sign_progress: info.sign_progress,
                sign_target: info.sign_target,
            });
        }

        let signed = self
            .mobile_post(
                "/capacity/growth/sign",
                &params,
                &serde_json::json!({"sign_cyclic": true}),
            )
            .await?;
        if let Some(err) = Self::api_error(&signed) {
            return Err(AppError::Http(err));
        }
        let daily_reward_bytes = signed
            .data
            .as_ref()
            .and_then(|data| data.get("sign_daily_reward"))
            .and_then(value_as_i64)
            .unwrap_or(info.sign_daily_reward_bytes);

        Ok(QuarkSigninResult {
            signed: true,
            already_signed: false,
            daily_reward_bytes,
            total_capacity_bytes: info.total_capacity_bytes,
            sign_reward_bytes: info.sign_reward_bytes,
            member_type: info.member_type,
            sign_progress: info.sign_progress + 1,
            sign_target: info.sign_target,
        })
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

fn cookie_value(cookie: &str, key: &str) -> Option<String> {
    cookie.split([';', '&']).find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if name.trim() == key {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

fn mobile_param_value(input: &str, key: &str) -> Option<String> {
    url_query_value(input.trim(), key)
        .or_else(|| {
            input.split(';').find_map(|part| {
                let (name, value) = part.trim().split_once('=')?;
                if name.trim() == "url" {
                    url_query_value(value.trim(), key)
                } else {
                    None
                }
            })
        })
        .or_else(|| cookie_value(input, key))
        .map(|value| value.replace("%25", "%"))
        .filter(|value| !value.is_empty())
}

fn url_query_value(value: &str, key: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    url.query_pairs()
        .find_map(|(name, value)| (name == key).then(|| value.into_owned()))
}

fn parse_growth_info(data: &Value) -> Option<QuarkGrowthInfo> {
    let cap_sign = data.get("cap_sign")?;
    let cap_composition = data.get("cap_composition");
    Some(QuarkGrowthInfo {
        total_capacity_bytes: data
            .get("total_capacity")
            .and_then(value_as_i64)
            .unwrap_or(0),
        sign_reward_bytes: cap_composition
            .and_then(|value| value.get("sign_reward"))
            .and_then(value_as_i64)
            .unwrap_or(0),
        member_type: data
            .get("member_type")
            .and_then(|value| value.as_str())
            .unwrap_or("NORMAL")
            .to_string(),
        sign_daily: cap_sign
            .get("sign_daily")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        sign_daily_reward_bytes: cap_sign
            .get("sign_daily_reward")
            .and_then(value_as_i64)
            .unwrap_or(0),
        sign_progress: cap_sign
            .get("sign_progress")
            .and_then(value_as_i64)
            .unwrap_or(0),
        sign_target: cap_sign
            .get("sign_target")
            .and_then(value_as_i64)
            .unwrap_or(0),
    })
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_f64().map(|value| value as i64))
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

    #[test]
    fn test_mobile_params_extracts_required_cookie_values() {
        let client = QuarkSaveClient::new("foo=1; kps=abc; sign=a%252Bb; vcode=xyz;");
        let params = client.mobile_params().unwrap();

        assert_eq!(params.kps, "abc");
        assert_eq!(params.sign, "a%2Bb");
        assert_eq!(params.vcode, "xyz");
    }

    #[test]
    fn test_mobile_params_extracts_required_url_values() {
        let client = QuarkSaveClient::new(
            "https://drive-m.quark.cn/1/clouddrive/act/growth/reward?kps=abc&sign=a%252Bb&vcode=xyz",
        );
        let params = client.mobile_params().unwrap();

        assert_eq!(params.kps, "abc");
        assert_eq!(params.sign, "a%2Bb");
        assert_eq!(params.vcode, "xyz");

        let client = QuarkSaveClient::new(
            "user=张三; url=https://drive-m.quark.cn/1/clouddrive/act/growth/reward?kps=def&sign=sig&vcode=456;",
        );
        let params = client.mobile_params().unwrap();

        assert_eq!(params.kps, "def");
        assert_eq!(params.sign, "sig");
        assert_eq!(params.vcode, "456");
    }

    #[test]
    fn test_parse_growth_info() {
        let data = serde_json::json!({
            "member_type": "SUPER_VIP",
            "total_capacity": 1024,
            "cap_composition": {"sign_reward": 512},
            "cap_sign": {
                "sign_daily": true,
                "sign_daily_reward": 1048576,
                "sign_progress": 3,
                "sign_target": 7
            }
        });

        let info = parse_growth_info(&data).unwrap();
        assert_eq!(info.member_type, "SUPER_VIP");
        assert_eq!(info.total_capacity_bytes, 1024);
        assert_eq!(info.sign_reward_bytes, 512);
        assert!(info.sign_daily);
        assert_eq!(info.sign_daily_reward_bytes, 1048576);
        assert_eq!(info.sign_progress, 3);
        assert_eq!(info.sign_target, 7);
    }
}
