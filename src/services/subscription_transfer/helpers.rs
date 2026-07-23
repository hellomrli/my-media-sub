/// 递归收集目录下所有视频文件（独立函数，使用 Box 解决递归问题）
fn collect_video_files_recursive<'a>(
    save_client: &'a dyn CloudDriveProvider,
    parent_fid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<DriveItem>>> + Send + 'a>> {
    Box::pin(async move {
        use crate::services::is_video_name;
        let mut video_files = Vec::new();

        let items = save_client.list(parent_fid).await?;

        for item in items {
            if item.is_dir {
                // 递归进入子目录
                match collect_video_files_recursive(save_client, &item.id).await {
                    Ok(mut sub_videos) => video_files.append(&mut sub_videos),
                    Err(e) => warn!("读取子目录 {} 失败: {}", item.name, e),
                }
            } else if is_video_name(&item.name) {
                // 是视频文件，加入列表
                video_files.push(item);
            }
        }

        Ok(video_files)
    })
}

fn expected_video_names(file_names: &[String]) -> HashSet<String> {
    file_names
        .iter()
        .filter(|name| crate::services::is_video_name(name))
        .cloned()
        .collect()
}

/// 将分享侧转存结果映射为网盘 DriveItem，供落盘等待失败时回退提交 Aria2。
fn provider_files_to_drive_items(files: &[&ProviderFile], parent_id: &str) -> Vec<DriveItem> {
    files
        .iter()
        .filter(|file| !file.is_dir && !file.id.trim().is_empty())
        .map(|file| DriveItem {
            id: file.id.clone(),
            parent_id: parent_id.to_string(),
            name: file.name.clone(),
            is_dir: false,
            size: file.size,
            updated_at: file.updated_at.clone().unwrap_or_default(),
        })
        .collect()
}

fn dedup_provider_episode_files<'a>(
    sub: &Subscription,
    files: Vec<&'a ProviderFile>,
) -> Vec<&'a ProviderFile> {
    if sub.media_type == "movie" {
        return files;
    }

    let mut best_by_episode: std::collections::HashMap<(i32, i32), usize> =
        std::collections::HashMap::new();
    for (index, file) in files.iter().enumerate() {
        if !provider_file_matches_subscription_season(sub, file) {
            continue;
        }
        let Some(key) = crate::services::episode::episode_video_key(&file.name, sub.season) else {
            continue;
        };

        match best_by_episode.get(&key).copied() {
            Some(current_index) => {
                let current = files[current_index];
                if is_better_episode_duplicate_candidate(
                    EpisodeDuplicateCandidate {
                        name: &file.name,
                        size: file.size,
                        updated_at: file.updated_at.as_deref(),
                        order: index,
                    },
                    EpisodeDuplicateCandidate {
                        name: &current.name,
                        size: current.size,
                        updated_at: current.updated_at.as_deref(),
                        order: current_index,
                    },
                    &sub.rules.duplicate_episode_strategy,
                ) {
                    best_by_episode.insert(key, index);
                }
            }
            None => {
                best_by_episode.insert(key, index);
            }
        }
    }

    let selected: HashSet<usize> = best_by_episode.values().copied().collect();
    files
        .into_iter()
        .enumerate()
        .filter(|(index, file)| {
            if !provider_file_matches_subscription_season(sub, file) {
                return false;
            }
            crate::services::episode::episode_video_key(&file.name, sub.season)
                .map(|_| selected.contains(index))
                .unwrap_or(true)
        })
        .map(|(_, file)| file)
        .collect()
}

fn provider_file_matches_subscription_season(sub: &Subscription, file: &ProviderFile) -> bool {
    sub.media_type == "movie"
        || crate::services::episode::matches_subscription_season_range(
            &file.name,
            &file.parent_path,
            sub.season_start(),
            sub.season_end_inclusive(),
        )
}

fn has_rename_rules(rules: &TransferRules) -> bool {
    !rules.rename_template.trim().is_empty() || !rules.rename_regex.trim().is_empty()
}

fn filter_rename_candidates(
    video_files: Vec<DriveItem>,
    expected_names: Option<&HashSet<String>>,
) -> Vec<DriveItem> {
    match expected_names {
        Some(names) if !names.is_empty() => video_files
            .into_iter()
            .filter(|file| names.contains(&file.name))
            .collect(),
        _ => video_files,
    }
}

