use crate::error::{AppError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use super::quark::QuarkClient;

const QUARK_API_BASE: &str = "https://drive.quark.cn/1/clouddrive";

/// 夸克文件项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkItem {
    pub fid: String,
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
}

/// 夸克转存客户端
pub struct QuarkSaveClient {
    client: Client,
    cookie: String,
}

impl QuarkSaveClient {
    /// 创建新的转存客户端
    pub fn new(cookie: String) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()
            .unwrap();
        
        Self { client, cookie }
    }

    /// 通用 POST 请求
    async fn post(&self, path: &str, payload: &Value) -> Result<Value> {
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

        let json = resp.json::<Value>()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;

        Ok(json)
    }

    /// 通用 GET 请求
    async fn get(&self, path: &str, params: &HashMap<&str, String>) -> Result<Value> {
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

        let json = resp.json::<Value>()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;

        Ok(json)
    }

    /// 检查 API 错误
    fn check_error(data: &Value) -> Result<()> {
        let code = data["code"].as_i64().unwrap_or(0);
        if code != 0 {
            let msg = data["message"].as_str()
                .or(data["msg"].as_str())
                .unwrap_or("API 请求失败");
            return Err(AppError::Http(msg.to_string()));
        }
        Ok(())
    }

    /// 列出目录
    pub async fn list_dir(&self, parent_fid: &str) -> Result<Vec<QuarkItem>> {
        let mut params = HashMap::new();
        params.insert("pdir_fid", parent_fid.to_string());
        params.insert("_page", "1".to_string());
        params.insert("_size", "200".to_string());
        params.insert("_fetch_total", "1".to_string());
        params.insert("fetch_all_file", "1".to_string());
        params.insert("_sort", "file_type:asc,file_name:asc".to_string());

        let data = self.get("/file/sort", &params).await?;
        Self::check_error(&data)?;

        let list = data["data"]["list"].as_array()
            .ok_or_else(|| AppError::Database("文件列表格式错误".to_string()))?;

        let mut items = Vec::new();
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
                || item["file_type"].as_i64() == Some(0);

            items.push(QuarkItem {
                fid,
                name,
                is_dir,
                size: item["size"].as_i64().unwrap_or(0),
            });
        }

        Ok(items)
    }

    /// 创建目录
    pub async fn create_dir(&self, parent_fid: &str, name: &str) -> Result<String> {
        let payload = serde_json::json!({
            "pdir_fid": parent_fid,
            "file_name": name,
            "dir_path": "",
            "dir_init_lock": false
        });

        let data = self.post("/file", &payload).await?;
        Self::check_error(&data)?;

        let fid = data["data"]["fid"].as_str()
            .or(data["data"]["file_id"].as_str())
            .ok_or_else(|| AppError::Database("无法获取创建的目录 fid".to_string()))?;

        Ok(fid.to_string())
    }

    /// 确保目录路径存在
    pub async fn ensure_dir_path(&self, path: &str) -> Result<String> {
        let mut parent_fid = "0".to_string();
        
        for part in path.trim_matches('/').split('/').filter(|p| !p.is_empty()) {
            let items = self.list_dir(&parent_fid).await?;
            
            let found = items.iter()
                .find(|item| item.is_dir && item.name == part)
                .map(|item| item.fid.clone());

            parent_fid = match found {
                Some(fid) => fid,
                None => self.create_dir(&parent_fid, part).await?,
            };
        }

        Ok(parent_fid)
    }

    /// 转存分享文件
    pub async fn save_share_files(
        &self,
        pwd_id: &str,
        stoken: &str,
        fid_list: Vec<String>,
        fid_token_list: Vec<String>,
        to_pdir_fid: &str,
    ) -> Result<Value> {
        let payload = serde_json::json!({
            "fid_list": fid_list,
            "fid_token_list": fid_token_list,
            "to_pdir_fid": to_pdir_fid,
            "pwd_id": pwd_id,
            "stoken": stoken
        });

        let data = self.post("/share/sharepage/save", &payload).await?;
        Self::check_error(&data)?;

        Ok(data)
    }
}
