use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tokio_stream::StreamExt;

use crate::clients::http_pool::ObservedRequestBuilder;
use crate::clients::{ensure_upstream_status, http_pool};
use crate::error::{AppError, Result};

const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 256;
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
struct CachedImage {
    content_type: String,
    body: Vec<u8>,
}

struct ImageProxyState {
    cache: RwLock<HashMap<String, CachedImage>>,
}

impl ImageProxyState {
    fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }
}

fn validate_tmdb_image_path(size: &str, file_name: &str) -> Result<()> {
    const SIZES: &[&str] = &[
        "w92", "w154", "w185", "w300", "w342", "w400", "w500", "w780", "w1280", "original",
    ];
    if !SIZES.contains(&size) {
        return Err(AppError::Validation("不支持的 TMDB 图片尺寸".to_string()));
    }
    if file_name.is_empty() || file_name.len() > 160 || file_name.contains('/') {
        return Err(AppError::Validation("TMDB 图片文件名不合法".to_string()));
    }
    let Some((stem, extension)) = file_name.rsplit_once('.') else {
        return Err(AppError::Validation("TMDB 图片缺少扩展名".to_string()));
    };
    if stem.is_empty()
        || !stem
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        || !matches!(
            extension.to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "avif"
        )
    {
        return Err(AppError::Validation("TMDB 图片文件名不合法".to_string()));
    }
    Ok(())
}

fn image_response(image: CachedImage, cache_status: &'static str) -> Result<Response> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, image.content_type)
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=2592000, immutable"),
        )
        .header("x-media-sub-image-cache", cache_status)
        .body(Body::from(image.body))
        .map_err(|error| AppError::Internal(format!("构建图片代理响应失败: {error}")))
}

async fn tmdb_image(
    State(state): State<Arc<ImageProxyState>>,
    Path((size, file_name)): Path<(String, String)>,
) -> Result<Response> {
    validate_tmdb_image_path(&size, &file_name)?;
    let cache_key = format!("{size}/{file_name}");
    if let Some(image) = state.cache.read().await.get(&cache_key).cloned() {
        return image_response(image, "hit");
    }

    let url = format!("https://image.tmdb.org/t/p/{size}/{file_name}");
    let upstream = http_pool::medium_client()
        .get(&url)
        .header(
            reqwest::header::ACCEPT,
            "image/avif,image/webp,image/png,image/jpeg",
        )
        .send_observed("tmdb_image")
        .await
        .map_err(|error| AppError::Http(format!("TMDB 图片请求失败: {error}")))?;
    ensure_upstream_status(&upstream, "TMDB 图片")?;

    if upstream
        .content_length()
        .is_some_and(|length| length > MAX_IMAGE_BYTES as u64)
    {
        return Err(AppError::Validation("TMDB 图片超过大小限制".to_string()));
    }
    let content_type = upstream
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| {
            matches!(
                *value,
                "image/jpeg" | "image/png" | "image/webp" | "image/avif"
            )
        })
        .ok_or_else(|| AppError::Http("TMDB 返回了非图片内容".to_string()))?
        .to_string();
    let mut body = Vec::new();
    let mut stream = upstream.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if body.len().saturating_add(chunk.len()) > MAX_IMAGE_BYTES {
            return Err(AppError::Validation("TMDB 图片超过大小限制".to_string()));
        }
        body.extend_from_slice(&chunk);
    }
    if body.is_empty() {
        return Err(AppError::Validation("TMDB 图片大小不合法".to_string()));
    }

    let image = CachedImage { content_type, body };
    let mut cache = state.cache.write().await;
    while !cache.contains_key(&cache_key)
        && (cache.len() >= MAX_CACHE_ENTRIES
            || cache
                .values()
                .map(|cached| cached.body.len())
                .sum::<usize>()
                .saturating_add(image.body.len())
                > MAX_CACHE_BYTES)
    {
        if let Some(eviction_key) = cache.keys().next().cloned() {
            cache.remove(&eviction_key);
        } else {
            break;
        }
    }
    cache.insert(cache_key, image.clone());
    drop(cache);
    image_response(image, "miss")
}

pub fn routes() -> Router {
    Router::new()
        .route("/api/images/tmdb/{size}/{file_name}", get(tmdb_image))
        .with_state(Arc::new(ImageProxyState::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_known_tmdb_image_paths() {
        assert!(validate_tmdb_image_path("w500", "Abc_123-def.jpg").is_ok());
        assert!(validate_tmdb_image_path("original", "poster.webp").is_ok());
    }

    #[test]
    fn rejects_paths_that_could_escape_the_tmdb_image_origin() {
        assert!(validate_tmdb_image_path("w999", "poster.jpg").is_err());
        assert!(validate_tmdb_image_path("w500", "../poster.jpg").is_err());
        assert!(validate_tmdb_image_path("w500", "poster.svg").is_err());
    }
}