#[derive(Debug, Clone)]
struct TransferMatchTargets {
    names: HashSet<String>,
    episode_keys: HashSet<(i32, i32)>,
}

impl TransferMatchTargets {
    fn from_file_names(sub: &Subscription, file_names: &[String]) -> Self {
        Self {
            names: file_names.iter().cloned().collect(),
            episode_keys: file_names
                .iter()
                .filter_map(|name| episode_video_key(name, sub.season))
                .collect(),
        }
    }

    fn matches_name(&self, sub: &Subscription, name: &str) -> bool {
        self.names.contains(name)
            || episode_video_key(name, sub.season)
                .map(|key| self.episode_keys.contains(&key))
                .unwrap_or(false)
    }
}

fn filter_transfer_candidates_by_targets<'a>(
    sub: &Subscription,
    files: impl IntoIterator<Item = &'a ProviderFile>,
    targets: &TransferMatchTargets,
) -> Vec<&'a ProviderFile> {
    files
        .into_iter()
        .filter(|file| {
            provider_file_matches_subscription_season(sub, file)
                && targets.matches_name(sub, &file.name)
        })
        .collect()
}

#[derive(Debug, Clone)]
struct RenameResult {
    renamed_count: usize,
    files: Vec<DriveItem>,
}

#[derive(Debug, Clone)]
struct SyncDownloadReport {
    submitted_count: usize,
    dir: String,
    error: Option<String>,
    items: Vec<SyncDownloadItem>,
}

#[derive(Debug, Clone)]
struct SyncDownloadItem {
    gid: String,
    file_name: String,
}

fn merge_sync_download_reports(reports: Vec<SyncDownloadReport>) -> Option<SyncDownloadReport> {
    if reports.is_empty() {
        return None;
    }
    let mut submitted_count = 0usize;
    let mut items = Vec::new();
    let mut errors = Vec::new();
    let mut dirs = Vec::new();
    for report in reports {
        submitted_count += report.submitted_count;
        items.extend(report.items);
        if let Some(error) = report.error {
            if !error.trim().is_empty() {
                errors.push(error);
            }
        }
        if !report.dir.trim().is_empty() && !dirs.iter().any(|dir| dir == &report.dir) {
            dirs.push(report.dir);
        }
    }
    Some(SyncDownloadReport {
        submitted_count,
        dir: dirs.join("；"),
        error: (!errors.is_empty()).then(|| errors.join("；")),
        items,
    })
}

#[derive(Debug, Clone)]
struct StrmGenerationReport {
    generated_count: usize,
    dir: String,
    error: Option<String>,
}

fn append_path(base: &str, segment: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let segment = segment.trim().trim_matches('/');
    if segment.is_empty() {
        return base.to_string();
    }
    if base.is_empty() || base == "/" {
        format!("/{}", segment)
    } else {
        format!("{}/{}", base, segment)
    }
}

fn sanitize_path_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' => ' ',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if sanitized.trim().is_empty() {
        "未命名".to_string()
    } else {
        sanitized.trim().to_string()
    }
}

