use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetadataProvider {
    Tmdb,
    Douban,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub provider: MetadataProvider,
    pub provider_id: String,
    pub title: String,
    #[serde(default)]
    pub original_title: String,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub overview: String,
    #[serde(default)]
    pub poster_url: Option<String>,
    #[serde(default)]
    pub backdrop_url: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub vote_average: Option<f32>,
    #[serde(default)]
    pub number_of_episodes: Option<i32>,
    #[serde(default)]
    pub number_of_seasons: Option<i32>,
    #[serde(default)]
    pub seasons: Vec<MediaMetadataSeason>,
    #[serde(default)]
    pub next_episode_to_air: Option<MediaMetadataEpisode>,
    #[serde(default)]
    pub episodes: Vec<MediaMetadataEpisode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadataSeason {
    pub season_number: i32,
    #[serde(default)]
    pub episode_count: Option<i32>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub air_date: Option<String>,
    #[serde(default)]
    pub poster_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaMetadataEpisode {
    #[serde(default)]
    pub season_number: i32,
    #[serde(default)]
    pub episode_number: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub overview: String,
    #[serde(default)]
    pub air_date: Option<String>,
    #[serde(default)]
    pub still_url: Option<String>,
}

pub fn episode_count_for_season(metadata: Option<&MediaMetadata>, season: i32) -> Option<i32> {
    let metadata = metadata?;
    let season = if season > 0 { season } else { 1 };
    metadata
        .seasons
        .iter()
        .find(|item| item.season_number == season)
        .and_then(|item| item.episode_count)
        .or_else(|| {
            if season == 1 && metadata.number_of_seasons.unwrap_or(1) <= 1 {
                metadata.number_of_episodes
            } else {
                None
            }
        })
}

/// Merge a refreshed metadata snapshot without discarding image/schedule data when an
/// upstream season request was only partially successful. A different provider item is
/// treated as an intentional replacement and is not merged with the old selection.
pub fn merge_refreshed_metadata(
    existing: Option<&MediaMetadata>,
    mut refreshed: MediaMetadata,
) -> MediaMetadata {
    let Some(existing) = existing.filter(|current| {
        current.provider == refreshed.provider && current.provider_id == refreshed.provider_id
    }) else {
        return refreshed;
    };

    if refreshed.poster_url.is_none() {
        refreshed.poster_url.clone_from(&existing.poster_url);
    }
    if refreshed.backdrop_url.is_none() {
        refreshed.backdrop_url.clone_from(&existing.backdrop_url);
    }

    for season in &mut refreshed.seasons {
        if season.poster_url.is_none() {
            season.poster_url = existing
                .seasons
                .iter()
                .find(|current| current.season_number == season.season_number)
                .and_then(|current| current.poster_url.clone());
        }
    }
    for season in &existing.seasons {
        if !refreshed
            .seasons
            .iter()
            .any(|current| current.season_number == season.season_number)
        {
            refreshed.seasons.push(season.clone());
        }
    }
    refreshed.seasons.sort_by_key(|season| season.season_number);

    for episode in &mut refreshed.episodes {
        if episode.still_url.is_none() {
            episode.still_url = existing
                .episodes
                .iter()
                .find(|current| {
                    current.season_number == episode.season_number
                        && current.episode_number == episode.episode_number
                })
                .and_then(|current| current.still_url.clone());
        }
    }
    for episode in &existing.episodes {
        if !refreshed.episodes.iter().any(|current| {
            current.season_number == episode.season_number
                && current.episode_number == episode.episode_number
        }) {
            refreshed.episodes.push(episode.clone());
        }
    }
    refreshed
        .episodes
        .sort_by_key(|episode| (episode.season_number, episode.episode_number));

    if let (Some(previous), Some(next)) = (
        existing.next_episode_to_air.as_ref(),
        refreshed.next_episode_to_air.as_mut(),
    ) {
        if next.season_number == previous.season_number
            && next.episode_number == previous.episode_number
            && next.still_url.is_none()
        {
            next.still_url.clone_from(&previous.still_url);
        }
    }

    refreshed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_metadata_serialize() {
        let metadata = MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "123".to_string(),
            title: "测试标题".to_string(),
            original_title: "Original".to_string(),
            media_type: "series".to_string(),
            overview: "简介".to_string(),
            poster_url: Some("https://image.tmdb.org/t/p/w500/poster.jpg".to_string()),
            backdrop_url: None,
            release_date: Some("2024-01-01".to_string()),
            vote_average: Some(8.2),
            number_of_episodes: Some(12),
            number_of_seasons: Some(1),
            seasons: vec![MediaMetadataSeason {
                season_number: 1,
                episode_count: Some(12),
                name: "Season 1".to_string(),
                air_date: Some("2024-01-01".to_string()),
                poster_url: None,
            }],
            next_episode_to_air: Some(MediaMetadataEpisode {
                season_number: 1,
                episode_number: 2,
                name: "第二集".to_string(),
                overview: String::new(),
                air_date: Some("2024-01-08".to_string()),
                still_url: None,
            }),
            episodes: vec![MediaMetadataEpisode {
                season_number: 1,
                episode_number: 1,
                name: "第一集".to_string(),
                overview: String::new(),
                air_date: Some("2024-01-01".to_string()),
                still_url: None,
            }],
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let parsed: MediaMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, MetadataProvider::Tmdb);
        assert_eq!(parsed.provider_id, "123");
        assert_eq!(episode_count_for_season(Some(&parsed), 1), Some(12));
    }

    #[test]
    fn refreshed_metadata_preserves_images_from_partial_same_item_response() {
        let existing = MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "123".to_string(),
            title: "Old title".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: Some("poster-old".to_string()),
            backdrop_url: Some("backdrop-old".to_string()),
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(2),
            number_of_seasons: Some(1),
            seasons: vec![MediaMetadataSeason {
                season_number: 1,
                episode_count: Some(2),
                name: "Season 1".to_string(),
                air_date: None,
                poster_url: Some("season-old".to_string()),
            }],
            next_episode_to_air: None,
            episodes: vec![MediaMetadataEpisode {
                season_number: 1,
                episode_number: 1,
                name: "Episode 1".to_string(),
                overview: String::new(),
                air_date: Some("2026-01-01".to_string()),
                still_url: Some("still-old".to_string()),
            }],
        };
        let refreshed = MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "123".to_string(),
            title: "Fresh title".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: "Fresh overview".to_string(),
            poster_url: None,
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: Some(2),
            number_of_seasons: Some(1),
            seasons: vec![],
            next_episode_to_air: None,
            episodes: vec![],
        };

        let merged = merge_refreshed_metadata(Some(&existing), refreshed);
        assert_eq!(merged.title, "Fresh title");
        assert_eq!(merged.poster_url.as_deref(), Some("poster-old"));
        assert_eq!(merged.seasons[0].poster_url.as_deref(), Some("season-old"));
        assert_eq!(merged.episodes[0].still_url.as_deref(), Some("still-old"));
    }

    #[test]
    fn refreshed_metadata_does_not_merge_a_different_provider_item() {
        let existing = MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "old".to_string(),
            title: "Old".to_string(),
            original_title: String::new(),
            media_type: "series".to_string(),
            overview: String::new(),
            poster_url: Some("old-poster".to_string()),
            backdrop_url: None,
            release_date: None,
            vote_average: None,
            number_of_episodes: None,
            number_of_seasons: None,
            seasons: vec![],
            next_episode_to_air: None,
            episodes: vec![],
        };
        let mut refreshed = existing.clone();
        refreshed.provider_id = "new".to_string();
        refreshed.poster_url = None;

        let merged = merge_refreshed_metadata(Some(&existing), refreshed);
        assert!(merged.poster_url.is_none());
    }
}
