use std::path::Path;

use tokio_stream::StreamExt;

use crate::clients::http_pool::{self, ObservedRequestBuilder};
use crate::error::{AppError, Result};
use crate::models::{MediaMetadata, MetadataProvider, Settings};
use crate::services::subscription_transfer::{
    season_folder_name, season_suffix_number, strip_season_suffix,
};
use crate::utils::write_file_atomic;

/// 单张图片下载与落盘的硬上限（与 image_proxy 一致）。
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// 一次「下载完成 → 写入媒体库元数据」的结果，供日志与测试断言。
#[derive(Debug, Clone, Default)]
pub struct MediaMetadataFilesOutcome {
    /// 实际写入（或更新）的文件数。
    pub written: usize,
    /// 已存在且字节相同、无需写入的文件数。
    pub skipped: usize,
    /// 单项失败数（图片下载失败、目录不可写等）。
    pub failed: usize,
}

/// 订阅下载完成后，按匹配到的 TMDB 元数据把海报/NFO 写入 Aria2 本地下载目录，
/// 供 Jellyfin/Emby/Kodi 刮削。开关关闭或无法安全推导目录时直接返回空结果。
///
/// 布局（与 `subscription_transfer` 的目录生成逻辑严格对应）：
/// - 剧集/动画：`剧名根目录/tvshow.nfo` + `poster.jpg` + `backdrop.jpg`，
///   `剧名根目录/Season N/season.nfo` + 季海报 `poster.jpg`（seasons 命中当前季才写）。
/// - 电影：`task.dir/movie.nfo` + `poster.jpg` + `backdrop.jpg`。
///
/// 单项失败不阻塞其余文件；所有失败聚合成一个 Err，由调用方统一 warn 一次。
pub async fn write_media_metadata_files(
    settings: &Settings,
    metadata: &MediaMetadata,
    media_type: &str,
    task_dir: &str,
) -> Result<MediaMetadataFilesOutcome> {
    if !settings.media_metadata_files_enabled {
        return Ok(MediaMetadataFilesOutcome::default());
    }
    let Some((show_root, season)) = resolve_layout(media_type, task_dir) else {
        return Ok(MediaMetadataFilesOutcome::default());
    };

    // 图片在 async 侧下载（网络 IO 不占用 executor 线程），随后文件系统
    // 操作整体放进 spawn_blocking，避免阻塞下载监控的轮询线程。
    let poster_bytes = fetch_optional_image(metadata.poster_url.as_deref()).await;
    let backdrop_bytes = fetch_optional_image(metadata.backdrop_url.as_deref()).await;
    let season_poster_bytes =
        fetch_optional_image(season.and_then(|season| season_poster_url(metadata, season))).await;

    let is_series = matches!(media_type, "series" | "anime");
    let root_nfo = if is_series {
        build_tvshow_nfo(metadata)
    } else {
        build_movie_nfo(metadata)
    };
    let season_dir = season.map(|season| {
        let mut dir = show_root.clone();
        if !dir.is_empty() {
            dir.push('/');
        }
        dir.push_str(&season_folder_name(season));
        dir
    });

    let root_path = show_root.clone();
    tokio::task::spawn_blocking(move || {
        let mut outcome = MediaMetadataFilesOutcome::default();
        let mut failures = Vec::new();

        let root_nfo_name = if is_series { "tvshow.nfo" } else { "movie.nfo" };
        record_write(
            &mut outcome,
            &mut failures,
            &root_path,
            root_nfo_name,
            root_nfo.as_bytes(),
        );
        record_image_write(
            &mut outcome,
            &mut failures,
            &root_path,
            "poster.jpg",
            poster_bytes.as_ref(),
        );
        record_image_write(
            &mut outcome,
            &mut failures,
            &root_path,
            "backdrop.jpg",
            backdrop_bytes.as_ref(),
        );
        if let (Some(season_dir), Some(season)) = (season_dir, season) {
            record_write(
                &mut outcome,
                &mut failures,
                &season_dir,
                "season.nfo",
                build_season_nfo(season).as_bytes(),
            );
            record_image_write(
                &mut outcome,
                &mut failures,
                &season_dir,
                "poster.jpg",
                season_poster_bytes.as_ref(),
            );
        }

        if !failures.is_empty() {
            return Err(AppError::Internal(format!(
                "媒体库元数据写入部分失败（写入 {}，跳过 {}，失败 {}）: {}",
                outcome.written,
                outcome.skipped,
                outcome.failed,
                failures.join("; ")
            )));
        }
        Ok(outcome)
    })
    .await
    .map_err(|error| AppError::Internal(format!("媒体库元数据写入任务执行失败: {error}")))?
}

