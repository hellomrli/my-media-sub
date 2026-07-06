/// 递归收集目录下所有视频文件（独立函数，使用 Box 解决递归问题）
fn collect_video_files_recursive<'a>(
    save_client: &'a QuarkSaveClient,
    parent_fid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<NormalizedItem>>> + Send + 'a>> {
    Box::pin(async move {
        use crate::services::is_video_name;
        let mut video_files = Vec::new();

        let items = save_client.list_dir(parent_fid).await?;

        for item in items {
            if item.is_dir {
                // 递归进入子目录
                match collect_video_files_recursive(save_client, &item.fid).await {
                    Ok(mut sub_videos) => video_files.append(&mut sub_videos),
                    Err(e) => warn!("读取子目录 {} 失败: {}", item.file_name, e),
                }
            } else if is_video_name(&item.file_name) {
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

fn dedup_quark_episode_files<'a>(
    sub: &Subscription,
    files: Vec<&'a QuarkFile>,
) -> Vec<&'a QuarkFile> {
    if sub.media_type == "movie" {
        return files;
    }

    let mut best_by_episode: std::collections::HashMap<(i32, i32), usize> =
        std::collections::HashMap::new();
    for (index, file) in files.iter().enumerate() {
        if !quark_file_matches_subscription_season(sub, file) {
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
            if !quark_file_matches_subscription_season(sub, file) {
                return false;
            }
            crate::services::episode::episode_video_key(&file.name, sub.season)
                .map(|_| selected.contains(index))
                .unwrap_or(true)
        })
        .map(|(_, file)| file)
        .collect()
}

fn quark_file_matches_subscription_season(sub: &Subscription, file: &QuarkFile) -> bool {
    sub.media_type == "movie"
        || matches_subscription_season(&file.name, &file.parent_path, sub.season)
}

fn has_rename_rules(rules: &TransferRules) -> bool {
    !rules.rename_template.trim().is_empty() || !rules.rename_regex.trim().is_empty()
}

fn filter_rename_candidates(
    video_files: Vec<NormalizedItem>,
    expected_names: Option<&HashSet<String>>,
) -> Vec<NormalizedItem> {
    match expected_names {
        Some(names) if !names.is_empty() => video_files
            .into_iter()
            .filter(|file| names.contains(&file.file_name))
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
    files: impl IntoIterator<Item = &'a QuarkFile>,
    targets: &TransferMatchTargets,
) -> Vec<&'a QuarkFile> {
    files
        .into_iter()
        .filter(|file| {
            quark_file_matches_subscription_season(sub, file)
                && targets.matches_name(sub, &file.name)
        })
        .collect()
}

#[derive(Debug, Clone)]
struct RenameResult {
    renamed_count: usize,
    files: Vec<NormalizedItem>,
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

#[derive(Debug, Clone)]
struct StrmGenerationReport {
    generated_count: usize,
    dir: String,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct ShareTransferFile {
    fid: String,
    share_fid_token: String,
    name: String,
}

fn raw_share_name(item: &std::collections::HashMap<String, serde_json::Value>) -> String {
    item.get("file_name")
        .or_else(|| item.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn raw_share_fid(item: &std::collections::HashMap<String, serde_json::Value>) -> String {
    item.get("fid")
        .or_else(|| item.get("file_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn raw_share_token(item: &std::collections::HashMap<String, serde_json::Value>) -> String {
    item.get("share_fid_token")
        .or_else(|| item.get("file_token"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn raw_share_is_dir(item: &std::collections::HashMap<String, serde_json::Value>) -> bool {
    item.get("dir").and_then(|v| v.as_bool()).unwrap_or(false)
        || (item.get("file").and_then(|v| v.as_bool()) == Some(false))
        || (item.get("file_type").and_then(|v| v.as_i64()) == Some(0)
            && !item.contains_key("format_type")
            && item.get("size").and_then(|v| v.as_i64()).unwrap_or(0) == 0)
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

fn append_share_parent_path(parent_path: &str, segment: &str) -> String {
    let parent_path = parent_path.trim().trim_matches('/');
    let segment = segment.trim().trim_matches('/');
    if segment.is_empty() {
        return parent_path.to_string();
    }
    if parent_path.is_empty() {
        segment.to_string()
    } else {
        format!("{}/{}", parent_path, segment)
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

fn determine_subscription_target_directory(sub: &Subscription, settings: &Settings) -> String {
    let mut target_dir = if sub.rules.target_dir.trim().is_empty() {
        append_path(&category_directory(sub, settings), &media_folder_name(sub))
    } else {
        sub.rules.target_dir.trim().to_string()
    };

    if matches!(sub.media_type.as_str(), "series" | "anime") && !has_season_suffix(&target_dir) {
        target_dir = append_path(&target_dir, &season_folder_name(sub.season));
    }

    target_dir
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
) -> Result<Vec<NormalizedItem>>
where
    C: FnMut() -> Fut,
    Fut: Future<Output = Result<Vec<NormalizedItem>>>,
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

async fn collect_share_transfer_files(
    probe: &QuarkShareProbe,
    sub: &Subscription,
    pwd_id: &str,
    stoken: &str,
    targets: &TransferMatchTargets,
) -> Result<Vec<ShareTransferFile>> {
    if targets.names.is_empty() && targets.episode_keys.is_empty() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let mut queue = VecDeque::from([("0".to_string(), String::new())]);
    let mut visited_dirs = HashSet::new();

    while let Some((parent_fid, parent_path)) = queue.pop_front() {
        if !visited_dirs.insert(parent_fid.clone()) {
            continue;
        }

        let (items, err) = probe.list_share_files(pwd_id, stoken, &parent_fid).await?;
        if let Some(err_msg) = err {
            return Err(AppError::Http(format!("获取文件列表失败: {}", err_msg)));
        }

        for item in items {
            let fid = raw_share_fid(&item);
            let name = raw_share_name(&item);
            if raw_share_is_dir(&item) {
                if !fid.is_empty() {
                    queue.push_back((fid, append_share_parent_path(&parent_path, &name)));
                }
                continue;
            }

            if !crate::services::is_video_name(&name) {
                continue;
            }

            if sub.media_type != "movie" {
                if !matches_subscription_season(&name, &parent_path, sub.season) {
                    continue;
                }
                if episode_video_key(&name, sub.season).is_none() {
                    continue;
                }
            }

            if !targets.matches_name(sub, &name) {
                continue;
            }

            let share_fid_token = raw_share_token(&item);
            if !fid.is_empty() && !share_fid_token.is_empty() {
                result.push(ShareTransferFile {
                    fid,
                    share_fid_token,
                    name,
                });
            }
        }
    }

    result.sort_by(|left, right| left.name.cmp(&right.name));
    result.dedup_by(|left, right| left.name == right.name && left.fid == right.fid);
    Ok(result)
}

