use crate::error::{AppError, Result};
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

pub struct Aria2Client {
    rpc_url: String,
    secret: String,
    dir: String,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct Aria2Response<T> {
    result: Option<T>,
    error: Option<Aria2Error>,
}

#[derive(Debug, Deserialize)]
struct Aria2Error {
    code: i64,
    message: String,
}

#[derive(Debug, Serialize)]
pub struct Aria2TaskList {
    pub active: Vec<Aria2Task>,
    pub waiting: Vec<Aria2Task>,
    pub stopped: Vec<Aria2Task>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Aria2Version {
    pub version: String,
    #[serde(default, rename = "enabledFeatures")]
    pub enabled_features: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Aria2Task {
    pub gid: String,
    pub status: String,
    pub total_length: u64,
    pub completed_length: u64,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub connections: u64,
    pub dir: String,
    pub file_name: String,
    pub error_code: String,
    pub error_message: String,
    pub progress: f64,
    pub eta_seconds: Option<u64>,
    pub files: Vec<Aria2TaskFile>,
}

#[derive(Debug, Serialize)]
pub struct Aria2TaskFile {
    pub index: String,
    pub path: String,
    pub file_name: String,
    pub length: u64,
    pub completed_length: u64,
    pub selected: bool,
}

#[derive(Debug, Deserialize)]
struct RawAria2Task {
    #[serde(default)]
    gid: String,
    #[serde(default)]
    status: String,
    #[serde(default, rename = "totalLength")]
    total_length: String,
    #[serde(default, rename = "completedLength")]
    completed_length: String,
    #[serde(default, rename = "downloadSpeed")]
    download_speed: String,
    #[serde(default, rename = "uploadSpeed")]
    upload_speed: String,
    #[serde(default)]
    connections: String,
    #[serde(default)]
    dir: String,
    #[serde(default, rename = "errorCode")]
    error_code: String,
    #[serde(default, rename = "errorMessage")]
    error_message: String,
    #[serde(default)]
    files: Vec<RawAria2TaskFile>,
}

#[derive(Debug, Deserialize)]
struct RawAria2TaskFile {
    #[serde(default)]
    index: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    length: String,
    #[serde(default, rename = "completedLength")]
    completed_length: String,
    #[serde(default)]
    selected: String,
    #[serde(default)]
    uris: Vec<RawAria2TaskUri>,
}

#[derive(Debug, Deserialize)]
struct RawAria2TaskUri {
    #[serde(default)]
    uri: String,
}

impl Aria2Client {
    pub fn new(
        rpc_url: impl Into<String>,
        secret: impl Into<String>,
        dir: impl Into<String>,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();
        let rpc_url = rpc_url.into();
        Self {
            rpc_url: normalize_rpc_url(&rpc_url),
            secret: secret.into(),
            dir: dir.into(),
            client,
        }
    }

    pub async fn add_uri(
        &self,
        uri: &str,
        output_name: Option<&str>,
        headers: &[String],
    ) -> Result<String> {
        if self.rpc_url.trim().is_empty() {
            return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
        }
        if uri.trim().is_empty() {
            return Err(AppError::Validation("下载地址为空".to_string()));
        }

        let payload = build_add_uri_payload(
            uri,
            self.secret.trim(),
            self.dir.trim(),
            output_name,
            headers,
        );
        let gid: Option<String> = self.send_payload(payload).await?;
        gid.filter(|gid| !gid.trim().is_empty())
            .ok_or_else(|| AppError::Http("Aria2 响应缺少任务 GID".to_string()))
    }

    pub async fn list_tasks(&self, stopped_limit: u64) -> Result<Aria2TaskList> {
        if self.rpc_url.trim().is_empty() {
            return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
        }

        let keys = json!(task_status_keys());
        let active: Vec<RawAria2Task> = self
            .call_rpc("aria2.tellActive", vec![keys.clone()])
            .await?;
        let waiting: Vec<RawAria2Task> = self
            .call_rpc(
                "aria2.tellWaiting",
                vec![json!(0), json!(100), keys.clone()],
            )
            .await?;
        let stopped: Vec<RawAria2Task> = self
            .call_rpc(
                "aria2.tellStopped",
                vec![json!(0), json!(stopped_limit), keys],
            )
            .await?;

        Ok(Aria2TaskList {
            active: active.into_iter().map(Into::into).collect(),
            waiting: waiting.into_iter().map(Into::into).collect(),
            stopped: stopped.into_iter().map(Into::into).collect(),
        })
    }

    pub async fn get_version(&self) -> Result<Aria2Version> {
        if self.rpc_url.trim().is_empty() {
            return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
        }

        self.call_rpc("aria2.getVersion", Vec::new()).await
    }

    pub async fn pause(&self, gid: &str) -> Result<String> {
        self.call_gid_rpc("aria2.pause", gid).await
    }

    pub async fn unpause(&self, gid: &str) -> Result<String> {
        self.call_gid_rpc("aria2.unpause", gid).await
    }

    pub async fn force_remove(&self, gid: &str) -> Result<String> {
        self.call_gid_rpc("aria2.forceRemove", gid).await
    }

    pub async fn remove_download_result(&self, gid: &str) -> Result<String> {
        self.call_gid_rpc("aria2.removeDownloadResult", gid).await
    }

    pub async fn pause_all(&self) -> Result<String> {
        if self.rpc_url.trim().is_empty() {
            return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
        }

        self.call_rpc("aria2.pauseAll", Vec::new()).await
    }

    async fn call_gid_rpc(&self, method: &str, gid: &str) -> Result<String> {
        if self.rpc_url.trim().is_empty() {
            return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
        }
        let gid = gid.trim();
        if gid.is_empty() {
            return Err(AppError::Validation("Aria2 任务 GID 为空".to_string()));
        }

        self.call_rpc(method, vec![json!(gid)]).await
    }

    async fn call_rpc<T>(&self, method: &str, params: Vec<Value>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let payload = build_rpc_payload(method, self.secret.trim(), params);
        self.send_payload(payload)
            .await?
            .ok_or_else(|| AppError::Http(format!("Aria2 响应缺少结果: {}", method)))
    }

    async fn send_payload<T>(&self, payload: Value) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let response = self
            .client
            .post(self.rpc_url.trim())
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("请求 Aria2 失败: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let detail = body.trim();
            return Err(AppError::Http(if detail.is_empty() {
                format!("Aria2 HTTP 状态异常: {}", status)
            } else {
                format!("Aria2 HTTP 状态异常: {} {}", status, detail)
            }));
        }

        let data: Aria2Response<T> = response
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析 Aria2 响应失败: {}", e)))?;

        if let Some(error) = data.error {
            return Err(AppError::Http(format!(
                "Aria2 返回错误 {}: {}",
                error.code, error.message
            )));
        }

        Ok(data.result)
    }
}

fn normalize_rpc_url(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Ok(mut url) = reqwest::Url::parse(trimmed) {
        if url.path().is_empty() || url.path() == "/" {
            url.set_path("jsonrpc");
            return url.to_string();
        }
    }

    trimmed.to_string()
}

fn build_add_uri_payload(
    uri: &str,
    secret: &str,
    dir: &str,
    output_name: Option<&str>,
    headers: &[String],
) -> Value {
    let mut options = serde_json::Map::new();
    if !dir.is_empty() {
        options.insert("dir".to_string(), json!(dir));
    }
    if let Some(name) = output_name.map(str::trim).filter(|name| !name.is_empty()) {
        options.insert("out".to_string(), json!(name));
    }
    if !headers.is_empty() {
        options.insert("header".to_string(), json!(headers));
    }

    let params = vec![json!([uri]), Value::Object(options)];

    build_rpc_payload("aria2.addUri", secret, params)
}

fn build_rpc_payload(method: &str, secret: &str, params: Vec<Value>) -> Value {
    let mut rpc_params = Vec::new();
    if !secret.is_empty() {
        rpc_params.push(json!(format!("token:{}", secret)));
    }
    rpc_params.extend(params);

    json!({
        "jsonrpc": "2.0",
        "id": "my-media-sub",
        "method": method,
        "params": rpc_params,
    })
}

fn task_status_keys() -> Vec<&'static str> {
    vec![
        "gid",
        "status",
        "totalLength",
        "completedLength",
        "downloadSpeed",
        "uploadSpeed",
        "connections",
        "dir",
        "errorCode",
        "errorMessage",
        "files",
    ]
}

impl From<RawAria2Task> for Aria2Task {
    fn from(raw: RawAria2Task) -> Self {
        let total_length = parse_u64(&raw.total_length);
        let completed_length = parse_u64(&raw.completed_length);
        let download_speed = parse_u64(&raw.download_speed);
        let progress = calculate_progress(completed_length, total_length);
        let eta_seconds =
            calculate_eta(completed_length, total_length, download_speed, &raw.status);

        let fallback_uri = raw
            .files
            .iter()
            .flat_map(|file| file.uris.iter())
            .map(|uri| uri.uri.as_str())
            .find(|uri| !uri.trim().is_empty())
            .unwrap_or_default()
            .to_string();
        let files: Vec<Aria2TaskFile> = raw.files.into_iter().map(Into::into).collect();
        let file_name = files
            .iter()
            .map(|file| file.file_name.as_str())
            .find(|name| !name.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| file_name_from_uri(&fallback_uri));

        Self {
            gid: raw.gid,
            status: raw.status,
            total_length,
            completed_length,
            download_speed,
            upload_speed: parse_u64(&raw.upload_speed),
            connections: parse_u64(&raw.connections),
            dir: raw.dir,
            file_name,
            error_code: raw.error_code,
            error_message: raw.error_message,
            progress,
            eta_seconds,
            files,
        }
    }
}

impl From<RawAria2TaskFile> for Aria2TaskFile {
    fn from(raw: RawAria2TaskFile) -> Self {
        let file_name = file_name_from_path(&raw.path);

        Self {
            index: raw.index,
            path: raw.path,
            file_name,
            length: parse_u64(&raw.length),
            completed_length: parse_u64(&raw.completed_length),
            selected: parse_bool(&raw.selected),
        }
    }
}

fn parse_u64(value: &str) -> u64 {
    value.trim().parse().unwrap_or(0)
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "true" | "1")
}

