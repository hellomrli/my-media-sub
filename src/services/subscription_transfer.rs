use std::collections::{HashSet, VecDeque};
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
use crate::services::notification::{
    add_notification, dispatch_push_event_for_notification, PushDispatchRequest,
};
use crate::services::push::{PushEvent, PushLevel};
use crate::services::strm::{
    generate_subscription_strm_files_async, strm_generation_enabled, StrmGenerationResult,
};
use crate::services::subscription_progress::{
    completion_target_episode, should_mark_completed_from_transferred_files,
};
use crate::services::transfer_rule::{apply_rename, effective_rules, transfer_state_key};
use crate::services::{
    episode::episode_video_key, episode::is_better_episode_duplicate_candidate,
    episode::matches_subscription_season, episode::EpisodeDuplicateCandidate,
};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};
use crate::utils::unix_now;

include!("subscription_transfer/helpers.rs");
include!("subscription_transfer/notification_methods.rs");

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
    pub async fn auto_transfer_new_files_with_options(
        &self,
        subscription_id: &str,
        new_file_names: &[String],
        force_transfer: bool,
    ) -> Result<TransferResult> {
        let mut sub = self
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
                push_notification_id: None,
            });
        }

        let settings = self.settings_store.get().await;
        sub.rules = effective_rules(
            &sub.rules,
            &sub.media_type,
            &settings.default_rename_template,
        );

        if !force_transfer && !settings.auto_download_new_subscription_items {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "自动下载新订阅项未启用".to_string(),
                push_title: None,
                push_message: None,
                push_notification_id: None,
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
                push_notification_id: None,
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
                push_notification_id: None,
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
                push_notification_id: None,
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

        // 2. 筛选出待转存文件。分享方可能会改名，补转时按同集数兜底匹配。
        let match_targets = TransferMatchTargets::from_file_names(&sub, new_file_names);
        let files_to_transfer =
            filter_transfer_candidates_by_targets(&sub, &share_info.files, &match_targets);
        let files_to_transfer = dedup_quark_episode_files(&sub, files_to_transfer);
        let new_file_names: Vec<String> = files_to_transfer
            .iter()
            .map(|file| file.name.clone())
            .collect();

        if files_to_transfer.is_empty() {
            return Ok(TransferResult {
                subscription_id: sub.id.clone(),
                transferred_count: 0,
                skipped: true,
                reason: "未找到匹配的文件".to_string(),
                push_title: None,
                push_message: None,
                push_notification_id: None,
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

        // 6. 递归收集本次新增的视频文件。不要直接转存父目录，否则会在目标
        // 目录下多出一层原始分享目录，导致重命名和 Aria2 同步路径不符合预期。
        let transfer_targets = TransferMatchTargets::from_file_names(&sub, &new_file_names);
        let transfer_files =
            collect_share_transfer_files(&probe, &sub, &pwd_id, &stoken, &transfer_targets).await?;
        let transfer_file_names: Vec<String> = transfer_files
            .iter()
            .map(|file| file.name.clone())
            .collect();
        let fid_list: Vec<String> = transfer_files.iter().map(|file| file.fid.clone()).collect();
        let fid_token_list: Vec<String> = transfer_files
            .iter()
            .map(|file| file.share_fid_token.clone())
            .collect();

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
            self.rename_transferred_files(
                &save_client,
                &target_fid,
                &sub,
                Some(&transfer_file_names),
            )
            .await?
            .files
        } else {
            let expected_names = expected_video_names(&transfer_file_names);
            wait_for_rename_candidates(
                || collect_video_files_recursive(&save_client, &target_fid),
                Some(&expected_names),
                30,
                Duration::from_secs(2),
            )
            .await?
        };

        // 10. 更新订阅的 transferred_files
        self.mark_files_as_transferred(&sub, &transfer_file_names)
            .await?;

        // 11. 如果订阅开启了同步下载，提交 Aria2 下载任务
        let sync_report = self
            .submit_sync_downloads(&save_client, &settings, &sub, &transferred_files)
            .await;

        if self.complete_if_transferred_target_reached(&sub.id).await? {
            info!("订阅 {} 已达到完结集数并标记为完结", sub.title);
        }

        // 12. 如果订阅开启了 STRM，生成 HTTPStrm 文件
        let strm_report = self
            .generate_strm_files(&settings, &sub, &target_dir, &transferred_files)
            .await;

        // 13. 发送转存成功通知
        let (push_title, push_message, push_notification_id) = self
            .send_transfer_notification(
                &sub,
                &transfer_file_names,
                &target_dir,
                sync_report.as_ref(),
                strm_report.as_ref(),
            )
            .await;

        info!("成功转存 {} 个文件", fid_list.len());
        let reason = transfer_reason(&target_dir, sync_report.as_ref(), strm_report.as_ref());

        Ok(TransferResult {
            subscription_id: sub.id.clone(),
            transferred_count: fid_list.len(),
            skipped: false,
            reason,
            push_title: Some(push_title),
            push_message: Some(push_message),
            push_notification_id,
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
            if sub.media_type != "movie"
                && !matches_subscription_season(&video_file.file_name, "", sub.season)
            {
                info!(
                    "文件 {} 不属于订阅第 {} 季，跳过重命名",
                    video_file.file_name, sub.season
                );
                files.push(final_file);
                continue;
            }
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
        let mut sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        let settings = self.settings_store.get().await;
        sub.rules = effective_rules(
            &sub.rules,
            &sub.media_type,
            &settings.default_rename_template,
        );

        if !has_rename_rules(&sub.rules) {
            return Err(AppError::Validation("订阅未配置重命名规则".to_string()));
        }

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

    /// 按订阅目标目录中的现有视频补齐 STRM 文件。
    pub async fn generate_existing_strm_files(
        &self,
        subscription_id: &str,
    ) -> Result<StrmGenerationResult> {
        let sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        let settings = self.settings_store.get().await;
        if !settings.strm_enabled {
            return Err(AppError::Validation("全局 STRM 生成未启用".to_string()));
        }
        if !sub.strm_enabled {
            return Err(AppError::Validation("订阅未启用 STRM 生成".to_string()));
        }
        if settings.quark_cookie.trim().is_empty() {
            return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
        }

        let save_client = QuarkSaveClient::new(settings.quark_cookie.clone());
        let target_dir = self.determine_target_directory(&sub, &settings);
        let target_fid = save_client.ensure_dir_path(&target_dir).await?;
        let files = collect_video_files_recursive(&save_client, &target_fid).await?;

        generate_subscription_strm_files_async(&settings, &sub, &target_dir, &files).await
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
            media_type_aria2_directory(sub, settings)
        } else {
            dir.to_string()
        };

        if settings.aria2_rpc_url.trim().is_empty() {
            let error = "未配置 Aria2 RPC URL".to_string();
            warn!("订阅 {} 同步下载跳过: {}", sub.title, error);
            return Some(SyncDownloadReport {
                submitted_count: 0,
                dir: dir.clone(),
                error: Some(error),
                items: vec![],
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
                dir: dir.clone(),
                error: Some(error),
                items: vec![],
            });
        }

        let aria2 = Aria2Client::new(
            settings.aria2_rpc_url.clone(),
            settings.aria2_secret.clone(),
            dir.clone(),
        );

        let download_infos = match save_client.download_infos(&fids).await {
            Ok(infos) => infos,
            Err(e) => {
                let error = format!("获取夸克下载直链失败: {}", e);
                warn!("订阅 {} 同步下载失败: {}", sub.title, error);
                return Some(SyncDownloadReport {
                    submitted_count: 0,
                    dir: dir.clone(),
                    error: Some(error),
                    items: vec![],
                });
            }
        };

        let mut submitted_count = 0usize;
        let mut last_error = None;
        let mut items = Vec::new();
        for info in download_infos {
            match aria2
                .add_uri(&info.download_url, Some(&info.file_name), &info.headers)
                .await
            {
                Ok(gid) => {
                    submitted_count += 1;
                    info!("已提交 Aria2 同步下载: {} ({})", info.file_name, gid);
                    items.push(SyncDownloadItem {
                        gid,
                        file_name: info.file_name,
                    });
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
            dir,
            error: last_error,
            items,
        })
    }

    async fn generate_strm_files(
        &self,
        settings: &Settings,
        sub: &Subscription,
        target_dir: &str,
        files: &[NormalizedItem],
    ) -> Option<StrmGenerationReport> {
        if !strm_generation_enabled(settings, sub) {
            return None;
        }

        match generate_subscription_strm_files_async(settings, sub, target_dir, files).await {
            Ok(result) => {
                let dir = result.output_dir.display().to_string();
                info!(
                    "订阅 {} 已生成 {} 个 STRM 文件到 {}",
                    sub.title, result.generated_count, dir
                );
                Some(StrmGenerationReport {
                    generated_count: result.generated_count,
                    dir,
                    error: None,
                })
            }
            Err(e) => {
                let error = format!("{}", e);
                warn!("订阅 {} STRM 生成失败: {}", sub.title, error);
                Some(StrmGenerationReport {
                    generated_count: 0,
                    dir: settings.strm_output_dir.clone(),
                    error: Some(error),
                })
            }
        }
    }

    /// 确定目标目录
    fn determine_target_directory(&self, sub: &Subscription, settings: &Settings) -> String {
        determine_subscription_target_directory(sub, settings)
    }

    /// 标记文件为已转存
    async fn mark_files_as_transferred(
        &self,
        sub: &Subscription,
        file_names: &[String],
    ) -> Result<()> {
        let file_keys: Vec<String> = file_names
            .iter()
            .map(|name| {
                let episode = crate::services::detect_episode(name).episode;
                transfer_state_key(name, episode, sub.rules.ignore_extensions)
            })
            .collect();

        self.subscription_store
            .update(&sub.id, |sub| {
                for name in file_names {
                    if !sub.transferred_files.contains(name) {
                        sub.transferred_files.push(name.clone());
                    }
                }
                for key in &file_keys {
                    if !sub.transferred_file_keys.contains(key) {
                        sub.transferred_file_keys.push(key.clone());
                    }
                }
                sub.updated_at = unix_now();
            })
            .await?;

        Ok(())
    }

    async fn complete_if_transferred_target_reached(&self, subscription_id: &str) -> Result<bool> {
        let sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        if sub.sync_download_enabled || !should_mark_completed_from_transferred_files(&sub, &[]) {
            return Ok(false);
        }

        let now = now();
        let updated = self
            .subscription_store
            .update(subscription_id, |sub| {
                if sub.completed {
                    return;
                }
                sub.completed = true;
                sub.status = "completed".to_string();
                sub.invalid_since = None;
                sub.last_error = String::new();
                if sub.total_episode_number.is_none() {
                    sub.total_episode_number = sub.rules.finish_after_episode;
                }
                sub.updated_at = now;
            })
            .await?
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        if !sub.completed && updated.completed {
            self.send_completed_notification(&updated).await;
            return Ok(true);
        }

        Ok(false)
    }

    subscription_transfer_notification_methods!();
}

mod result;
pub use result::TransferResult;

#[cfg(test)]
include!("subscription_transfer/tests.rs");
