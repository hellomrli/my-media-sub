use super::*;
use crate::services::title_normalize::{metadata_search_queries, MAX_METADATA_TITLE_QUERIES};
use std::future::Future;
use std::time::Duration;

const TITLE_SEARCH_BUDGET: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
pub struct MetadataSearchOutcome {
    pub results: Vec<MediaMetadata>,
    pub attempted_queries: Vec<String>,
    pub matched_query: Option<String>,
}

impl MetadataService {
    pub async fn search_with_title_fallback(
        &self,
        settings_store: &SettingsStore,
        query: &str,
        source_title: Option<&str>,
        media_type: Option<&str>,
    ) -> Result<MetadataSearchOutcome> {
        if query.trim().is_empty() {
            return Err(AppError::Validation("搜索关键词不能为空".to_string()));
        }
        let settings = settings_store.get().await;
        match settings.metadata_provider.as_str() {
            "none" | "" => return Ok(MetadataSearchOutcome::default()),
            "tmdb" if settings.tmdb_api_key.trim().is_empty() => {
                return Err(AppError::Config("未配置 TMDB API Key".to_string()));
            }
            "tmdb" => {}
            "douban" => {
                return Err(AppError::Config(
                    "豆瓣刮削适配器尚未启用，请先使用 TMDB".to_string(),
                ));
            }
            other => return Err(AppError::Config(format!("不支持的元数据提供方: {other}"))),
        }
        let queries = metadata_search_queries(query, source_title, media_type);
        if queries.is_empty() {
            return Err(AppError::Validation(
                "未能从标题中提取搜索关键词".to_string(),
            ));
        }
        let mut outcome = search_queries(queries, TITLE_SEARCH_BUDGET, |query| {
            let api_key = &settings.tmdb_api_key;
            let language = &settings.tmdb_language;
            async move {
                self.search_tmdb(api_key, language, &query, media_type)
                    .await
            }
        })
        .await?;
        // Fetch seasons only for the first successful lookup, not for every trial.
        self.enrich_tmdb_results(
            &settings.tmdb_api_key,
            &settings.tmdb_language,
            &mut outcome.results,
        )
        .await;
        Ok(outcome)
    }
}

async fn search_queries<F, Fut>(
    queries: Vec<String>,
    budget: Duration,
    mut lookup: F,
) -> Result<MetadataSearchOutcome>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<Vec<MediaMetadata>>>,
{
    let deadline = tokio::time::Instant::now() + budget;
    let mut outcome = MetadataSearchOutcome::default();
    for query in queries.into_iter().take(MAX_METADATA_TITLE_QUERIES) {
        outcome.attempted_queries.push(query.clone());
        // Only an empty successful response permits another title. Authentication,
        // rate limits and network errors must not masquerade as a naming problem.
        let results = tokio::time::timeout_at(deadline, lookup(query.clone()))
            .await
            .map_err(|_| AppError::Http("TMDB 剧名匹配超时，请稍后重试".to_string()))??;
        if !results.is_empty() {
            outcome.matched_query = Some(query);
            outcome.results = results;
            break;
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests;