/// 推导剧集根目录与当前下载季号；无法安全推导时返回 `None`。
fn resolve_layout(media_type: &str, task_dir: &str) -> Option<(String, Option<i32>)> {
    let task_dir = task_dir.trim().replace('\\', "/");
    let task_dir = task_dir.trim_end_matches('/');
    if task_dir.is_empty() {
        return None;
    }
    match media_type {
        // 剧集/动画必须有 `Season N` 后缀才能推导根目录；没有后缀只可能是
        // 落到 Aria2 默认目录的非标准布局，写 NFO 会污染错误位置。
        "series" | "anime" => {
            let season = season_suffix_number(task_dir)?;
            let show_root = strip_season_suffix(task_dir);
            if show_root.is_empty() || show_root == "/" || show_root == task_dir {
                return None;
            }
            Some((show_root, Some(season)))
        }
        // 电影目录即下载目录，即使名字恰好含 "Season 1" 也不剥除。
        "movie" => {
            if task_dir == "/" {
                return None;
            }
            Some((task_dir.to_string(), None))
        }
        _ => None,
    }
}

fn season_poster_url(metadata: &MediaMetadata, season: i32) -> Option<&str> {
    metadata
        .seasons
        .iter()
        .find(|item| item.season_number == season)
        .and_then(|item| item.poster_url.as_deref())
}

/// 生成 tvshow.nfo（Kodi/Jellyfin 标准字段集）。
fn build_tvshow_nfo(metadata: &MediaMetadata) -> String {
    build_root_nfo(metadata, "tvshow")
}

/// 生成 movie.nfo。
fn build_movie_nfo(metadata: &MediaMetadata) -> String {
    build_root_nfo(metadata, "movie")
}

fn build_root_nfo(metadata: &MediaMetadata, root: &str) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\" ?>\n<{root}>\n"
    );
    xml.push_str(&format!("  <title>{}</title>\n", xml_escape(&metadata.title)));
    if !metadata.original_title.trim().is_empty() {
        xml.push_str(&format!(
            "  <originaltitle>{}</originaltitle>\n",
            xml_escape(&metadata.original_title)
        ));
    }
    if let Some(year) = release_year(&metadata.release_date) {
        xml.push_str(&format!("  <year>{year}</year>\n"));
    }
    if !metadata.overview.trim().is_empty() {
        xml.push_str(&format!("  <plot>{}</plot>\n", xml_escape(&metadata.overview)));
    }
    if let Some(rating) = metadata.vote_average {
        // `{}` 输出 f32 的最短表示（如 8.25），避免 {:.1} 的舍入误差。
        xml.push_str(&format!("  <rating>{rating}</rating>\n"));
    }
    if let Some(premiered) = metadata
        .release_date
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        xml.push_str(&format!("  <premiered>{}</premiered>\n", xml_escape(premiered)));
    }
    // 豆瓣 id 冒充 TMDB id 会污染媒体库刮削，只在 TMDB 提供方时写 uniqueid。
    if metadata.provider == MetadataProvider::Tmdb && !metadata.provider_id.trim().is_empty() {
        xml.push_str(&format!(
            "  <uniqueid type=\"tmdb\">{}</uniqueid>\n",
            xml_escape(&metadata.provider_id)
        ));
    }
    xml.push_str(&format!("</{root}>\n"));
    xml
}

fn build_season_nfo(season: i32) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\" ?>\n<season>\n  <seasonnumber>{}</seasonnumber>\n</season>\n",
        season.max(1)
    )
}

/// release_date（如 `2024-01-01`）的前 4 位年份；非纯数字时视为缺失。
fn release_year(release_date: &Option<String>) -> Option<&str> {
    release_date
        .as_deref()
        .and_then(|date| date.get(0..4))
        .filter(|year| year.chars().all(|ch| ch.is_ascii_digit()))
}

/// XML 元素文本转义：`& < > " '` 与 XML 1.0 不允许的控制字符。
fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            ch if ch.is_control() && ch != '\t' && ch != '\r' && ch != '\n' => {}
            ch => escaped.push(ch),
        }
    }
    escaped
}

