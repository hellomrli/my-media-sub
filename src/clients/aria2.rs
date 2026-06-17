use crate::error::{AppError, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

pub struct Aria2Client {
    rpc_url: String,
    secret: String,
    dir: String,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct Aria2Response {
    result: Option<String>,
    error: Option<Aria2Error>,
}

#[derive(Debug, Deserialize)]
struct Aria2Error {
    code: i64,
    message: String,
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
        Self {
            rpc_url: rpc_url.into(),
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
        let response = self
            .client
            .post(self.rpc_url.trim())
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("提交 Aria2 任务失败: {}", e)))?;

        let data: Aria2Response = response
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析 Aria2 响应失败: {}", e)))?;

        if let Some(error) = data.error {
            return Err(AppError::Http(format!(
                "Aria2 返回错误 {}: {}",
                error.code, error.message
            )));
        }

        data.result
            .filter(|gid| !gid.trim().is_empty())
            .ok_or_else(|| AppError::Http("Aria2 响应缺少任务 GID".to_string()))
    }
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

    let mut params = Vec::new();
    if !secret.is_empty() {
        params.push(json!(format!("token:{}", secret)));
    }
    params.push(json!([uri]));
    params.push(Value::Object(options));

    json!({
        "jsonrpc": "2.0",
        "id": "my-media-sub",
        "method": "aria2.addUri",
        "params": params,
    })
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
}
