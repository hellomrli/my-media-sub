use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::error::{AppError, Result};
use crate::models::{MediaMetadata, MetadataProvider};
use crate::store::SettingsStore;

pub struct MetadataService {
    client: Client,
}

impl MetadataService {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("metadata HTTP client");
        Self { client }
    }

    pub async fn search(
        &self,
        settings_store: &SettingsStore,
        query: &str,
        media_type: Option<&str>,
    ) -> Result<Vec<MediaMetadata>> {
        let query = query.trim();
        if query.is_empty() {
            return Err(AppError::Validation("搜索关键词不能为空".to_string()));
        }

        let settings = settings_store.get().await;
        match settings.metadata_provider.as_str() {
            "tmdb" => {
                if settings.tmdb_api_key.trim().is_empty() {
                    return Err(AppError::Config("未配置 TMDB API Key".to_string()));
                }
                self.search_tmdb(
                    &settings.tmdb_api_key,
                    &settings.tmdb_language,
                    query,
                    media_type,
                )
                .await
            }
            "douban" => Err(AppError::Config(
                "豆瓣刮削适配器尚未启用，请先使用 TMDB".to_string(),
            )),
            "none" | "" => Ok(Vec::new()),
            other => Err(AppError::Config(format!("不支持的元数据提供方: {}", other))),
        }
    }

    async fn search_tmdb(
        &self,
        api_key: &str,
        language: &str,
        query: &str,
        media_type: Option<&str>,
    ) -> Result<Vec<MediaMetadata>> {
        let endpoint = match media_type {
            Some("movie") => "https://api.themoviedb.org/3/search/movie",
            Some("series") | Some("anime") => "https://api.themoviedb.org/3/search/tv",
            _ => "https://api.themoviedb.org/3/search/multi",
        };

        let response = self
            .client
            .get(endpoint)
            .query(&[
                ("api_key", api_key),
                ("language", language),
                ("query", query),
                ("include_adult", "false"),
                ("page", "1"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::Http(format!(
                "TMDB 搜索失败: HTTP {}",
                response.status()
            )));
        }

        let data: TmdbSearchResponse = response.json().await?;
        Ok(data
            .results
            .into_iter()
            .filter_map(|item| item.into_metadata())
            .take(10)
            .collect())
    }
}

#[derive(Debug, Deserialize)]
struct TmdbSearchResponse {
    #[serde(default)]
    results: Vec<TmdbSearchItem>,
}

#[derive(Debug, Deserialize)]
struct TmdbSearchItem {
    id: i64,
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    original_title: Option<String>,
    #[serde(default)]
    original_name: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    backdrop_path: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    first_air_date: Option<String>,
    #[serde(default)]
    vote_average: Option<f32>,
}

impl TmdbSearchItem {
    fn into_metadata(self) -> Option<MediaMetadata> {
        let tmdb_type = self.media_type.unwrap_or_else(|| {
            if self.name.is_some() || self.first_air_date.is_some() {
                "tv".to_string()
            } else {
                "movie".to_string()
            }
        });

        if tmdb_type == "person" {
            return None;
        }

        let media_type = match tmdb_type.as_str() {
            "movie" => "movie",
            "tv" => "series",
            _ => "series",
        }
        .to_string();

        let title = self.title.or(self.name)?;
        Some(MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: self.id.to_string(),
            title,
            original_title: self
                .original_title
                .or(self.original_name)
                .unwrap_or_default(),
            media_type,
            overview: self.overview.unwrap_or_default(),
            poster_url: tmdb_image_url(self.poster_path),
            backdrop_url: tmdb_image_url(self.backdrop_path),
            release_date: self.release_date.or(self.first_air_date),
            vote_average: self.vote_average,
        })
    }
}

fn tmdb_image_url(path: Option<String>) -> Option<String> {
    path.filter(|value| !value.trim().is_empty())
        .map(|value| format!("https://image.tmdb.org/t/p/w500{}", value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmdb_item_into_metadata() {
        let item = TmdbSearchItem {
            id: 1399,
            media_type: Some("tv".to_string()),
            title: None,
            name: Some("Game of Thrones".to_string()),
            original_title: None,
            original_name: Some("Game of Thrones".to_string()),
            overview: Some("overview".to_string()),
            poster_path: Some("/poster.jpg".to_string()),
            backdrop_path: None,
            release_date: None,
            first_air_date: Some("2011-04-17".to_string()),
            vote_average: Some(8.4),
        };

        let metadata = item.into_metadata().unwrap();
        assert_eq!(metadata.provider, MetadataProvider::Tmdb);
        assert_eq!(metadata.media_type, "series");
        assert_eq!(
            metadata.poster_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w500/poster.jpg")
        );
    }
}