/// 下载单张图片；`None` 表示没有可用 URL（不算失败）。
async fn fetch_optional_image(url: Option<&str>) -> Option<Result<Vec<u8>>> {
    let url = url.map(str::trim).filter(|value| !value.is_empty())?;
    Some(fetch_image(url).await)
}

/// 下载并校验图片字节（content-type 白名单 + 大小上限，与 image_proxy 一致）。
async fn fetch_image(url: &str) -> Result<Vec<u8>> {
    let upstream = http_pool::medium_client()
        .get(url)
        .header(
            reqwest::header::ACCEPT,
            "image/avif,image/webp,image/png,image/jpeg",
        )
        .send_observed("tmdb_image")
        .await
        .map_err(|error| AppError::Http(format!("海报图片请求失败: {error}")))?;
    if !upstream.status().is_success() {
        return Err(AppError::Http(format!(
            "海报图片请求失败: HTTP {}",
            upstream.status()
        )));
    }
    if upstream
        .content_length()
        .is_some_and(|length| length > MAX_IMAGE_BYTES as u64)
    {
        return Err(AppError::Validation("海报图片超过大小限制".to_string()));
    }
    upstream
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
        .ok_or_else(|| AppError::Http("上游返回了非图片内容".to_string()))?;
    let mut body = Vec::new();
    let mut stream = upstream.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| AppError::Http(format!("海报图片下载失败: {error}")))?;
        if body.len().saturating_add(chunk.len()) > MAX_IMAGE_BYTES {
            return Err(AppError::Validation("海报图片超过大小限制".to_string()));
        }
        body.extend_from_slice(&chunk);
    }
    if body.is_empty() {
        return Err(AppError::Validation("海报图片内容为空".to_string()));
    }
    Ok(body)
}

/// 与现有文件字节比较，相同则跳过，否则原子写入。返回是否实际写入。
fn write_if_changed(path: &Path, content: &[u8]) -> Result<bool> {
    match std::fs::read(path) {
        Ok(existing) if existing == content => Ok(false),
        _ => {
            write_file_atomic(path, content, 0o644)?;
            Ok(true)
        }
    }
}

fn record_write(
    outcome: &mut MediaMetadataFilesOutcome,
    failures: &mut Vec<String>,
    dir: &str,
    name: &str,
    content: &[u8],
) {
    let path = Path::new(dir).join(name);
    match write_if_changed(&path, content) {
        Ok(true) => outcome.written += 1,
        Ok(false) => outcome.skipped += 1,
        Err(error) => {
            outcome.failed += 1;
            failures.push(format!("{}: {error}", path.display()));
        }
    }
}

