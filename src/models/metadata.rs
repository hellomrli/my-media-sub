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
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let parsed: MediaMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, MetadataProvider::Tmdb);
        assert_eq!(parsed.provider_id, "123");
    }
}
