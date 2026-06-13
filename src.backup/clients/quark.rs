use crate::error::{AppError, Result};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

const QUARK_API_BASE: &str = "https://drive.quark.cn/1/clouddrive";

/// 夸克分享文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkFile {
    /// 文件 ID
    pub fid: String,
    /// 文件名
    pub name: String,
    /// 是否是目录
    pub is_dir: bool,
    /// 文件大小
    pub size: i64,
    /// 分享文件 token
    #[serde(default)]
    pub share_fid_token: String,
    /// 文件类型
    #[serde(default)]
    pub format_type: Option<String>,
}

/// 夸克分享探测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkShareInfo {
    /// 是否成功
    pub ok: bool,
    /// 状态：ok/locked/invalid_url/bad/error
    pub state: String,
    /// 消息
    pub message: String,
    /// 文件列表
    pub files: Vec<QuarkFile>,
    /// 文件总数
    pub file_count: usize,
    /// 疑似集数
    pub episode_count: usize,
}

/// 夸克客户端
pub struct QuarkClient {
    client: Client,
    cookie: String,
}

impl QuarkClient {
    /// 创建新的夸克客户端
    pub fn new(cookie: String) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()
            .unwrap();
        
        Self { client, cookie }
    }

    /// 更新 Cookie（从响应头提取）
    fn update_cookie(&mut self, resp: &Response) {
        if let Some(set_cookie) = resp.headers().get("set-cookie") {
            if let Ok(cookie_str) = set_cookie.to_str() {
                // 简单解析 Set-Cookie 头
                for part in cookie_str.split(';') {
                    let part = part.trim();
                    if part.contains('=') {
                        let mut split = part.splitn(2, '=');
                        if let (Some(name), Some(value)) = (split.next(), split.next()) {
                            if name == "__puus" || name == "__pus" {
                                self.cookie = self.set_cookie_value(name, value);
                            }
                        }
                    }
                }
            }
        }
    }

    /// 设置 Cookie 值
    fn set_cookie_value(&self, name: &str, value: &str) -> String {
        let parts: Vec<&str> = self.cookie.split(';').map(|s| s.trim()).collect();
        let mut new_parts = Vec::new();
        let mut replaced = false;

        for part in parts {
            if part.starts_with(&format!("{}=", name)) {
                new_parts.push(format!("{}={}", name, value));
                replaced = true;
            } else {
                new_parts.push(part.to_string());
            }
        }

        if !replaced {
            new_parts.push(format!("{}={}", name, value));
        }

        new_parts.join("; ")
    }

    /// 提取 pwd_id
    pub fn extract_pwd_id(url: &str) -> Option<String> {
        use regex::Regex;
        let re = Regex::new(r"/s/([A-Za-z0-9_-]+)").ok()?;
        re.captures(url)?.get(1).map(|m| m.as_str().to_string())
    }

    /// POST 请求
    async fn post(&mut self, path: &str, payload: &Value) -> Result<Value> {
        let url = format!("{}{}", QUARK_API_BASE, path);
        let mut params = HashMap::new();
        params.insert("pr", "ucpro");
        params.insert("fr", "pc");

        let resp = self.client
            .post(&url)
            .query(&params)
            .header("Cookie", &self.cookie)
            .header("Accept", "application/json")
            .header("Referer", "https://pan.quark.cn/")
            .header("Origin", "https://pan.quark.cn")
            .json(payload)
            .send()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;

        self.update_cookie(&resp);

        let json = resp.json::<Value>()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;

        Ok(json)
    }

    /// GET 请求
    async fn get(&mut self, path: &str, params: &HashMap<&str, String>) -> Result<Value> {
        let url = format!("{}{}", QUARK_API_BASE, path);
        let mut full_params = HashMap::new();
        full_params.insert("pr", "ucpro".to_string());
        full_params.insert("fr", "pc".to_string());
        
        for (k, v) in params {
            full_params.insert(*k, v.clone());
        }

        let resp = self.client
            .get(&url)
            .query(&full_params)
            .header("Cookie", &self.cookie)
            .header("Accept", "application/json")
            .header("Referer", "https://pan.quark.cn/")
            .send()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;

        self.update_cookie(&resp);

        let json = resp.json::<Value>()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;

        Ok(json)
    }

    /// 获取分享 token
    pub async fn get_share_token(&mut self, pwd_id: &str, passcode: &str) -> Result<String> {
        let payload = serde_json::json!({
            "pwd_id": pwd_id,
            "passcode": passcode
        });

        let data = self.post("/share/sharepage/token", &payload).await?;

        let code = data["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = data["message"].as_str()
                .or(data["msg"].as_str())
                .unwrap_or("获取 token 失败");
            return Err(AppError::Http(msg.to_string()));
        }

        let token = data["data"]["stoken"]
            .as_str()
            .ok_or_else(|| AppError::Http("未能获取 stoken".to_string()))?;

        Ok(token.to_string())
    }

    /// 列出文件
    pub async fn list_files(
        &mut self,
        pwd_id: &str,
        stoken: &str,
        pdir_fid: &str,
    ) -> Result<Vec<QuarkFile>> {
        let mut params = HashMap::new();
        params.insert("pwd_id", pwd_id.to_string());
        params.insert("stoken", stoken.to_string());
        params.insert("pdir_fid", pdir_fid.to_string());
        params.insert("force", "0".to_string());
        params.insert("_page", "1".to_string());
        params.insert("_size", "100".to_string());
        params.insert("_fetch_total", "1".to_string());
        params.insert("_fetch_sub_dirs", "0".to_string());
        params.insert("_sort", "file_type:asc,file_name:asc".to_string());

        let data = self.get("/share/sharepage/detail", &params).await?;

        let code = data["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = data["message"].as_str()
                .or(data["msg"].as_str())
                .unwrap_or("列出文件失败");
            return Err(AppError::Http(msg.to_string()));
        }

        let list = data["data"]["list"].as_array()
            .ok_or_else(|| AppError::Http("文件列表格式错误".to_string()))?;

        let mut files = Vec::new();
        for item in list {
            let fid = item["fid"].as_str()
                .or(item["file_id"].as_str())
                .unwrap_or("")
                .to_string();
            
            let name = item["file_name"].as_str()
                .or(item["name"].as_str())
                .unwrap_or("")
                .to_string();

            let is_dir = item["dir"].as_bool().unwrap_or(false)
                || item["file"].as_bool() == Some(false)
                || (item["file_type"].as_i64() == Some(0) 
                    && item["size"].as_i64().unwrap_or(0) == 0);

            let share_fid_token = item["share_fid_token"].as_str()
                .or(item["file_token"].as_str())
                .unwrap_or("")
                .to_string();

            files.push(QuarkFile {
                fid,
                name,
                is_dir,
                size: item["size"].as_i64().unwrap_or(0),
                share_fid_token,
                format_type: item["format_type"].as_str().map(|s| s.to_string()),
            });
        }

        Ok(files)
    }

    /// 探测分享链接
    pub async fn probe(&mut self, url: &str, passcode: &str, max_files: usize) -> QuarkShareInfo {
        let pwd_id = match Self::extract_pwd_id(url) {
            Some(id) => id,
            None => return QuarkShareInfo {
                ok: false,
                state: "invalid_url".to_string(),
                message: "不是有效的夸克分享链接".to_string(),
                files: vec![],
                file_count: 0,
                episode_count: 0,
            },
        };

        // 获取 token
        let stoken = match self.get_share_token(&pwd_id, passcode).await {
            Ok(token) => token,
            Err(e) => {
                let msg = e.to_string();
                let state = if msg.contains("提取码") || msg.contains("密码") {
                    "locked"
                } else {
                    "bad"
                };
                return QuarkShareInfo {
                    ok: false,
                    state: state.to_string(),
                    message: msg,
                    files: vec![],
                    file_count: 0,
                    episode_count: 0,
                };
            }
        };

        // 递归列出所有文件
        let mut all_files = Vec::new();
        let mut queue = match self.list_files(&pwd_id, &stoken, "0").await {
            Ok(files) => files,
            Err(e) => return QuarkShareInfo {
                ok: false,
                state: "bad".to_string(),
                message: e.to_string(),
                files: vec![],
                file_count: 0,
                episode_count: 0,
            },
        };

        while !queue.is_empty() && all_files.len() < max_files {
            let item = queue.remove(0);
            let is_dir = item.is_dir;
            let fid = item.fid.clone();
            
            all_files.push(item);

            if is_dir && all_files.len() < max_files {
                if let Ok(children) = self.list_files(&pwd_id, &stoken, &fid).await {
                    queue.extend(children);
                }
            }
        }

        let episode_count = Self::count_episodes(&all_files);
        let file_count = all_files.len();

        QuarkShareInfo {
            ok: true,
            state: "ok".to_string(),
            message: "链接可访问".to_string(),
            files: all_files,
            file_count,
            episode_count,
        }
    }

    /// 统计集数
    fn count_episodes(files: &[QuarkFile]) -> usize {
        use regex::Regex;
        
        let patterns = vec![
            Regex::new(r"(?:^|[^A-Za-z])S\d{1,2}E\d{1,3}(?:[^A-Za-z]|$)").unwrap(),
            Regex::new(r"(?:第\s*\d{1,3}\s*[集话])").unwrap(),
            Regex::new(r"(?:^|[^\d])E\d{1,3}(?:[^\d]|$)").unwrap(),
            Regex::new(r"(?:^|[^\d])\d{1,3}\s*\.\s*(?:mkv|mp4|avi|ts|mov|wmv)$").unwrap(),
        ];

        let video_exts = ["mkv", "mp4", "avi", "ts", "mov", "wmv", "flv", "m4v"];

        files.iter().filter(|f| {
            let name_lower = f.name.to_lowercase();
            let is_video = video_exts.iter().any(|ext| name_lower.ends_with(ext));
            is_video && patterns.iter().any(|p| p.is_match(&f.name))
        }).count()
    }
}