fn record_image_write(
    outcome: &mut MediaMetadataFilesOutcome,
    failures: &mut Vec<String>,
    dir: &str,
    name: &str,
    fetched: Option<&Result<Vec<u8>>>,
) {
    let Some(fetched) = fetched else {
        return; // 无 URL，不算失败
    };
    match fetched {
        Err(error) => {
            outcome.failed += 1;
            failures.push(format!("{name}: {error}"));
        }
        Ok(bytes) => record_write(outcome, failures, dir, name, bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MediaMetadataSeason;
    use std::sync::Arc;

    fn tmdb_metadata() -> MediaMetadata {
        MediaMetadata {
            provider: MetadataProvider::Tmdb,
            provider_id: "123".to_string(),
            title: "庆余年".to_string(),
            original_title: "Joy of Life".to_string(),
            media_type: "series".to_string(),
            overview: "一个范闲的故事。".to_string(),
            poster_url: Some("http://127.0.0.1:1/poster.jpg".to_string()),
            backdrop_url: Some("http://127.0.0.1:1/backdrop.jpg".to_string()),
            release_date: Some("2024-01-01".to_string()),
            vote_average: Some(8.25),
            number_of_episodes: Some(12),
            number_of_seasons: Some(1),
            seasons: vec![MediaMetadataSeason {
                season_number: 1,
                episode_count: Some(12),
                name: "Season 1".to_string(),
                air_date: Some("2024-01-01".to_string()),
                poster_url: Some("http://127.0.0.1:1/season1.jpg".to_string()),
            }],
            next_episode_to_air: None,
            episodes: vec![],
        }
    }

    #[test]
    fn tvshow_nfo_contains_full_standard_fields() {
        let xml = build_tvshow_nfo(&tmdb_metadata());
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("<tvshow>"));
        assert!(xml.contains("</tvshow>"));
        assert!(xml.contains("<title>庆余年</title>"));
        assert!(xml.contains("<originaltitle>Joy of Life</originaltitle>"));
        assert!(xml.contains("<year>2024</year>"));
        assert!(xml.contains("<plot>一个范闲的故事。</plot>"));
        assert!(xml.contains("<rating>8.25</rating>"));
        assert!(xml.contains("<premiered>2024-01-01</premiered>"));
        assert!(xml.contains("<uniqueid type=\"tmdb\">123</uniqueid>"));
    }

    #[test]
    fn tvshow_nfo_omits_missing_optional_fields() {
        let mut metadata = tmdb_metadata();
        metadata.original_title = String::new();
        metadata.release_date = None;
        metadata.vote_average = None;
        metadata.overview = String::new();
        let xml = build_tvshow_nfo(&metadata);
        assert!(!xml.contains("<originaltitle>"));
        assert!(!xml.contains("<year>"));
        assert!(!xml.contains("<premiered>"));
        assert!(!xml.contains("<rating>"));
        assert!(!xml.contains("<plot>"));
        assert!(xml.contains("<uniqueid type=\"tmdb\">123</uniqueid>"));
    }

    #[test]
    fn douban_provider_omits_uniqueid() {
        let mut metadata = tmdb_metadata();
        metadata.provider = MetadataProvider::Douban;
        let xml = build_tvshow_nfo(&metadata);
        assert!(!xml.contains("uniqueid"));

        let mut metadata = tmdb_metadata();
        metadata.provider_id = String::new();
        let xml = build_tvshow_nfo(&metadata);
        assert!(!xml.contains("uniqueid"));
    }

    #[test]
    fn xml_escape_handles_special_and_control_chars() {
        assert_eq!(
            xml_escape("A&B <C> \"D\" 'E'"),
            "A&amp;B &lt;C&gt; &quot;D&quot; &apos;E&apos;"
        );
        assert_eq!(xml_escape("庆余年"), "庆余年");
        assert_eq!(xml_escape("bad\u{1}text\u{7}"), "badtext");
        assert_eq!(xml_escape("tab\tnewline\n"), "tab\tnewline\n");
    }

    #[test]
    fn movie_nfo_uses_movie_root() {
        let xml = build_movie_nfo(&tmdb_metadata());
        assert!(xml.contains("<movie>"));
        assert!(xml.contains("</movie>"));
        assert!(xml.contains("<title>庆余年</title>"));
        assert!(!xml.contains("tvshow"));
    }

    #[test]
    fn season_nfo_contains_season_number() {
        let xml = build_season_nfo(1);
        assert!(xml.contains("<season>"));
        assert!(xml.contains("<seasonnumber>1</seasonnumber>"));
        assert!(xml.contains("</season>"));
    }

    #[test]
    fn resolve_layout_parses_show_root_and_season() {
        assert_eq!(
            resolve_layout("series", "/media/Show (2024)/Season 1"),
            Some(("/media/Show (2024)".to_string(), Some(1)))
        );
        // 小写 season 也命中。
        assert_eq!(
            resolve_layout("anime", "/media/Show/season 3"),
            Some(("/media/Show".to_string(), Some(3)))
        );
        // 无 Season 后缀无法推导。
        assert_eq!(resolve_layout("series", "/media/Show (2024)"), None);
        // SeasonX（无空格）不命中。
        assert_eq!(resolve_layout("series", "/media/Show/SeasonX"), None);
        // movie 目录即使含 "Season 1" 也不剥除。
        assert_eq!(
            resolve_layout("movie", "/movies/Show Season 1"),
            Some(("/movies/Show Season 1".to_string(), None))
        );
        // 空目录与根目录跳过。
        assert_eq!(resolve_layout("series", ""), None);
        assert_eq!(resolve_layout("series", "/"), None);
        assert_eq!(resolve_layout("movie", "/"), None);
        // 相对路径 "Season 1" 无法推导根目录。
        assert_eq!(resolve_layout("series", "Season 1"), None);
        // 未知媒体类型跳过。
        assert_eq!(resolve_layout("custom_1", "/media/Show/Season 1"), None);
        // Windows 分隔符归一化。
        assert_eq!(
            resolve_layout("series", "C:\\media\\Show\\Season 2"),
            Some(("C:/media/Show".to_string(), Some(2)))
        );
    }

    #[test]
    fn release_year_extracts_four_digits() {
        assert_eq!(release_year(&Some("2024-01-01".to_string())), Some("2024"));
        assert_eq!(release_year(&Some("2024".to_string())), Some("2024"));
        assert_eq!(release_year(&Some("abc".to_string())), None);
        assert_eq!(release_year(&None), None);
    }

    /// 起一个返回固定图片字节的本地测试服务器，返回请求计数与 URL。
    async fn spawn_image_server(
        body: &'static [u8],
        content_type: &'static str,
    ) -> (Arc<std::sync::atomic::AtomicUsize>, String) {
        use std::sync::atomic::AtomicUsize;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            }
        });
        (requests, format!("http://{addr}/poster.jpg"))
    }

    #[tokio::test]
    async fn disabled_switch_writes_nothing() {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-metadata-disabled-{}",
            uuid::Uuid::new_v4()
        ));
        let season_dir = dir.join("Show/Season 1");
        std::fs::create_dir_all(&season_dir).unwrap();
        let (requests, url) = spawn_image_server(b"img", "image/jpeg").await;

        let settings = Settings::default();
        let mut metadata = tmdb_metadata();
        metadata.poster_url = Some(url.clone());
        metadata.backdrop_url = Some(url.clone());
        metadata.seasons[0].poster_url = Some(url.clone());
        let outcome = write_media_metadata_files(&settings, &metadata, "series", season_dir.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(outcome.written, 0);
        assert_eq!(outcome.failed, 0);
        // 开关关闭时不得发起任何图片请求，也不得写任何文件。
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(!season_dir.exists() || std::fs::read_dir(&season_dir).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn writes_full_series_layout_with_images() {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-metadata-files-{}",
            uuid::Uuid::new_v4()
        ));
        let show_root = dir.join("Show (2024)");
        let season_dir = show_root.join("Season 1");
        std::fs::create_dir_all(&season_dir).unwrap();
        let image = b"fake-jpeg-bytes".as_slice();
        let (requests, url) = spawn_image_server(image, "image/jpeg").await;

        let settings = Settings {
            media_metadata_files_enabled: true,
            ..Default::default()
        };
        let mut metadata = tmdb_metadata();
        metadata.poster_url = Some(url.clone());
        metadata.backdrop_url = Some(url.clone());
        metadata.seasons[0].poster_url = Some(url.clone());
        let outcome = write_media_metadata_files(
            &settings,
            &metadata,
            "series",
            season_dir.to_str().unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(outcome.written, 5);
        assert_eq!(outcome.failed, 0);
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 3);
        let tvshow = std::fs::read_to_string(show_root.join("tvshow.nfo")).unwrap();
        assert!(tvshow.contains("<title>庆余年</title>"));
        assert!(tvshow.contains("<uniqueid type=\"tmdb\">123</uniqueid>"));
        let season_nfo = std::fs::read_to_string(season_dir.join("season.nfo")).unwrap();
        assert!(season_nfo.contains("<seasonnumber>1</seasonnumber>"));
        for file in [
            show_root.join("poster.jpg"),
            show_root.join("backdrop.jpg"),
            season_dir.join("poster.jpg"),
        ] {
            assert_eq!(std::fs::read(file).unwrap(), image);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn idempotent_second_run_skips_all() {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-metadata-idempotent-{}",
            uuid::Uuid::new_v4()
        ));
        let season_dir = dir.join("Show/Season 1");
        std::fs::create_dir_all(&season_dir).unwrap();
        let (requests, url) = spawn_image_server(b"img", "image/jpeg").await;

        let settings = Settings {
            media_metadata_files_enabled: true,
            ..Default::default()
        };
        let mut metadata = tmdb_metadata();
        metadata.poster_url = Some(url.clone());
        metadata.backdrop_url = None;
        metadata.seasons[0].poster_url = Some(url.clone());
        write_media_metadata_files(&settings, &metadata, "series", season_dir.to_str().unwrap())
            .await
            .unwrap();
        let first = write_media_metadata_files(&settings, &metadata, "series", season_dir.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(first.written, 0);
        assert_eq!(first.skipped, 4);
        assert_eq!(first.failed, 0);
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 4);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn metadata_change_rewrites_nfo() {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-metadata-update-{}",
            uuid::Uuid::new_v4()
        ));
        let season_dir = dir.join("Show/Season 1");
        std::fs::create_dir_all(&season_dir).unwrap();
        let (requests, url) = spawn_image_server(b"img", "image/jpeg").await;

        let settings = Settings {
            media_metadata_files_enabled: true,
            ..Default::default()
        };
        let mut metadata = tmdb_metadata();
        metadata.poster_url = Some(url.clone());
        metadata.backdrop_url = None;
        metadata.seasons[0].poster_url = Some(url.clone());
        write_media_metadata_files(&settings, &metadata, "series", season_dir.to_str().unwrap())
            .await
            .unwrap();
        metadata.title = "新标题".to_string();
        let second = write_media_metadata_files(&settings, &metadata, "series", season_dir.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(second.written, 1);
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 4);
        let tvshow = std::fs::read_to_string(dir.join("Show/tvshow.nfo")).unwrap();
        assert!(tvshow.contains("<title>新标题</title>"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn non_image_response_fails_that_image_only() {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-metadata-badimage-{}",
            uuid::Uuid::new_v4()
        ));
        let season_dir = dir.join("Show/Season 1");
        std::fs::create_dir_all(&season_dir).unwrap();
        let (requests, url) = spawn_image_server(b"<html>error</html>", "text/html").await;

        let settings = Settings {
            media_metadata_files_enabled: true,
            ..Default::default()
        };
        let mut metadata = tmdb_metadata();
        metadata.poster_url = Some(url.clone());
        metadata.backdrop_url = Some(url.clone());
        metadata.seasons[0].poster_url = Some(url.clone());
        let result =
            write_media_metadata_files(&settings, &metadata, "series", season_dir.to_str().unwrap())
                .await;

        // 三张图全部因 content-type 失败，但 NFO 仍写入。
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 3);
        let outcome = result.unwrap_err().to_string();
        assert!(outcome.contains("部分失败"));
        assert!(outcome.contains("失败 3"));
        assert!(std::fs::read_to_string(dir.join("Show/tvshow.nfo"))
            .unwrap()
            .contains("<title>庆余年</title>"));
        assert!(std::fs::read_to_string(season_dir.join("season.nfo")).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn missing_backdrop_and_season_poster_are_not_failures() {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-metadata-nobackdrop-{}",
            uuid::Uuid::new_v4()
        ));
        let season_dir = dir.join("Show/Season 1");
        std::fs::create_dir_all(&season_dir).unwrap();
        let (requests, url) = spawn_image_server(b"img", "image/jpeg").await;

        let settings = Settings {
            media_metadata_files_enabled: true,
            ..Default::default()
        };
        let mut metadata = tmdb_metadata();
        metadata.poster_url = Some(url.clone());
        metadata.backdrop_url = None;
        metadata.seasons = vec![]; // 当前季未命中
        let outcome = write_media_metadata_files(&settings, &metadata, "series", season_dir.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(outcome.written, 3); // tvshow.nfo + season.nfo + poster.jpg（两图无 URL 跳过）
        assert_eq!(outcome.failed, 0);
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(!season_dir.join("poster.jpg").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn movie_writes_flat_layout() {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-metadata-movie-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (requests, url) = spawn_image_server(b"img", "image/jpeg").await;

        let settings = Settings {
            media_metadata_files_enabled: true,
            ..Default::default()
        };
        let mut metadata = tmdb_metadata();
        metadata.poster_url = Some(url.clone());
        metadata.backdrop_url = Some(url.clone());
        let outcome = write_media_metadata_files(&settings, &metadata, "movie", dir.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(outcome.written, 3); // movie.nfo + poster.jpg + backdrop.jpg
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);
        let movie = std::fs::read_to_string(dir.join("movie.nfo")).unwrap();
        assert!(movie.contains("<movie>"));
        assert!(!dir.join("season.nfo").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn unwritable_directory_aggregates_failure() {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-metadata-readonly-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (requests, url) = spawn_image_server(b"img", "image/jpeg").await;

        let settings = Settings {
            media_metadata_files_enabled: true,
            ..Default::default()
        };
        let mut metadata = tmdb_metadata();
        metadata.poster_url = Some(url.clone());
        metadata.backdrop_url = None;
        metadata.seasons = vec![];
        // 把「根目录」设为一个已存在的普通文件，写入必然失败。
        let blocker = dir.join("Show (2024)");
        std::fs::write(&blocker, b"x").unwrap();
        let result = write_media_metadata_files(
            &settings,
            &metadata,
            "series",
            dir.join("Show (2024)/Season 1").to_str().unwrap(),
        )
        .await;

        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        let message = result.unwrap_err().to_string();
        assert!(message.contains("部分失败"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
