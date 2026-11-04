use super::*;
use std::sync::{Arc, Mutex};

fn item(title: &str) -> MediaMetadata {
    serde_json::from_value(serde_json::json!({
        "provider": "tmdb", "provider_id": "1", "title": title,
        "media_type": "series", "release_date": "2024-01-01", "vote_average": 8.0
    }))
    .unwrap()
}

#[tokio::test]
async fn first_match_does_not_try_adjusted_titles() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let outcome = search_queries(
        vec!["原剧名".into(), "回退剧名".into()],
        TITLE_SEARCH_BUDGET,
        |query| {
            calls.lock().unwrap().push(query.clone());
            async move { Ok(vec![item(&query)]) }
        },
    )
    .await
    .unwrap();
    assert_eq!(*calls.lock().unwrap(), ["原剧名"]);
    assert_eq!(outcome.attempted_queries, ["原剧名"]);
    assert_eq!(outcome.matched_query.as_deref(), Some("原剧名"));
}

#[tokio::test]
async fn empty_results_adjust_title_and_stop_at_first_match() {
    let queries = metadata_search_queries("凡人修仙传之星海飞驰篇", None, Some("anime"));
    let outcome = search_queries(queries, TITLE_SEARCH_BUDGET, |query| async move {
        Ok(if query == "凡人修仙传" {
            vec![item(&query)]
        } else {
            vec![]
        })
    })
    .await
    .unwrap();
    assert_eq!(
        outcome.attempted_queries,
        ["凡人修仙传之星海飞驰篇", "凡人修仙传"]
    );
    assert_eq!(outcome.matched_query.as_deref(), Some("凡人修仙传"));
    assert_eq!(outcome.results[0].title, "凡人修仙传");
}

#[tokio::test]
async fn exhausted_attempts_return_empty_without_fabricating_a_match() {
    let queries = (0..10).map(|n| format!("title {n}")).collect();
    let outcome = search_queries(queries, TITLE_SEARCH_BUDGET, |_| async { Ok(vec![]) })
        .await
        .unwrap();
    assert_eq!(outcome.attempted_queries.len(), MAX_METADATA_TITLE_QUERIES);
    assert!(outcome.results.is_empty());
    assert_eq!(outcome.matched_query, None);
}

#[tokio::test]
async fn upstream_errors_do_not_trigger_title_retries() {
    for message in ["HTTP 401", "HTTP 429", "HTTP 500", "connection refused"] {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let error = search_queries(
            vec!["first".into(), "second".into()],
            TITLE_SEARCH_BUDGET,
            |_| {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async { Err(AppError::Http(message.to_string())) }
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains(message));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn title_search_has_a_total_time_budget() {
    let error = search_queries(
        vec!["first".into(), "second".into()],
        Duration::from_millis(1),
        |_| async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(vec![])
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("超时"));
}

#[tokio::test]
async fn disabled_provider_and_missing_key_do_not_make_requests() {
    let dir =
        std::env::temp_dir().join(format!("metadata-title-fallback-{}", uuid::Uuid::new_v4()));
    let settings = SettingsStore::new(dir.join("settings.json"));
    let service = MetadataService::new();
    let error = service
        .search_with_title_fallback(&settings, "剧名", None, Some("series"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("API Key"));
    settings
        .update(|settings| settings.metadata_provider = "none".into())
        .await
        .unwrap();
    let outcome = service
        .search_with_title_fallback(&settings, "剧名", None, Some("series"))
        .await
        .unwrap();
    assert!(outcome.attempted_queries.is_empty());
    assert!(outcome.results.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
