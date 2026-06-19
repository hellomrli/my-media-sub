use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::clients::quark::{QuarkFile, QuarkShareProbe};
use crate::clients::quark_save::{NormalizedItem, QuarkSaveClient};
use crate::clients::Aria2Client;
use crate::error::{AppError, Result};
use crate::models::rules::TransferRules;
use crate::models::subscription::Subscription;
use crate::models::Settings;
use crate::services::notification::add_notification;
use crate::services::transfer_rule::apply_rename;
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

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
struct RenameResult {
    renamed_count: usize,
    files: Vec<NormalizedItem>,
}

#[derive(Debug, Clone)]
struct SyncDownloadReport {
    submitted_count: usize,
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

fn transfer_reason(target_dir: &str, sync_report: Option<&SyncDownloadReport>) -> String {
    let target = if target_dir.trim().is_empty() {
        "根目录"
    } else {
        target_dir
    };
    match sync_report {
        Some(report) if report.submitted_count > 0 && report.error.is_none() => format!(
            "已转存到 {}，已提交 {} 个 Aria2 同步下载任务到 {}",
            target,
            report.submitted_count,
            sync_dir_label(&report.dir)
        ),
        Some(report) if report.submitted_count > 0 => format!(
            "已转存到 {}，已提交 {} 个 Aria2 同步下载任务，部分失败: {}",
            target,
            report.submitted_count,
            report.error.as_deref().unwrap_or("未知错误")
        ),
        Some(report) => format!(
            "已转存到 {}，同步下载失败: {}",
            target,
            report.error.as_deref().unwrap_or("未知错误")
        ),
        None => format!("已转存到 {}", target),
    }
}

fn sync_dir_label(dir: &str) -> &str {
    if dir.trim().is_empty() {
        "Aria2 默认目录"
    } else {
        dir
    }
}

fn transfer_notification_message(
    file_count: usize,
    target_dir: &str,
    sync_report: Option<&SyncDownloadReport>,
) -> String {
    match sync_report {
        Some(report) if report.submitted_count > 0 && report.error.is_none() => format!(
            "已转存 {} 个文件到 {}，并提交 {} 个 Aria2 下载任务到 {}",
            file_count,
            target_dir,
            report.submitted_count,
            sync_dir_label(&report.dir)
        ),
        Some(report) if report.submitted_count > 0 => format!(
            "已转存 {} 个文件到 {}，已提交 {} 个 Aria2 下载任务，部分失败: {}",
            file_count,
            target_dir,
            report.submitted_count,
            report.error.as_deref().unwrap_or("未知错误")
        ),
        Some(report) => format!(
            "已转存 {} 个文件到 {}，但同步下载失败: {}",
            file_count,
            target_dir,
            report.error.as_deref().unwrap_or("未知错误")
        ),
        None => format!("已转存 {} 个文件到 {}", file_count, target_dir),
    }
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

/// 订阅自动转存服务
pub struct SubscriptionTransferService {
    subscription_store: Arc<SubscriptionStore>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
}

impl SubscriptionTransferService {
    pub fn new(
        subscription_store: Arc<SubscriptionStore>,
        settings_store: Arc<SettingsStore>,
        notification_store: Arc<NotificationStore>,
    ) -> Self {
        Self {
            subscription_store,
            settings_store,
            notification_store,
        }
    }

    /// 自动转存订阅的新文件
    /// 在 check_subscription 发现新文件后调用
    pub async fn auto_transfer_new_files(
        &self,
        subscription_id: &str,
        new_file_names: &[String],
    ) -> Result<TransferResult> {
        let sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        // 检查是否启用自动转存
        if sub.notify_only {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "订阅设置为仅通知模式".to_string(),
                push_title: None,
                push_message: None,
            });
        }

        let settings = self.settings_store.get().await;

        if !settings.auto_download_new_subscription_items {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "自动下载新订阅项未启用".to_string(),
                push_title: None,
                push_message: None,
            });
        }

        if !settings.quark_save_enabled {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "全局自动转存未启用".to_string(),
                push_title: None,
                push_message: None,
            });
        }

        let cookie = settings.quark_cookie.clone();
        if cookie.is_empty() {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "未配置夸克 Cookie".to_string(),
                push_title: None,
                push_message: None,
            });
        }

        if new_file_names.is_empty() {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "无新文件需要转存".to_string(),
                push_title: None,
                push_message: None,
            });
        }

        info!(
            "开始自动转存订阅 {} 的 {} 个新文件",
            sub.title,
            new_file_names.len()
        );

        // 1. 探测分享链接获取文件信息
        let probe = QuarkShareProbe::new(cookie.clone());
        let share_info = probe.probe(&sub.url, &sub.password, 200).await;

        if !share_info.ok {
            warn!("探测分享链接失败: {}", share_info.message);
            return Err(AppError::Http(format!(
                "探测分享链接失败: {}",
                share_info.message
            )));
        }

        // 2. 筛选出新文件
        let files_to_transfer: Vec<&QuarkFile> = share_info
            .files
            .iter()
            .filter(|f| new_file_names.contains(&f.name))
            .collect();

        if files_to_transfer.is_empty() {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "未找到匹配的文件".to_string(),
                push_title: None,
                push_message: None,
            });
        }

        // 3. 确定目标目录
        let target_dir = self.determine_target_directory(&sub, &settings);
        let save_client = QuarkSaveClient::new(cookie.clone());

        let target_fid = if target_dir.is_empty() || target_dir == "/" {
            "0".to_string()
        } else {
            match save_client.ensure_dir_path(&target_dir).await {
                Ok(fid) => fid,
                Err(e) => {
                    warn!("创建/查找目标目录失败: {}, 使用根目录", e);
                    "0".to_string()
                }
            }
        };

        // 4. 提取 pwd_id
        let pwd_id = match QuarkShareProbe::extract_pwd_id(&sub.url) {
            Some(id) => id,
            None => {
                return Err(AppError::Validation("无法提取分享链接 ID".to_string()));
            }
        };

        // 5. 重新获取最新的 stoken 和 share_fid_token
        let (stoken, err) = probe.get_share_token(&pwd_id, &sub.password).await?;
        if let Some(err_msg) = err {
            return Err(AppError::Http(format!("获取分享 token 失败: {}", err_msg)));
        }

        let stoken = stoken.ok_or_else(|| AppError::Http("未能获取分享 token".to_string()))?;

        // 6. 重新列出文件获取最新 token
        let (fresh_files, err) = probe.list_share_files(&pwd_id, &stoken, "0").await?;
        if let Some(err_msg) = err {
            return Err(AppError::Http(format!("获取文件列表失败: {}", err_msg)));
        }

        // 7. 收集 fid 和 share_fid_token
        let mut fid_list = Vec::new();
        let mut fid_token_list = Vec::new();

        for item in &fresh_files {
            let fid = item
                .get("fid")
                .or_else(|| item.get("file_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name = item
                .get("file_name")
                .or_else(|| item.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let share_fid_token = item
                .get("share_fid_token")
                .or_else(|| item.get("file_token"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // 只转存新文件
            if !fid.is_empty()
                && !share_fid_token.is_empty()
                && new_file_names.contains(&name.to_string())
            {
                fid_list.push(fid.to_string());
                fid_token_list.push(share_fid_token.to_string());
            }
        }

        if fid_list.is_empty() {
            return Err(AppError::Validation(
                "没有可转存的文件（缺少 fid 或 token）".to_string(),
            ));
        }

        // 8. 执行转存
        info!("转存 {} 个文件到 {}", fid_list.len(), target_dir);
        save_client
            .save_share_files(&pwd_id, &stoken, &fid_list, &fid_token_list, &target_fid)
            .await?;

        // 9. 等待转存文件落盘，并按规则重命名
        let transferred_files = if has_rename_rules(&sub.rules) {
            info!("开始按订阅规则重命名文件");
            self.rename_transferred_files(&save_client, &target_fid, &sub, Some(new_file_names))
                .await?
                .files
        } else {
            let expected_names = expected_video_names(new_file_names);
            wait_for_rename_candidates(
                || collect_video_files_recursive(&save_client, &target_fid),
                Some(&expected_names),
                30,
                Duration::from_secs(2),
            )
            .await?
        };

        // 10. 更新订阅的 transferred_files
        self.mark_files_as_transferred(&sub.id, new_file_names)
            .await?;

        // 11. 如果订阅开启了同步下载，提交 Aria2 下载任务
        let sync_report = self
            .submit_sync_downloads(&save_client, &settings, &sub, &transferred_files)
            .await;

        // 12. 发送转存成功通知
        let (push_title, push_message) = self
            .send_transfer_notification(&sub, new_file_names, &target_dir, sync_report.as_ref())
            .await;

        info!("成功转存 {} 个文件", fid_list.len());
        let reason = transfer_reason(&target_dir, sync_report.as_ref());

        Ok(TransferResult {
            subscription_id: sub.id.clone(),
            transferred_count: fid_list.len(),
            skipped: false,
            reason,
            push_title: Some(push_title),
            push_message: Some(push_message),
        })
    }

    /// 重命名转存后的文件
    async fn rename_transferred_files(
        &self,
        save_client: &QuarkSaveClient,
        target_fid: &str,
        sub: &Subscription,
        expected_file_names: Option<&[String]>,
    ) -> Result<RenameResult> {
        use crate::services::detect_episode;

        info!("开始重命名文件，目标目录 fid: {}", target_fid);

        let expected_names = expected_file_names.map(expected_video_names);
        let expected_count = expected_names
            .as_ref()
            .map(HashSet::len)
            .unwrap_or_default();
        let max_attempts = if expected_count > 0 { 30 } else { 10 };

        // 夸克转存接口可能先返回成功，再异步落盘；自动转存时等待本次新增视频出现。
        let rename_candidates = wait_for_rename_candidates(
            || collect_video_files_recursive(save_client, target_fid),
            expected_names.as_ref(),
            max_attempts,
            Duration::from_secs(2),
        )
        .await?;
        info!("找到 {} 个待重命名视频文件", rename_candidates.len());

        let mut renamed_count = 0;
        let mut files = Vec::new();

        // 按订阅规则重命名目标目录下的视频文件。
        for video_file in &rename_candidates {
            let mut final_file = video_file.clone();
            let episode_info = detect_episode(&video_file.file_name);
            if sub.rules.rename_template.contains("{}") && episode_info.episode.is_none() {
                info!("无法从 {} 提取集数，跳过重命名", video_file.file_name);
                files.push(final_file);
                continue;
            }

            let (new_name, rename_error) = apply_rename(
                &video_file.file_name,
                &sub.rules,
                Some(sub),
                episode_info.episode,
            );
            if let Some(err) = rename_error {
                warn!("生成重命名结果失败 {}: {}", video_file.file_name, err);
                files.push(final_file);
                continue;
            }

            // 如果新旧文件名相同，跳过
            if new_name == video_file.file_name {
                info!("文件名已经匹配模板，跳过: {}", video_file.file_name);
                files.push(final_file);
                continue;
            }

            // 执行重命名
            info!("重命名: {} -> {}", video_file.file_name, new_name);
            let parent_fid = if video_file.parent_fid.trim().is_empty() {
                None
            } else {
                Some(video_file.parent_fid.as_str())
            };
            match save_client
                .rename_item(&video_file.fid, &new_name, parent_fid)
                .await
            {
                Ok(_) => {
                    renamed_count += 1;
                    final_file.file_name = new_name.clone();
                    info!("重命名成功: {}", new_name);
                }
                Err(e) => warn!("重命名失败 {}: {}", video_file.file_name, e),
            }
            files.push(final_file);
        }

        Ok(RenameResult {
            renamed_count,
            files,
        })
    }

    /// 按订阅规则重命名目标目录中的现有视频文件。
    pub async fn rename_existing_files(&self, subscription_id: &str) -> Result<usize> {
        let sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        if !has_rename_rules(&sub.rules) {
            return Err(AppError::Validation("订阅未配置重命名规则".to_string()));
        }

        let settings = self.settings_store.get().await;
        if settings.quark_cookie.trim().is_empty() {
            return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
        }

        let save_client = QuarkSaveClient::new(settings.quark_cookie.clone());
        let target_dir = self.determine_target_directory(&sub, &settings);
        let target_fid = save_client.ensure_dir_path(&target_dir).await?;

        info!(
            "开始修复订阅 {} 目标目录 {} 的文件命名",
            sub.title, target_dir
        );
        self.rename_transferred_files(&save_client, &target_fid, &sub, None)
            .await
            .map(|result| result.renamed_count)
    }

    async fn submit_sync_downloads(
        &self,
        save_client: &QuarkSaveClient,
        settings: &Settings,
        sub: &Subscription,
        files: &[NormalizedItem],
    ) -> Option<SyncDownloadReport> {
        if !sub.sync_download_enabled {
            return None;
        }

        let dir = sub.sync_download_dir.trim();
        let dir = if dir.is_empty() {
            settings.aria2_dir.trim()
        } else {
            dir
        };

        if settings.aria2_rpc_url.trim().is_empty() {
            let error = "未配置 Aria2 RPC URL".to_string();
            warn!("订阅 {} 同步下载跳过: {}", sub.title, error);
            return Some(SyncDownloadReport {
                submitted_count: 0,
                dir: dir.to_string(),
                error: Some(error),
            });
        }

        let mut fids: Vec<String> = files
            .iter()
            .filter(|file| file.file && !file.fid.trim().is_empty())
            .map(|file| file.fid.clone())
            .collect();
        fids.sort();
        fids.dedup();

        if fids.is_empty() {
            let error = "没有可同步下载的视频文件".to_string();
            warn!("订阅 {} 同步下载跳过: {}", sub.title, error);
            return Some(SyncDownloadReport {
                submitted_count: 0,
                dir: dir.to_string(),
                error: Some(error),
            });
        }

        let aria2 = Aria2Client::new(
            settings.aria2_rpc_url.clone(),
            settings.aria2_secret.clone(),
            dir.to_string(),
        );

        let download_infos = match save_client.download_infos(&fids).await {
            Ok(infos) => infos,
            Err(e) => {
                let error = format!("获取夸克下载直链失败: {}", e);
                warn!("订阅 {} 同步下载失败: {}", sub.title, error);
                return Some(SyncDownloadReport {
                    submitted_count: 0,
                    dir: dir.to_string(),
                    error: Some(error),
                });
            }
        };

        let mut submitted_count = 0usize;
        let mut last_error = None;
        for info in download_infos {
            match aria2
                .add_uri(&info.download_url, Some(&info.file_name), &info.headers)
                .await
            {
                Ok(gid) => {
                    submitted_count += 1;
                    info!("已提交 Aria2 同步下载: {} ({})", info.file_name, gid);
                }
                Err(e) => {
                    let error = format!("提交 {} 到 Aria2 失败: {}", info.file_name, e);
                    warn!("订阅 {} 同步下载失败: {}", sub.title, error);
                    last_error = Some(error);
                }
            }
        }

        Some(SyncDownloadReport {
            submitted_count,
            dir: dir.to_string(),
            error: last_error,
        })
    }

    /// 确定目标目录
    fn determine_target_directory(&self, sub: &Subscription, settings: &Settings) -> String {
        determine_subscription_target_directory(sub, settings)
    }

    /// 标记文件为已转存
    async fn mark_files_as_transferred(
        &self,
        subscription_id: &str,
        file_names: &[String],
    ) -> Result<()> {
        self.subscription_store
            .update(subscription_id, |sub| {
                for name in file_names {
                    if !sub.transferred_files.contains(name) {
                        sub.transferred_files.push(name.clone());
                    }
                }
                sub.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
            })
            .await?;

        Ok(())
    }

    /// 发送转存通知
    async fn send_transfer_notification(
        &self,
        sub: &Subscription,
        file_names: &[String],
        target_dir: &str,
        sync_report: Option<&SyncDownloadReport>,
    ) -> (String, String) {
        let target_dir_label = if target_dir.is_empty() {
            "根目录"
        } else {
            target_dir
        };
        let message =
            transfer_notification_message(file_names.len(), target_dir_label, sync_report);
        let meta = std::collections::HashMap::from([
            (
                "mode".to_string(),
                serde_json::Value::String("auto".to_string()),
            ),
            (
                "subscription_id".to_string(),
                serde_json::Value::String(sub.id.clone()),
            ),
            (
                "subscription_title".to_string(),
                serde_json::Value::String(sub.title.clone()),
            ),
            (
                "target_dir".to_string(),
                serde_json::Value::String(target_dir_label.to_string()),
            ),
            (
                "saved_count".to_string(),
                serde_json::Value::Number(serde_json::Number::from(file_names.len())),
            ),
            (
                "file_names".to_string(),
                serde_json::Value::Array(
                    file_names
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            ),
        ]);

        let title = format!("订阅自动转存: {}", sub.title);
        let _ = add_notification(
            &self.notification_store,
            "success",
            "subscription_transferred",
            title.clone(),
            message.clone(),
            meta,
        )
        .await;
        (title, message)
    }
}

/// 转存结果
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TransferResult {
    pub subscription_id: String,
    pub transferred_count: usize,
    pub skipped: bool,
    pub reason: String,
    pub push_title: Option<String>,
    pub push_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MediaMetadata, MetadataProvider, Settings};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    fn test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "my_media_sub_transfer_{}_{}_{}.json",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn video_item(name: &str) -> NormalizedItem {
        NormalizedItem {
            fid: format!("fid-{name}"),
            parent_fid: "parent".to_string(),
            file_name: name.to_string(),
            file: true,
            is_dir: false,
            size: 0,
            updated_at: String::new(),
        }
    }

    fn subscription(media_type: &str, season: i32) -> Subscription {
        Subscription {
            id: "sub".to_string(),
            title: "庆余年".to_string(),
            source_title: String::new(),
            media_type: media_type.to_string(),
            season,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            metadata: Some(MediaMetadata {
                provider: MetadataProvider::Tmdb,
                provider_id: "1".to_string(),
                title: "庆余年".to_string(),
                original_title: String::new(),
                media_type: media_type.to_string(),
                overview: String::new(),
                poster_url: None,
                backdrop_url: None,
                release_date: Some("2024-01-01".to_string()),
                vote_average: None,
                number_of_episodes: None,
                number_of_seasons: None,
                seasons: vec![],
            }),
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            enabled: true,
            completed: false,
            rules: TransferRules::default(),
            created_at: 1,
            updated_at: 1,
            last_checked_at: 1,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
        }
    }

    #[test]
    fn determine_target_directory_uses_media_folder_and_season_for_series() {
        let settings = Settings {
            quark_save_series_dir: "/连续剧".to_string(),
            ..Default::default()
        };
        let sub = subscription("series", 1);

        let target = determine_subscription_target_directory(&sub, &settings);

        assert_eq!(target, "/连续剧/庆余年（2024）/Season 1");
    }

    #[test]
    fn determine_target_directory_does_not_append_season_for_movie() {
        let settings = Settings {
            quark_save_movie_dir: "/电影".to_string(),
            ..Default::default()
        };
        let sub = subscription("movie", 1);

        let target = determine_subscription_target_directory(&sub, &settings);

        assert_eq!(target, "/电影/庆余年（2024）");
    }

    #[test]
    fn determine_target_directory_keeps_existing_season_suffix() {
        let settings = Settings {
            quark_save_anime_dir: "/动画".to_string(),
            ..Default::default()
        };
        let mut sub = subscription("anime", 2);
        sub.rules.target_dir = "/动画/孤独摇滚（2022）/Season 2".to_string();

        let target = determine_subscription_target_directory(&sub, &settings);

        assert_eq!(target, "/动画/孤独摇滚（2022）/Season 2");
    }

    #[test]
    fn expected_video_names_only_keeps_videos() {
        let names = vec![
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4".to_string(),
            "poster.jpg".to_string(),
            "Episode.06.mkv".to_string(),
        ];

        let expected = expected_video_names(&names);

        assert_eq!(expected.len(), 2);
        assert!(expected.contains("Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"));
        assert!(expected.contains("Episode.06.mkv"));
        assert!(!expected.contains("poster.jpg"));
    }

    #[test]
    fn filter_rename_candidates_limits_auto_rename_to_expected_names() {
        let expected = expected_video_names(&[
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4".to_string(),
        ]);
        let candidates = vec![
            video_item("Joy.of.Life.2019.S01.EP04.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"),
            video_item("Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"),
        ];

        let filtered = filter_rename_candidates(candidates, Some(&expected));

        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].file_name,
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"
        );
    }

    #[test]
    fn filter_rename_candidates_keeps_all_for_manual_repair() {
        let candidates = vec![video_item("Episode.01.mp4"), video_item("Episode.02.mp4")];

        let filtered = filter_rename_candidates(candidates, None);

        assert_eq!(filtered.len(), 2);
    }

    #[tokio::test]
    async fn auto_transfer_new_files_respects_subscription_auto_download_switch() {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("notifications")));
        subscriptions
            .create(subscription("series", 1))
            .await
            .unwrap();
        settings
            .update(|settings| {
                settings.auto_download_new_subscription_items = false;
                settings.quark_save_enabled = true;
                settings.quark_cookie = "cookie".to_string();
            })
            .await
            .unwrap();

        let service = SubscriptionTransferService::new(subscriptions, settings, notifications);
        let result = service
            .auto_transfer_new_files("sub", &["Episode.01.mkv".to_string()])
            .await
            .unwrap();

        assert!(result.skipped);
        assert_eq!(result.transferred_count, 0);
        assert_eq!(result.reason, "自动下载新订阅项未启用");
    }

    #[tokio::test]
    async fn wait_for_rename_candidates_waits_for_expected_transfer_file() {
        let expected = expected_video_names(&[
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4".to_string(),
        ]);
        let responses = Arc::new(Mutex::new(VecDeque::from([
            vec![video_item(
                "Joy.of.Life.2019.S01.EP04.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4",
            )],
            vec![
                video_item("Joy.of.Life.2019.S01.EP04.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"),
                video_item("Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"),
            ],
        ])));
        let attempts = Arc::new(Mutex::new(0usize));

        let candidates = wait_for_rename_candidates(
            || {
                let responses = responses.clone();
                let attempts = attempts.clone();
                async move {
                    *attempts.lock().unwrap() += 1;
                    Ok(responses.lock().unwrap().pop_front().unwrap_or_default())
                }
            },
            Some(&expected),
            3,
            Duration::ZERO,
        )
        .await
        .unwrap();

        assert_eq!(*attempts.lock().unwrap(), 2);
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].file_name,
            "Joy.of.Life.2019.S01.EP05.WEB-DL.4K.HEVC.AAC-LeagueWEB.mp4"
        );
    }

    #[tokio::test]
    async fn wait_for_rename_candidates_stops_after_max_attempts() {
        let expected = expected_video_names(&["Episode.03.mp4".to_string()]);
        let attempts = Arc::new(Mutex::new(0usize));

        let candidates = wait_for_rename_candidates(
            || {
                let attempts = attempts.clone();
                async move {
                    *attempts.lock().unwrap() += 1;
                    Ok(vec![video_item("Episode.01.mp4")])
                }
            },
            Some(&expected),
            2,
            Duration::ZERO,
        )
        .await
        .unwrap();

        assert_eq!(*attempts.lock().unwrap(), 2);
        assert!(candidates.is_empty());
    }
}
