//! 分享链接的季度探测：从探测到的文件名/目录路径中识别有哪些季度，
//! 供订阅编辑器把「手填季号」换成「勾选检测到的季度」。
//!
//! 季度识别复用权威实现 `episode::season_hint_from_context`（文件名
//! Sxx 标记、Season N 目录名、第 N 季中文标记等），与检查/转存链路
//! 的季度过滤完全同一口径，避免「检测出第 2 季、检查却过滤掉」的错位。

use std::collections::BTreeMap;

use serde::Serialize;

use crate::models::subscription::ProbeFile;

/// 单个检测到的季度及其证据。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DetectedSeason {
    pub season: i32,
    /// 该季下的视频文件数
    pub file_count: usize,
    /// 识别出的集数（展示用，去重升序）
    pub episodes: Vec<i32>,
    /// 证据文件名样本（最多 3 个）
    pub sample_files: Vec<String>,
    /// 该季号是推断出来的（分享里没有任何季度标记，按第一季处理），
    /// 而非文件名/目录里显式标注；UI 据此提示用户核对。
    pub inferred: bool,
}

/// 季度探测结果。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SeasonDetection {
    pub ok: bool,
    pub message: String,
    /// 检测到的季度（按季号升序）
    pub seasons: Vec<DetectedSeason>,
    /// 未识别出季度提示的视频文件数（根目录裸文件等）
    pub unspecified_file_count: usize,
    /// 视频文件总数
    pub total_file_count: usize,
}

/// 把一个视频文件计入某季的统计条目（文件数、去重集数、最多 3 个样本）。
fn push_file(entry: &mut (usize, Vec<i32>, Vec<String>), file: &ProbeFile) {
    entry.0 += 1;
    if let Some(episode) = crate::services::detect_episode(&file.name).episode {
        if !entry.1.contains(&episode) {
            entry.1.push(episode);
        }
    }
    if entry.2.len() < 3 {
        entry.2.push(file.name.clone());
    }
}

