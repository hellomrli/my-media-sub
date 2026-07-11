use crate::clients::http_pool::ObservedRequestBuilder;
use crate::error::{AppError, Result};
use crate::models::{Settings, Subscription};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MediaLibraryRefreshReport {
    pub provider: String,
    pub success: bool,
    pub status: Option<u16>,
    pub error: Option<String>,
}

pub async fn refresh_media_library(
    settings: &Settings,
    sub: &Subscription,
    reason: &str,
) -> Option<MediaLibraryRefreshReport> {
    if !settings.media_library_refresh_enabled
        || settings.media_library_refresh_url.trim().is_empty()
    {
        return None;
    }
    let provider = settings.media_library_type.trim().to_ascii_lowercase();
    let client = crate::clients::http_pool::short_client();
    let mut request = if provider == "plex" {
        client.get(settings.media_library_refresh_url.trim())
    } else {
        client.post(settings.media_library_refresh_url.trim()).json(&serde_json::json!({
            "event": "media_library.refresh", "subscription_id": sub.id, "title": sub.title, "reason": reason
        }))
    };
    let token = settings.media_library_token.trim();
    if !token.is_empty() {
        request = match provider.as_str() {
            "jellyfin" | "emby" => request.header("X-Emby-Token", token),
            "plex" => request.query(&[("X-Plex-Token", token)]),
            _ => request.bearer_auth(token),
        };
    }
    match request.send_observed("media_library").await {
        Ok(response) if response.status().is_success() => Some(MediaLibraryRefreshReport {
            provider,
            success: true,
            status: Some(response.status().as_u16()),
            error: None,
        }),
        Ok(response) => Some(MediaLibraryRefreshReport {
            provider,
            success: false,
            status: Some(response.status().as_u16()),
            error: Some(format!("媒体库刷新返回 {}", response.status())),
        }),
        Err(error) => Some(MediaLibraryRefreshReport {
            provider,
            success: false,
            status: None,
            error: Some(format!("媒体库刷新请求失败: {error}")),
        }),
    }
}

pub fn validate_media_library_settings(settings: &Settings) -> Result<()> {
    if settings.media_library_refresh_enabled
        && settings.media_library_refresh_url.trim().is_empty()
    {
        return Err(AppError::Validation(
            "启用媒体库刷新时必须配置 URL".to_string(),
        ));
    }
    if !matches!(
        settings
            .media_library_type
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "jellyfin" | "emby" | "plex" | "webhook"
    ) {
        return Err(AppError::Validation(
            "媒体库类型必须为 jellyfin/emby/plex/webhook".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_library_configuration_is_safe_and_compatible() {
        let mut settings = Settings::default();
        assert!(validate_media_library_settings(&settings).is_ok());
        settings.media_library_refresh_enabled = true;
        assert!(validate_media_library_settings(&settings).is_err());
        settings.media_library_refresh_url = "https://media.example/refresh".to_string();
        for kind in ["jellyfin", "emby", "plex", "webhook"] {
            settings.media_library_type = kind.to_string();
            assert!(validate_media_library_settings(&settings).is_ok());
        }
        settings.media_library_type = "unsupported".to_string();
        assert!(validate_media_library_settings(&settings).is_err());
    }
}