fn metadata_year(sub: &Subscription) -> Option<String> {
    sub.metadata
        .as_ref()
        .and_then(|metadata| metadata.release_date.as_deref())
        .and_then(|date| date.get(0..4))
        .filter(|year| year.chars().all(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
}

fn media_folder_name(sub: &Subscription) -> String {
    let title = sub
        .metadata
        .as_ref()
        .map(|metadata| metadata.title.as_str())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&sub.title);
    let title = sanitize_path_segment(title);
    match metadata_year(sub) {
        Some(year) => format!("{}（{}）", title, year),
        None => title,
    }
}

fn season_folder_name(season: i32) -> String {
    format!("Season {}", season.max(1))
}

fn has_season_suffix(path: &str) -> bool {
    let last = path
        .trim()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    last.strip_prefix("season ")
        .and_then(|number| number.parse::<i32>().ok())
        .is_some()
}

fn category_directory(sub: &Subscription, settings: &Settings) -> String {
    match sub.media_type.as_str() {
        "movie" => settings.quark_save_movie_dir.clone(),
        "series" => settings.quark_save_series_dir.clone(),
        "anime" => settings.quark_save_anime_dir.clone(),
        _ => settings
            .custom_categories
            .iter()
            .find(|cat| sub.media_type == format!("custom_{}", cat.id))
            .map(|cat| cat.dir.clone())
            .unwrap_or_else(|| settings.quark_save_root.clone()),
    }
}

fn media_type_aria2_directory(sub: &Subscription, settings: &Settings) -> String {
    let dir = match sub.media_type.as_str() {
        "movie" => settings.aria2_movie_dir.trim(),
        "series" => settings.aria2_series_dir.trim(),
        "anime" => settings.aria2_anime_dir.trim(),
        _ => settings
            .custom_categories
            .iter()
            .find(|cat| sub.media_type == format!("custom_{}", cat.id))
            .map(|cat| cat.aria2_dir.trim())
            .unwrap_or(""),
    };

    dir.to_string()
}

fn strip_season_suffix(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches('/');
    if !has_season_suffix(trimmed) {
        return trimmed.to_string();
    }
    match trimmed.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => parent.to_string(),
        _ => trimmed.to_string(),
    }
}

/// 剧集根目录（不含 Season N）。多季订阅转存时作为父路径。
fn determine_subscription_show_root(sub: &Subscription, settings: &Settings) -> String {
    let target_dir = if sub.rules.target_dir.trim().is_empty() {
        append_path(&category_directory(sub, settings), &media_folder_name(sub))
    } else {
        sub.rules.target_dir.trim().to_string()
    };
    if matches!(sub.media_type.as_str(), "series" | "anime") {
        strip_season_suffix(&target_dir)
    } else {
        target_dir
    }
}

fn determine_subscription_target_directory(sub: &Subscription, settings: &Settings) -> String {
    let mut target_dir = if sub.rules.target_dir.trim().is_empty() {
        append_path(&category_directory(sub, settings), &media_folder_name(sub))
    } else {
        sub.rules.target_dir.trim().to_string()
    };

    if matches!(sub.media_type.as_str(), "series" | "anime") {
        if sub.is_multi_season() {
            // 多季：只保留剧名目录，具体 Season N 在转存时按文件季号创建
            target_dir = strip_season_suffix(&target_dir);
        } else if !has_season_suffix(&target_dir) {
            target_dir = append_path(&target_dir, &season_folder_name(sub.season_start()));
        }
    }

    target_dir
}

fn season_target_directory(show_root: &str, season: i32) -> String {
    let root = strip_season_suffix(show_root);
    if root.is_empty() || root == "/" {
        return season_folder_name(season);
    }
    append_path(&root, &season_folder_name(season))
}

/// Aria2 本地下载根目录：优先订阅自定义路径，否则媒体类型默认目录。
/// 剧集/动画且未自定义时，在类型目录下再拼剧名，便于自动建 Season 子目录。
fn resolve_sync_download_base(sub: &Subscription, settings: &Settings) -> String {
    let custom = sub.sync_download_dir.trim();
    if !custom.is_empty() {
        return strip_season_suffix(custom);
    }
    let category = media_type_aria2_directory(sub, settings);
    if category.is_empty() {
        return String::new();
    }
    if matches!(sub.media_type.as_str(), "series" | "anime") {
        append_path(&category, &media_folder_name(sub))
    } else {
        category
    }
}

/// 剧集/动画自动落到 `…/Season N`；电影保持原路径。
fn resolve_sync_download_dir_for_season(
    sub: &Subscription,
    settings: &Settings,
    season: i32,
) -> String {
    let base = resolve_sync_download_base(sub, settings);
    if base.is_empty() {
        return base;
    }
    if matches!(sub.media_type.as_str(), "series" | "anime") {
        if has_season_suffix(&base) {
            base
        } else {
            append_path(&base, &season_folder_name(season.max(1)))
        }
    } else {
        base
    }
}