/// 从探测文件列表中分析季度分布（纯函数）。
pub fn analyze_seasons(files: &[ProbeFile]) -> SeasonDetection {
    let mut by_season: BTreeMap<i32, (usize, Vec<i32>, Vec<String>)> = BTreeMap::new();
    let mut unmarked: Vec<&ProbeFile> = Vec::new();
    let mut total = 0usize;

    for file in files {
        if file.is_dir || !crate::services::is_video_name(&file.name) {
            continue;
        }
        total += 1;
        let Some(season) =
            crate::services::episode::season_hint_from_context(&file.name, &file.parent_path)
        else {
            unmarked.push(file);
            continue;
        };
        push_file(by_season.entry(season.max(1)).or_default(), file);
    }

    // 整个分享都没有季度标记时按第一季处理：`01.mkv`/`02.mkv` 这类命名是最
    // 常见的单季资源，转存侧的 `episode::resolve_file_season` 本来就会把无
    // 标记文件回落到订阅的最小季，这里让探测口径与之对齐，免得用户还要回去
    // 手填一个 1。
    //
    // 只要有任何文件带季度标记就不推断：混合分享（`Season 2/` 正片 + 根目录
    // 番外）如果凭空补一个 S01，用户会勾出一个并不存在的季。
    let inferred = by_season.is_empty() && !unmarked.is_empty();
    if inferred {
        let entry = by_season.entry(1).or_default();
        for file in &unmarked {
            push_file(entry, file);
        }
    }
    let unspecified = if inferred { 0 } else { unmarked.len() };

    let mut seasons: Vec<DetectedSeason> = by_season
        .into_iter()
        .map(|(season, (file_count, mut episodes, sample_files))| {
            episodes.sort();
            DetectedSeason {
                season,
                file_count,
                episodes,
                sample_files,
                inferred,
            }
        })
        .collect();

    let ok = total > 0;
    let message = if !ok {
        "分享中没有识别到视频文件".to_string()
    } else if inferred {
        format!("未检测到季度标记，已按第一季处理（共 {total} 个视频文件）")
    } else if seasons.is_empty() {
        // 防御性兜底：total > 0 且未推断时 by_season 必然非空，此分支不可达。
        format!(
            "未识别到季度标记（{unspecified} 个视频文件都在根目录或文件名无 Sxx/Season 标记），可手动填写季号"
        )
    } else {
        let names = seasons
            .iter()
            .map(|season| format!("S{:02}", season.season))
            .collect::<Vec<_>>()
            .join("、");
        format!("检测到 {names}，共 {total} 个视频文件")
    };
    seasons.shrink_to_fit();

    SeasonDetection {
        ok,
        message,
        seasons,
        unspecified_file_count: unspecified,
        total_file_count: total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, parent: &str) -> ProbeFile {
        ProbeFile {
            name: name.to_string(),
            is_dir: false,
            parent_path: parent.to_string(),
            size: 1,
            updated_at: None,
            file_key: name.to_string(),
        }
    }

    #[test]
    fn detects_seasons_from_names_and_paths() {
        let files = vec![
            file("Show S01E01.mkv", "Season 1"),
            file("Show S01E02.mkv", "Season 1"),
            file("Show.S02E01.mkv", ""),
            file("第二季/Show EP01.mp4", "第二季"),
            file("海报.jpg", "Season 1"),
            file("Show E05.mkv", ""),
        ];

        let detection = analyze_seasons(&files);

        assert!(detection.ok);
        assert_eq!(detection.total_file_count, 5);
        assert_eq!(detection.unspecified_file_count, 1);
        let seasons = detection
            .seasons
            .iter()
            .map(|season| season.season)
            .collect::<Vec<_>>();
        assert_eq!(seasons, vec![1, 2]);
        let s01 = detection.seasons.iter().find(|s| s.season == 1).unwrap();
        assert_eq!(s01.file_count, 2);
        assert_eq!(s01.episodes, vec![1, 2]);
        assert!(s01.sample_files.contains(&"Show S01E01.mkv".to_string()));
        // 中文季标记同样识别（与检查链路同一口径）
        assert!(detection.seasons.iter().any(|season| season.season == 2));
    }

    #[test]
    fn reports_no_videos() {
        let empty = analyze_seasons(&[]);
        assert!(!empty.ok);
        assert!(empty.seasons.is_empty());
        assert_eq!(empty.total_file_count, 0);
    }

    #[test]
    fn infers_season_one_when_share_has_no_markers() {
        // 最常见的单季资源：文件名只有 01/02，没有任何 Sxx/Season 标记。
        // 探测应直接按第一季处理，省掉用户手填季号那一步；转存侧的
        // resolve_file_season 对这些文件同样回落到最小季，口径一致。
        let files = vec![
            file("01.mkv", ""),
            file("02.mkv", ""),
            file("03.mkv", ""),
            file("海报.jpg", ""),
        ];

        let detection = analyze_seasons(&files);

        assert!(detection.ok);
        assert_eq!(detection.total_file_count, 3);
        // 已归入第一季，不再算作「未指定」
        assert_eq!(detection.unspecified_file_count, 0);
        assert_eq!(detection.seasons.len(), 1);
        let s01 = &detection.seasons[0];
        assert_eq!(s01.season, 1);
        assert_eq!(s01.file_count, 3);
        assert_eq!(s01.episodes, vec![1, 2, 3]);
        assert!(s01.inferred, "无标记推断出的季必须标记为 inferred");
        assert!(detection.message.contains("按第一季处理"));
    }

    #[test]
    fn does_not_infer_season_one_when_any_marker_exists() {
        // 回归：混合分享（Season 2 正片 + 根目录番外）不得凭空多报一个 S01，
        // 否则用户会勾选到一个并不存在的季。未标记文件仍计入 unspecified。
        let files = vec![
            file("01.mkv", "Season 2"),
            file("02.mkv", "Season 2"),
            file("番外.mkv", ""),
        ];

        let detection = analyze_seasons(&files);

        let seasons = detection
            .seasons
            .iter()
            .map(|season| season.season)
            .collect::<Vec<_>>();
        assert_eq!(seasons, vec![2]);
        assert!(!detection.seasons[0].inferred);
        assert_eq!(detection.unspecified_file_count, 1);
        assert_eq!(detection.total_file_count, 3);
    }
}
