use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::error::{AppError, Result};
use crate::models::{MediaMetadata, MediaMetadataSeason, MetadataProvider};
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

    pub fn choose_best_match(
        query: &str,
        media_type: &str,
        candidates: &[MediaMetadata],
    ) -> Option<MediaMetadata> {
        candidates
            .iter()
            .filter(|item| media_type_compatible(media_type, &item.media_type))
            .max_by_key(|item| metadata_score(query, item))
            .cloned()
            .or_else(|| {
                candidates
                    .iter()
                    .max_by_key(|item| metadata_score(query, item))
                    .cloned()
            })
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
        let mut results: Vec<MediaMetadata> = data
            .results
            .into_iter()
            .filter_map(|item| {
                let mut metadata = item.into_metadata()?;
                if media_type == Some("anime") && metadata.media_type == "series" {
                    metadata.media_type = "anime".to_string();
                }
                Some(metadata)
            })
            .take(10)
            .collect();

        for item in &mut results {
            if item.media_type == "series" || item.media_type == "anime" {
                if let Ok(Some(details)) = self
                    .fetch_tmdb_tv_details(api_key, language, &item.provider_id)
                    .await
                {
                    item.number_of_episodes = details.number_of_episodes;
                    item.number_of_seasons = details.number_of_seasons;
                    item.seasons = details.seasons.into_iter().map(Into::into).collect();
                }
            }
        }

        Ok(results)
    }

    async fn fetch_tmdb_tv_details(
        &self,
        api_key: &str,
        language: &str,
        provider_id: &str,
    ) -> Result<Option<TmdbTvDetails>> {
        let endpoint = format!("https://api.themoviedb.org/3/tv/{}", provider_id);
        let response = self
            .client
            .get(endpoint)
            .query(&[("api_key", api_key), ("language", language)])
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(None);
        }

        Ok(Some(response.json().await?))
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

#[derive(Debug, Deserialize)]
struct TmdbTvDetails {
    #[serde(default)]
    number_of_episodes: Option<i32>,
    #[serde(default)]
    number_of_seasons: Option<i32>,
    #[serde(default)]
    seasons: Vec<TmdbSeason>,
}

#[derive(Debug, Deserialize)]
struct TmdbSeason {
    #[serde(default)]
    season_number: i32,
    #[serde(default)]
    episode_count: Option<i32>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    air_date: Option<String>,
    #[serde(default)]
    poster_path: Option<String>,
}

impl From<TmdbSeason> for MediaMetadataSeason {
    fn from(value: TmdbSeason) -> Self {
        Self {
            season_number: value.season_number,
            episode_count: value.episode_count,
            name: value.name,
            air_date: value.air_date,
            poster_url: tmdb_image_url(value.poster_path),
        }
    }
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
            number_of_episodes: None,
            number_of_seasons: None,
            seasons: vec![],
        })
    }
}

fn tmdb_image_url(path: Option<String>) -> Option<String> {
    path.filter(|value| !value.trim().is_empty())
        .map(|value| format!("https://image.tmdb.org/t/p/w500{}", value))
}

fn normalize_title(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn media_type_compatible(expected: &str, actual: &str) -> bool {
    match expected {
        "anime" => actual == "anime" || actual == "series",
        "" => true,
        value => value == actual,
    }
}

fn metadata_score(query: &str, item: &MediaMetadata) -> i32 {
    let query = normalize_title(query);
    let title = normalize_title(&item.title);
    let original_title = normalize_title(&item.original_title);

    let mut score = 0;
    if !query.is_empty() && query == title {
        score += 100;
    }
    if !query.is_empty() && query == original_title {
        score += 90;
    }
    if !query.is_empty() && title.contains(&query) {
        score += 40;
    }
    if !query.is_empty() && original_title.contains(&query) {
        score += 30;
    }
    if item.poster_url.is_some() {
        score += 5;
    }
    if item.overview.trim().is_empty() {
        score -= 5;
    }
    score += item.vote_average.unwrap_or_default().round() as i32;
    score
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

    #[test]
    fn test_choose_best_match_prefers_compatible_exact_title() {
        let candidates = vec![
            MediaMetadata {
                provider: MetadataProvider::Tmdb,
                provider_id: "1".to_string(),
                title: "Other Show".to_string(),
                original_title: "Other Show".to_string(),
                media_type: "movie".to_string(),
                overview: "overview".to_string(),
                poster_url: None,
                backdrop_url: None,
                release_date: None,
                vote_average: Some(9.0),
                number_of_episodes: None,
                number_of_seasons: None,
                seasons: vec![],
            },
            MediaMetadata {
                provider: MetadataProvider::Tmdb,
                provider_id: "2".to_string(),
                title: "Joy of Life".to_string(),
                original_title: "庆余年".to_string(),
                media_type: "series".to_string(),
                overview: "overview".to_string(),
                poster_url: Some("poster".to_string()),
                backdrop_url: None,
                release_date: None,
                vote_average: Some(7.0),
                number_of_episodes: None,
                number_of_seasons: None,
                seasons: vec![],
            },
        ];

        let selected =
            MetadataService::choose_best_match("Joy of Life", "series", &candidates).unwrap();

        assert_eq!(selected.provider_id, "2");
    }
}