fn transfer_reason(
    target_dir: &str,
    sync_report: Option<&SyncDownloadReport>,
    strm_report: Option<&StrmGenerationReport>,
) -> String {
    let target = if target_dir.trim().is_empty() {
        "根目录"
    } else {
        target_dir
    };
    let mut parts = vec![format!("已转存到 {}", target)];
    if let Some(report) = sync_report {
        parts.push(match report {
            report if report.submitted_count > 0 && report.error.is_none() => format!(
                "已提交 {} 个 Aria2 同步下载任务到 {}",
                report.submitted_count,
                sync_dir_label(&report.dir)
            ),
            report if report.submitted_count > 0 => format!(
                "已提交 {} 个 Aria2 同步下载任务，部分失败: {}",
                report.submitted_count,
                report.error.as_deref().unwrap_or("未知错误")
            ),
            report => format!(
                "同步下载失败: {}",
                report.error.as_deref().unwrap_or("未知错误")
            ),
        });
    }
    if let Some(report) = strm_report {
        parts.push(match report {
            report if report.generated_count > 0 && report.error.is_none() => format!(
                "已生成 {} 个 STRM 文件到 {}",
                report.generated_count, report.dir
            ),
            report if report.generated_count > 0 => format!(
                "已生成 {} 个 STRM 文件，部分失败: {}",
                report.generated_count,
                report.error.as_deref().unwrap_or("未知错误")
            ),
            report => format!(
                "STRM 生成失败: {}",
                report.error.as_deref().unwrap_or("未知错误")
            ),
        });
    }
    parts.join("，")
}

fn sync_dir_label(dir: &str) -> &str {
    if dir.trim().is_empty() {
        "Aria2 自身目录"
    } else {
        dir
    }
}

fn now() -> i64 {
    unix_now()
}

fn transfer_notification_message(
    file_count: usize,
    target_dir: &str,
    sync_report: Option<&SyncDownloadReport>,
    strm_report: Option<&StrmGenerationReport>,
) -> String {
    let mut parts = vec![format!("已转存 {} 个文件到 {}", file_count, target_dir)];
    if let Some(report) = sync_report {
        parts.push(match report {
            report if report.submitted_count > 0 && report.error.is_none() => format!(
                "提交 {} 个 Aria2 下载任务到 {}",
                report.submitted_count,
                sync_dir_label(&report.dir)
            ),
            report if report.submitted_count > 0 => format!(
                "提交 {} 个 Aria2 下载任务，部分失败: {}",
                report.submitted_count,
                report.error.as_deref().unwrap_or("未知错误")
            ),
            report => format!(
                "同步下载失败: {}",
                report.error.as_deref().unwrap_or("未知错误")
            ),
        });
    }
    if let Some(report) = strm_report {
        parts.push(match report {
            report if report.generated_count > 0 && report.error.is_none() => format!(
                "生成 {} 个 STRM 文件到 {}",
                report.generated_count, report.dir
            ),
            report if report.generated_count > 0 => format!(
                "生成 {} 个 STRM 文件，部分失败: {}",
                report.generated_count,
                report.error.as_deref().unwrap_or("未知错误")
            ),
            report => format!(
                "STRM 生成失败: {}",
                report.error.as_deref().unwrap_or("未知错误")
            ),
        });
    }
    parts.join("，")
}

async fn wait_for_rename_candidates<C, Fut>(
    mut collect_video_files: C,
    expected_names: Option<&HashSet<String>>,
    max_attempts: usize,
    retry_delay: Duration,
) -> Result<Vec<DriveItem>>
where
    C: FnMut() -> Fut,
    Fut: Future<Output = Result<Vec<DriveItem>>>,
{
    let expected_count = expected_names
        .as_ref()
        .map(|names| names.len())
        .unwrap_or_default();
    let mut rename_candidates = Vec::new();

    for attempt in 1..=max_attempts {
        let video_files = collect_video_files().await?;
        rename_candidates = filter_rename_candidates(video_files, expected_names);

        if expected_count > 0 {
            if rename_candidates.len() >= expected_count {
                break;
            }
            info!(
                "本次转存视频暂未全部出现，已看到 {}/{}，等待后重试 ({}/{})",
                rename_candidates.len(),
                expected_count,
                attempt,
                max_attempts
            );
        } else if !rename_candidates.is_empty() {
            break;
        } else {
            info!(
                "目标目录暂未看到视频文件，等待后重试 ({}/{})",
                attempt, max_attempts
            );
        }

        if !retry_delay.is_zero() {
            tokio::time::sleep(retry_delay).await;
        }
    }

    Ok(rename_candidates)
}

