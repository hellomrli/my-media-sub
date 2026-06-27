use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

use crate::clients::QuarkSaveClient;
use crate::error::{AppError, Result};
use crate::store::SettingsStore;
use crate::utils::constant_time_eq;

pub struct StrmState {
    pub settings_store: Arc<SettingsStore>,
}

#[derive(Debug, Deserialize)]
struct StrmQuery {
    #[serde(default)]
    token: String,
}

async fn quark_httpstrm(
    State(state): State<Arc<StrmState>>,
    Path((fid, _file_name)): Path<(String, String)>,
    Query(query): Query<StrmQuery>,
    headers: HeaderMap,
) -> Result<Response> {
    let settings = state.settings_store.get().await;
    let expected_token = settings.strm_access_token.trim();
    let provided_token = strm_token_from_headers(&headers).unwrap_or_else(|| query.token.trim());
    if expected_token.is_empty() || !constant_time_eq(provided_token, expected_token) {
        return Ok((StatusCode::UNAUTHORIZED, "Unauthorized").into_response());
    }
    if !settings.strm_enabled {
        return Ok((StatusCode::NOT_FOUND, "STRM disabled").into_response());
    }
    if settings.quark_cookie.trim().is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }

    let quark = QuarkSaveClient::new(settings.quark_cookie);
    let infos = quark.download_infos(&[fid]).await?;
    let info = infos
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Http("未能获取夸克文件下载链接".to_string()))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| AppError::Http(format!("创建 HTTPStrm 客户端失败: {}", e)))?;
    let mut request = client.get(&info.download_url);

    for raw in &info.headers {
        if let Some((name, value)) = raw.split_once(':') {
            request = request.header(name.trim(), value.trim());
        }
    }
    request = with_request_header(request, &headers, header::RANGE);
    request = with_request_header(request, &headers, header::IF_RANGE);

    let upstream = request
        .send()
        .await
        .map_err(|e| AppError::Http(format!("HTTPStrm 获取上游响应失败: {}", e)))?;
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    let mut builder = Response::builder().status(status);
    for name in response_headers_to_forward() {
        if let Some(value) = upstream.headers().get(&name) {
            if let Ok(value) = HeaderValue::from_bytes(value.as_bytes()) {
                builder = builder.header(name, value);
            }
        }
    }

    builder
        .body(Body::from_stream(upstream.bytes_stream()))
        .map_err(|e| AppError::Internal(format!("构建 HTTPStrm 响应失败: {}", e)))
}

fn strm_token_from_headers(headers: &HeaderMap) -> Option<&str> {
    if let Some(token) = headers
        .get("x-httpstrm-token")
        .or_else(|| headers.get("x-strm-token"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(token);
    }

    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn with_request_header(
    request: reqwest::RequestBuilder,
    headers: &HeaderMap,
    name: HeaderName,
) -> reqwest::RequestBuilder {
    if let Some(value) = headers.get(&name) {
        if let Ok(value) = value.to_str() {
            return request.header(name.as_str(), value);
        }
    }
    request
}

fn response_headers_to_forward() -> Vec<HeaderName> {
    vec![
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        HeaderName::from_static("content-range"),
        HeaderName::from_static("accept-ranges"),
        header::ETAG,
        header::LAST_MODIFIED,
        header::CACHE_CONTROL,
    ]
}

pub fn routes(settings_store: Arc<SettingsStore>) -> Router {
    let state = Arc::new(StrmState { settings_store });

    Router::new()
        .route("/strm/quark/{fid}/{file_name}", get(quark_httpstrm))
        .with_state(state)
}
