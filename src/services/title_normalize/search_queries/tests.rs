use super::*;

#[test]
fn preserves_primary_lookup_and_recovers_aliases() {
    assert_eq!(
        metadata_search_queries(
            "未知译名",
            Some("【字幕组】未知译名 / Known English Title (2024) S02 1080p"),
            Some("series"),
        ),
        ["未知译名", "Known English Title"]
    );
    assert_eq!(
        metadata_search_queries("最后生还者 The Last of Us", None, Some("series")),
        ["最后生还者", "The Last of Us"]
    );
    assert_eq!(
        metadata_search_queries("未知译名（又名：正确剧名）", None, Some("series")),
        ["未知译名", "正确剧名"]
    );
}

#[test]
fn retries_series_arc_and_book_titles() {
    for title in [
        "凡人修仙传之星海飞驰篇",
        "凡人修仙传 星海飞驰篇",
        "凡人修仙传：星海飞驰篇",
    ] {
        let queries = metadata_search_queries(title, None, Some("anime"));
        assert_eq!(queries[0], title);
        assert!(queries.contains(&"凡人修仙传".to_string()), "{queries:?}");
    }
    let queries = metadata_search_queries("《凡人修仙传》星海飞驰篇", None, Some("anime"));
    assert_eq!(queries[1], "凡人修仙传");
}

#[test]
fn preserves_explicit_subtitle_boundaries_and_combines_safe_adjustments() {
    for title in [
        "星海旅人 - 远航篇",
        "星海旅人 — 远航篇",
        "星海旅人：远航篇",
        "国漫：星海旅人之远航篇",
    ] {
        let queries = metadata_search_queries(title, None, Some("anime"));
        assert!(queries.contains(&"星海旅人".to_string()), "{queries:?}");
        assert!(queries.len() <= MAX_METADATA_TITLE_QUERIES);
    }
    assert!(
        !metadata_search_queries("Story - A New Beginning", None, Some("movie"))
            .contains(&"Story".to_string())
    );
    assert!(
        metadata_search_queries("Story - A New Beginning", None, Some("series"))
            .contains(&"Story".to_string())
    );
}

#[test]
fn punctuation_and_spacing_adjustments_preserve_sequel_numbers() {
    assert_eq!(
        metadata_search_queries("国漫：凡人修仙传", None, Some("anime")),
        ["国漫：凡人修仙传", "凡人修仙传"]
    );
    assert_eq!(
        metadata_search_queries("庆 余 年", None, Some("series")),
        ["庆 余 年", "庆余年"]
    );
    assert_eq!(
        metadata_search_queries("沙丘 2", None, Some("movie")),
        ["沙丘 2", "沙丘2"]
    );
    for title in [
        "请回答1988",
        "1917",
        "三体 II",
        "鬼滅の刃",
        "四月は君の嘘",
        "The Last of Us",
        "山河之影",
    ] {
        assert_eq!(
            metadata_search_queries(title, None, Some("series")),
            [title]
        );
    }
    let movie = metadata_search_queries("Spider-Man: No Way Home", None, Some("movie"));
    assert!(!movie.contains(&"Spider-Man".to_string()));
    assert!(!movie.contains(&"Spider Man".to_string()));
}

#[test]
fn ignores_unrelated_original_titles_and_noise_only_aliases() {
    assert_eq!(
        metadata_search_queries("新剧名", Some("旧剧名 / Old Show"), Some("series")),
        ["新剧名"]
    );
    assert_eq!(
        metadata_search_queries(
            "庆余年 (2024) [字幕组] / Netflix / 1080p",
            None,
            Some("series")
        ),
        ["庆余年"]
    );
    assert!(metadata_search_queries("https://example.com/share", None, Some("series")).is_empty());
    assert!(metadata_search_queries(" ", None, None).is_empty());
}

#[test]
fn deduplicates_and_caps_title_attempts() {
    let queries = metadata_search_queries(
        "剧名 / First Alias / first alias / Second Alias / Third Alias / Fourth Alias / Fifth Alias / Sixth Alias",
        None,
        Some("series"),
    );
    assert_eq!(queries.len(), MAX_METADATA_TITLE_QUERIES);
    assert_eq!(
        queries
            .iter()
            .filter(|query| query.eq_ignore_ascii_case("First Alias"))
            .count(),
        1
    );
}