fn calculate_progress(completed_length: u64, total_length: u64) -> f64 {
    if total_length == 0 {
        return 0.0;
    }
    let progress = completed_length as f64 / total_length as f64 * 100.0;
    progress.clamp(0.0, 100.0)
}

fn calculate_eta(
    completed_length: u64,
    total_length: u64,
    download_speed: u64,
    status: &str,
) -> Option<u64> {
    if status != "active" || download_speed == 0 || total_length <= completed_length {
        return None;
    }
    Some((total_length - completed_length).div_ceil(download_speed))
}

fn file_name_from_path(path: &str) -> String {
    path.trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_string()
}

fn file_name_from_uri(uri: &str) -> String {
    uri.trim()
        .split('?')
        .next()
        .unwrap_or_default()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_add_uri_payload_with_secret_and_headers() {
        let payload = build_add_uri_payload(
            "https://example.com/a.mkv",
            "secret",
            "/downloads",
            Some("a.mkv"),
            &[
                "Cookie: a=b".to_string(),
                "Referer: https://pan.quark.cn/".to_string(),
            ],
        );

        assert_eq!(payload["method"], json!("aria2.addUri"));
        assert_eq!(payload["params"][0], json!("token:secret"));
        assert_eq!(payload["params"][1][0], json!("https://example.com/a.mkv"));
        assert_eq!(payload["params"][2]["dir"], json!("/downloads"));
        assert_eq!(payload["params"][2]["out"], json!("a.mkv"));
        assert_eq!(payload["params"][2]["header"][0], json!("Cookie: a=b"));
    }

    #[test]
    fn test_build_add_uri_payload_without_secret() {
        let payload = build_add_uri_payload("https://example.com/a.mkv", "", "", None, &[]);

        assert_eq!(payload["params"][0][0], json!("https://example.com/a.mkv"));
        assert!(payload["params"][1].as_object().unwrap().is_empty());
    }

    #[test]
    fn test_build_rpc_payload_with_secret() {
        let payload = build_rpc_payload("aria2.tellActive", "secret", vec![json!(["gid"])]);

        assert_eq!(payload["method"], json!("aria2.tellActive"));
        assert_eq!(payload["params"][0], json!("token:secret"));
        assert_eq!(payload["params"][1][0], json!("gid"));
    }

    #[test]
    fn test_normalize_rpc_url_adds_jsonrpc_to_root() {
        assert_eq!(
            normalize_rpc_url("http://192.168.50.100:6800"),
            "http://192.168.50.100:6800/jsonrpc"
        );
        assert_eq!(
            normalize_rpc_url("http://192.168.50.100:6800/"),
            "http://192.168.50.100:6800/jsonrpc"
        );
        assert_eq!(
            normalize_rpc_url("http://192.168.50.100:6800/jsonrpc"),
            "http://192.168.50.100:6800/jsonrpc"
        );
    }

    #[test]
    fn test_convert_raw_task_calculates_progress_and_eta() {
        let task = Aria2Task::from(RawAria2Task {
            gid: "gid-1".to_string(),
            status: "active".to_string(),
            total_length: "100".to_string(),
            completed_length: "40".to_string(),
            download_speed: "10".to_string(),
            upload_speed: "0".to_string(),
            connections: "2".to_string(),
            dir: "/downloads".to_string(),
            error_code: String::new(),
            error_message: String::new(),
            files: vec![RawAria2TaskFile {
                index: "1".to_string(),
                path: "/downloads/movie.mkv".to_string(),
                length: "100".to_string(),
                completed_length: "40".to_string(),
                selected: "true".to_string(),
                uris: vec![
                    RawAria2TaskUri {
                        uri: "https://example.com/movie.mkv?token=1".to_string(),
                    },
                    RawAria2TaskUri {
                        uri: "https://example.com/movie.mkv?token=1".to_string(),
                    },
                ],
            }],
        });

        assert_eq!(task.file_name, "movie.mkv");
        assert_eq!(task.progress, 40.0);
        assert_eq!(task.eta_seconds, Some(6));
        assert_eq!(task.connections, 2);
        assert!(task.files[0].selected);
    }
}
