use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{info, warn};

use crate::clients::quark::QuarkShareProbe;
use crate::error::{AppError, Result};
use crate::jobs::{JobQueue, SubscriptionTransferPayload};
use crate::models::subscription::{CheckHistoryItem, ProbeFile, ProbeResult, Subscription};
use crate::services::episode::{
    episode_video_key, is_better_episode_duplicate_candidate, normalize_duplicate_episode_strategy,
    EpisodeDuplicateCandidate,
};
use crate::services::notification::{add_notification, dispatch_push_event};
use crate::services::push::{PushEvent, PushLevel};
use crate::services::subscription_progress::{
    completion_target_episode, reopen_completed_subscription_status,
    should_mark_completed_from_known_episodes, should_mark_completed_from_transferred_files,
    should_reopen_completed_subscription,
};
use crate::services::transfer_rule::transfer_state_key;
use crate::services::SubscriptionTransferService;
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

/// 订阅检查服务
pub struct SubscriptionCheckService {
    subscription_store: Arc<SubscriptionStore>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Option<Arc<JobQueue>>,
    transfer_service: Option<Arc<SubscriptionTransferService>>,
}

impl SubscriptionCheckService {
    pub fn new(
        subscription_store: Arc<SubscriptionStore>,
        settings_store: Arc<SettingsStore>,
        notification_store: Arc<NotificationStore>,
    ) -> Self {
        Self {
            subscription_store,
            settings_store,
            notification_store,
            job_queue: None,
            transfer_service: None,
        }
    }

    /// 设置后台任务队列，用于异步自动转存。
    pub fn with_job_queue(mut self, job_queue: Arc<JobQueue>) -> Self {
        self.job_queue = Some(job_queue);
        self
    }

    /// 设置转存服务（保留为同步回退路径）。
    #[allow(dead_code)]
    pub fn with_transfer_service(
        mut self,
        transfer_service: Arc<SubscriptionTransferService>,
    ) -> Self {
        self.transfer_service = Some(transfer_service);
        self
    }

    /// 检查单个订阅
    pub async fn check_subscription(
        &self,
        subscription_id: &str,
        cookie: &str,
    ) -> Result<CheckResult> {
        self.check_subscription_with_options(subscription_id, cookie, false)
            .await
    }

    pub async fn check_subscription_with_options(
        &self,
        subscription_id: &str,
        cookie: &str,
        force_transfer: bool,
    ) -> Result<CheckResult> {
        let sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        if !sub.enabled {
            return Err(AppError::Validation("订阅未启用".to_string()));
        }

        let sub = if should_reopen_completed_subscription(&sub) {
            self.reopen_completed_subscription(&sub).await?
        } else {
            sub
        };

        if sub.completed {
            return Err(AppError::Validation("订阅已完成".to_string()));
        }

        // 1. 探测分享链接
        info!("检查订阅: {} ({})", sub.title, sub.id);
        let probe_result = self.probe_share(&sub, cookie).await?;

        if !probe_result.ok {
            // 探测失败，标记为失效
            self.mark_subscription_invalid(&sub, &probe_result.message)
                .await?;
            return Ok(CheckResult {
                subscription_id: sub.id.clone(),
                subscription_title: sub.title.clone(),
                new_files: vec![],
                new_episodes: vec![],
                details: CheckDetails::default(),
                became_invalid: true,
                became_completed: false,
                summary: format!("链接失效: {}", probe_result.message),
            });
        }

        let auto_transfer_enabled = self
            .auto_transfer_disabled_reason(&sub, force_transfer)
            .await;

        // 2. 对比文件，找出新增文件
        let new_files = self.find_new_files(&sub, &probe_result.files);
        let new_file_names: Vec<String> = new_files.iter().map(|f| f.name.clone()).collect();
        let transfer_file_names = if auto_transfer_enabled.is_none() {
            self.transfer_candidate_file_names(&sub, &probe_result.files, &new_file_names)
        } else {
            new_file_names.clone()
        };

        // 3. 解析集数
        let new_episodes = self.parse_episodes(&new_file_names);
        let details = self.build_check_details(&sub, &probe_result.files);
        let became_completed = if sub.notify_only {
            should_mark_completed_from_known_episodes(&sub, &new_episodes)
        } else if sub.sync_download_enabled {
            false
        } else {
            should_mark_completed_from_transferred_files(&sub, &[])
        };

        // 4. 更新订阅状态
        let summary = if new_file_names.is_empty() {
            "无更新".to_string()
        } else {
            format!("发现 {} 个新文件", new_file_names.len())
        };

        self.update_subscription_after_check(
            &sub,
            &probe_result,
            &new_file_names,
            &new_episodes,
            &summary,
            became_completed,
        )
        .await?;

        // 5. 发送通知
        if !new_file_names.is_empty() && sub.rules.notify_on_update {
            self.send_update_notification(&sub, &new_file_names, &new_episodes)
                .await;
        }
        if became_completed {
            self.send_completed_notification(&sub).await;
        }

        // 6. 自动转存：优先提交后台任务，保留同步转存作为回退路径。
        if !transfer_file_names.is_empty() {
            if let Some(reason) = auto_transfer_enabled {
                info!("跳过订阅自动转存: {} ({})", sub.title, reason);
            } else if let Some(job_queue) = &self.job_queue {
                match job_queue
                    .submit_subscription_transfer(SubscriptionTransferPayload {
                        subscription_id: sub.id.clone(),
                        file_names: transfer_file_names.clone(),
                        force_transfer,
                    })
                    .await
                {
                    Ok(job) => info!("已创建订阅自动转存任务: {}", job.id),
                    Err(e) => warn!("创建订阅自动转存任务失败: {}", e),
                }
            } else if let Some(transfer_service) = &self.transfer_service {
                match transfer_service
                    .auto_transfer_new_files_with_options(
                        &sub.id,
                        &transfer_file_names,
                        force_transfer,
                    )
                    .await
                {
                    Ok(result) => {
                        if !result.skipped {
                            info!("自动转存成功: {}", result.reason);
                            if let (Some(title), Some(message)) =
                                (result.push_title, result.push_message)
                            {
                                dispatch_push_event(
                                    self.settings_store.clone(),
                                    self.notification_store.clone(),
                                    None,
                                    PushEvent::TransferSaved,
                                    title,
                                    message,
                                    PushLevel::Success,
                                )
                                .await;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("自动转存失败: {}", e);
                    }
                }
            }
        }

        Ok(CheckResult {
            subscription_id: sub.id.clone(),
            subscription_title: sub.title.clone(),
            new_files: new_file_names,
            new_episodes,
            details,
            became_invalid: false,
            became_completed,
            summary,
        })
    }

    async fn auto_transfer_disabled_reason(
        &self,
        sub: &Subscription,
        force_transfer: bool,
    ) -> Option<&'static str> {
        if sub.notify_only {
            return Some("订阅设置为仅通知模式");
        }

        let settings = self.settings_store.get().await;
        if !force_transfer && !settings.auto_download_new_subscription_items {
            return Some("自动下载新订阅项未启用");
        }
        if !settings.quark_save_enabled {
            return Some("全局自动转存未启用");
        }

        None
    }

    async fn reopen_completed_subscription(&self, sub: &Subscription) -> Result<Subscription> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        info!("订阅 {} 仍未达到完结集数，恢复为追更中", sub.title);
        self.subscription_store
            .update(&sub.id, |sub| {
                reopen_completed_subscription_status(sub);
                sub.updated_at = now;
            })
            .await?
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))
    }

    /// 检查所有启用的订阅
    pub async fn check_all_subscriptions(&self, cookie: &str) -> Result<Vec<CheckResult>> {
        let subscriptions = self.subscription_store.list().await;
        let mut results = Vec::new();

        for sub in subscriptions {
            if !sub.enabled {
                continue;
            }
            if sub.completed && !should_reopen_completed_subscription(&sub) {
                continue;
            }

            match self.check_subscription(&sub.id, cookie).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!("检查订阅 {} 失败: {}", sub.id, e);
                }
            }
        }

        Ok(results)
    }

    /// 探测分享链接
    async fn probe_share(&self, sub: &Subscription, cookie: &str) -> Result<ProbeResult> {
        if let Some(mock_result) = mock_probe_result(&sub.url)? {
            return Ok(mock_result);
        }

        let probe = QuarkShareProbe::new(cookie.to_string());
        let share_info = probe.probe(&sub.url, &sub.password, 200).await;

        let files: Vec<ProbeFile> = share_info
            .files
            .iter()
            .map(|f| ProbeFile {
                name: f.name.clone(),
                size: f.size,
                updated_at: f.updated_at.clone(),
                file_key: f.fid.clone(),
            })
            .collect();

        Ok(ProbeResult {
            ok: share_info.ok,
            state: share_info.state,
            message: share_info.message,
            files,
        })
    }

    /// 找出新增文件
    fn find_new_files(&self, sub: &Subscription, files: &[ProbeFile]) -> Vec<ProbeFile> {
        let eligible_indices: Vec<usize> = files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                (!sub.known_file_keys.contains(&file.file_key)
                    && !self.is_before_start_episode(sub, &file.name)
                    && self.known_episode_video_reason(sub, file).is_none())
                .then_some(index)
            })
            .collect();
        let selected_episode_videos =
            self.selected_episode_video_indices(sub, files, &eligible_indices);

        eligible_indices
            .into_iter()
            .filter(|index| {
                self.keep_episode_video_index(sub, &files[*index], *index, &selected_episode_videos)
            })
            .map(|index| files[index].clone())
            .collect()
    }

    fn transfer_candidate_file_names(
        &self,
        sub: &Subscription,
        files: &[ProbeFile],
        new_file_names: &[String],
    ) -> Vec<String> {
        let mut names = new_file_names.to_vec();
        let mut seen = names.iter().cloned().collect::<HashSet<_>>();
        let mut transferred_keys: HashSet<String> =
            sub.transferred_file_keys.iter().cloned().collect();
        transferred_keys.extend(sub.transferred_files.iter().map(|name| {
            let episode = extract_episode_number(name);
            transfer_state_key(name, episode, sub.rules.ignore_extensions)
        }));

        if sub.media_type == "movie" {
            return names;
        }

        for file in files {
            if self.is_before_start_episode(sub, &file.name) {
                continue;
            }
            let episode = extract_episode_number(&file.name);
            let key = transfer_state_key(&file.name, episode, sub.rules.ignore_extensions);
            if !key.starts_with("ep:") || transferred_keys.contains(&key) {
                continue;
            }
            if seen.insert(file.name.clone()) {
                names.push(file.name.clone());
            }
        }

        names
    }

    fn known_episode_video_reason(
        &self,
        sub: &Subscription,
        file: &ProbeFile,
    ) -> Option<&'static str> {
        if sub.media_type == "movie" {
            return None;
        }

        let key = episode_video_key(&file.name, sub.season)?;
        let episode = key.1;
        if sub.known_episodes.contains(&episode) {
            return Some("同集已记录");
        }

        None
    }

    fn duplicate_episode_skip_reason(&self, sub: &Subscription) -> &'static str {
        match normalize_duplicate_episode_strategy(&sub.rules.duplicate_episode_strategy) {
            "latest_upload" => "同集重复视频，已保留上传时间最新版本",
            "largest_size" => "同集重复视频，已保留文件最大版本",
            "first" => "同集重复视频，已保留最先出现版本",
            _ => "同集重复视频，已保留清晰度最高版本",
        }
    }

    fn duplicate_candidate<'a>(
        &self,
        file: &'a ProbeFile,
        order: usize,
    ) -> EpisodeDuplicateCandidate<'a> {
        EpisodeDuplicateCandidate {
            name: &file.name,
            size: file.size,
            updated_at: file.updated_at.as_deref(),
            order,
        }
    }

    fn selected_episode_video_indices(
        &self,
        sub: &Subscription,
        files: &[ProbeFile],
        candidate_indices: &[usize],
    ) -> HashSet<usize> {
        if sub.media_type == "movie" {
            return HashSet::new();
        }

        let mut best_by_episode: HashMap<(i32, i32), usize> = HashMap::new();
        for &index in candidate_indices {
            let file = &files[index];
            let Some(key) = episode_video_key(&file.name, sub.season) else {
                continue;
            };

            match best_by_episode.get(&key).copied() {
                Some(current_index) => {
                    if is_better_episode_duplicate_candidate(
                        self.duplicate_candidate(file, index),
                        self.duplicate_candidate(&files[current_index], current_index),
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

        best_by_episode.values().copied().collect()
    }

    fn keep_episode_video_index(
        &self,
        sub: &Subscription,
        file: &ProbeFile,
        index: usize,
        selected_episode_videos: &HashSet<usize>,
    ) -> bool {
        if sub.media_type == "movie" {
            return true;
        }

        episode_video_key(&file.name, sub.season)
            .map(|_| selected_episode_videos.contains(&index))
            .unwrap_or(true)
    }

    fn is_before_start_episode(&self, sub: &Subscription, file_name: &str) -> bool {
        if sub.media_type == "movie" {
            return false;
        }

        let Some(start_episode) = sub.start_episode_number else {
            return false;
        };
        if start_episode <= 1 {
            return false;
        }

        extract_episode_number(file_name)
            .map(|episode| episode < start_episode)
            .unwrap_or(false)
    }

    fn build_check_details(&self, sub: &Subscription, files: &[ProbeFile]) -> CheckDetails {
        let mut details = CheckDetails {
            scanned_count: files.len(),
            ..Default::default()
        };

        let detail_candidate_indices: Vec<usize> = files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                (!sub.known_file_keys.contains(&file.file_key)
                    && !self.is_before_start_episode(sub, &file.name)
                    && self.known_episode_video_reason(sub, file).is_none())
                .then_some(index)
            })
            .collect();
        let selected_episode_videos =
            self.selected_episode_video_indices(sub, files, &detail_candidate_indices);

        for (index, file) in files.iter().enumerate() {
            let episode = extract_episode_number(&file.name);
            let (action, reason) = if sub.known_file_keys.contains(&file.file_key) {
                details.known_count += 1;
                ("known", "已知文件")
            } else if self.is_before_start_episode(sub, &file.name) {
                details.skipped_before_start_count += 1;
                ("skip", "低于起始转存集数")
            } else if let Some(reason) = self.known_episode_video_reason(sub, file) {
                details.skipped_duplicate_episode_count += 1;
                ("skip", reason)
            } else if !self.keep_episode_video_index(sub, file, index, &selected_episode_videos) {
                details.skipped_duplicate_episode_count += 1;
                ("skip", self.duplicate_episode_skip_reason(sub))
            } else {
                details.new_count += 1;
                ("new", "新增文件")
            };

            details.items.push(CheckDetailItem {
                name: file.name.clone(),
                episode,
                file_key: file.file_key.clone(),
                action: action.to_string(),
                reason: reason.to_string(),
            });
        }

        details
    }

    /// 解析集数
    fn parse_episodes(&self, file_names: &[String]) -> Vec<i32> {
        let mut episodes = Vec::new();

        for name in file_names {
            if let Some(ep) = extract_episode_number(name) {
                if !episodes.contains(&ep) {
                    episodes.push(ep);
                }
            }
        }

        episodes.sort();
        episodes
    }

    /// 更新订阅状态
    async fn update_subscription_after_check(
        &self,
        sub: &Subscription,
        probe: &ProbeResult,
        new_files: &[String],
        new_episodes: &[i32],
        summary: &str,
        completed: bool,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.subscription_store
            .update(&sub.id, |s| {
                // 更新已知文件列表
                for file in &probe.files {
                    if !s.known_file_keys.contains(&file.file_key) {
                        s.known_files.push(file.name.clone());
                        s.known_file_keys.push(file.file_key.clone());
                    }
                }

                // 更新已知集数
                for ep in new_episodes {
                    if !s.known_episodes.contains(ep) {
                        s.known_episodes.push(*ep);
                    }
                }
                s.known_episodes.sort();

                // 更新当前集数
                if let Some(&max_ep) = s.known_episodes.iter().max() {
                    s.current_episode_number = max_ep;
                }

                // 更新检查信息
                s.last_checked_at = now;
                s.last_new_files = new_files.to_vec();
                s.last_new_episodes = new_episodes.to_vec();
                s.last_check_summary = summary.to_string();
                s.last_probe = Some(probe.clone());
                s.updated_at = now;

                // 清除错误状态
                if probe.ok {
                    s.last_error = String::new();
                    s.invalid_since = None;
                    s.status = if completed {
                        "completed".to_string()
                    } else {
                        "active".to_string()
                    };
                }

                if completed {
                    s.completed = true;
                    if s.total_episode_number.is_none() {
                        s.total_episode_number = s.rules.finish_after_episode;
                    }
                }

                // 添加检查历史
                s.check_history.insert(
                    0,
                    CheckHistoryItem {
                        time: now,
                        state: probe.state.clone(),
                        matched_count: probe.files.len() as i32,
                        transfer_count: 0, // 转存服务会更新
                        new_files: new_files.to_vec(),
                        new_episodes: new_episodes.to_vec(),
                        summary: summary.to_string(),
                    },
                );

                // 保留最近 30 条历史
                if s.check_history.len() > 30 {
                    s.check_history.truncate(30);
                }
            })
            .await?;

        Ok(())
    }

    /// 标记订阅为失效
    async fn mark_subscription_invalid(&self, sub: &Subscription, error: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.subscription_store
            .update(&sub.id, |s| {
                s.status = "invalid".to_string();
                s.last_error = error.to_string();
                s.last_checked_at = now;
                s.updated_at = now;

                if s.invalid_since.is_none() {
                    s.invalid_since = Some(now);
                }
            })
            .await?;

        // 发送失效通知
        let title = format!("订阅链接疑似失效: {}", sub.title);
        let message = error.to_string();

        if sub.rules.notify_on_invalid {
            add_notification(
                &self.notification_store,
                "warning",
                "subscription_invalid",
                title.clone(),
                message.clone(),
                std::collections::HashMap::new(),
            )
            .await?;
            dispatch_push_event(
                self.settings_store.clone(),
                self.notification_store.clone(),
                self.job_queue.clone(),
                PushEvent::SubscriptionFailed,
                title,
                message,
                PushLevel::Warning,
            )
            .await;
        }

        Ok(())
    }

    /// 发送更新通知
    async fn send_update_notification(
        &self,
        sub: &Subscription,
        new_files: &[String],
        new_episodes: &[i32],
    ) {
        let message = if new_episodes.is_empty() {
            format!("发现新文件: {}", new_files.join("、"))
        } else {
            format!(
                "发现新集: 第 {} 集",
                new_episodes
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("、")
            )
        };

        let title = format!("订阅有更新: {}", sub.title);
        let _ = add_notification(
            &self.notification_store,
            "info",
            "subscription_updated",
            title.clone(),
            message.clone(),
            std::collections::HashMap::new(),
        )
        .await;
        dispatch_push_event(
            self.settings_store.clone(),
            self.notification_store.clone(),
            self.job_queue.clone(),
            PushEvent::SubscriptionUpdated,
            title,
            message,
            PushLevel::Info,
        )
        .await;
    }

    /// 发送完结通知
    async fn send_completed_notification(&self, sub: &Subscription) {
        let total = completion_target_episode(sub).unwrap_or(sub.current_episode_number);
        let title = format!("订阅已完结: {}", sub.title);
        let message = if total > 0 {
            format!("已达到完结集数：第 {} 集", total)
        } else {
            "订阅已标记为完结".to_string()
        };

        let _ = add_notification(
            &self.notification_store,
            "success",
            "subscription_completed",
            title.clone(),
            message.clone(),
            std::collections::HashMap::new(),
        )
        .await;
        dispatch_push_event(
            self.settings_store.clone(),
            self.notification_store.clone(),
            self.job_queue.clone(),
            PushEvent::SubscriptionCompleted,
            title,
            message,
            PushLevel::Success,
        )
        .await;
    }
}

fn mock_probe_result(url: &str) -> Result<Option<ProbeResult>> {
    let Ok(path) = std::env::var("MOCK_QUARK_SHARE_FIXTURE") else {
        return Ok(None);
    };
    let path = path.trim();
    if path.is_empty() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| AppError::Database(format!("读取模拟分享 fixture 失败: {}", e)))?;
    let fixtures: HashMap<String, ProbeResult> = serde_json::from_str(&content)
        .map_err(|e| AppError::Database(format!("解析模拟分享 fixture 失败: {}", e)))?;

    Ok(Some(fixtures.get(url).cloned().unwrap_or_else(|| {
        ProbeResult {
            ok: false,
            state: "mock_missing".to_string(),
            message: format!("模拟分享 fixture 中不存在链接: {}", url),
            files: vec![],
        }
    })))
}

/// 检查结果
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub subscription_id: String,
    pub subscription_title: String,
    pub new_files: Vec<String>,
    pub new_episodes: Vec<i32>,
    pub details: CheckDetails,
    pub became_invalid: bool,
    pub became_completed: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CheckDetails {
    pub scanned_count: usize,
    pub new_count: usize,
    pub known_count: usize,
    pub skipped_before_start_count: usize,
    pub skipped_duplicate_episode_count: usize,
    pub items: Vec<CheckDetailItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckDetailItem {
    pub name: String,
    pub episode: Option<i32>,
    pub file_key: String,
    pub action: String,
    pub reason: String,
}

/// 从文件名提取集数
/// 支持常见格式: E01, EP01, 第01集, [01], S01E01 等
fn extract_episode_number(filename: &str) -> Option<i32> {
    crate::services::episode::detect_episode(filename).episode
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};
    use std::sync::Arc;

    fn test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "my_media_sub_{}_{}_{}.json",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn make_subscription() -> Subscription {
        Subscription {
            id: "sub1".to_string(),
            title: "Show".to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season: 1,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            metadata: None,
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
            strm_enabled: false,
            enabled: true,
            completed: false,
            rules: crate::models::rules::TransferRules::default(),
            created_at: 0,
            updated_at: 0,
            last_checked_at: 0,
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

    fn make_service() -> (
        SubscriptionCheckService,
        Arc<SubscriptionStore>,
        Arc<NotificationStore>,
    ) {
        let subscriptions = Arc::new(SubscriptionStore::new(test_path("subscriptions")));
        let settings = Arc::new(SettingsStore::new(test_path("settings")));
        let notifications = Arc::new(NotificationStore::new(test_path("notifications")));
        (
            SubscriptionCheckService::new(subscriptions.clone(), settings, notifications.clone()),
            subscriptions,
            notifications,
        )
    }

    #[test]
    fn test_extract_episode_number() {
        assert_eq!(extract_episode_number("动画名称 E01 1080p.mkv"), Some(1));
        assert_eq!(
            extract_episode_number("[字幕组] 动画名称 第12集.mp4"),
            Some(12)
        );
        assert_eq!(extract_episode_number("Show.S01E05.720p.mkv"), Some(5));
        assert_eq!(extract_episode_number("[01][1080p].mkv"), Some(1));
        assert_eq!(extract_episode_number("EP 03.mkv"), Some(3));
        assert_eq!(extract_episode_number("03.mkv"), Some(3));
        assert_eq!(extract_episode_number("129 4K.mp4"), Some(129));
        assert_eq!(extract_episode_number("23(1).mp4"), Some(23));
        assert_eq!(
            extract_episode_number("S01E144.2025.2160p.WEB-DL.HQ.H265.30fps.10bit.AAC.mp4"),
            Some(144)
        );
        assert_eq!(extract_episode_number("Movie.2024.mkv"), None);
    }

    #[test]
    fn test_find_new_files_respects_start_episode_number() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.start_episode_number = Some(5);

        let files = vec![
            ProbeFile {
                name: "Show.S01E04.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "old-ep".to_string(),
            },
            ProbeFile {
                name: "Show.S01E05.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "start-ep".to_string(),
            },
            ProbeFile {
                name: "special.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "special".to_string(),
            },
        ];

        let new_names = service
            .find_new_files(&sub, &files)
            .into_iter()
            .map(|file| file.name)
            .collect::<Vec<_>>();

        assert_eq!(new_names, vec!["Show.S01E05.mkv", "special.mkv"]);
    }

    #[test]
    fn test_find_new_files_dedups_episode_video_variants() {
        let (service, _, _) = make_service();
        let sub = make_subscription();
        let files = vec![
            ProbeFile {
                name: "178.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "ep178".to_string(),
            },
            ProbeFile {
                name: "178-4k.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "ep178-4k".to_string(),
            },
        ];

        let new_names = service
            .find_new_files(&sub, &files)
            .into_iter()
            .map(|file| file.name)
            .collect::<Vec<_>>();

        assert_eq!(new_names, vec!["178-4k.mkv"]);
    }

    #[test]
    fn test_find_new_files_skips_known_episode_video_variant() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.known_episodes = vec![178];
        let files = vec![ProbeFile {
            name: "178-4k.mkv".to_string(),
            size: 1,
            updated_at: None,
            file_key: "ep178-4k".to_string(),
        }];

        let new_files = service.find_new_files(&sub, &files);

        assert!(new_files.is_empty());
    }

    #[test]
    fn test_build_check_details_classifies_probe_files() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.known_file_keys = vec!["known-key".to_string()];
        sub.start_episode_number = Some(5);
        let files = vec![
            ProbeFile {
                name: "Show.S01E03.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "known-key".to_string(),
            },
            ProbeFile {
                name: "Show.S01E04.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "before-start".to_string(),
            },
            ProbeFile {
                name: "Show.S01E05.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "new-key".to_string(),
            },
        ];

        let details = service.build_check_details(&sub, &files);

        assert_eq!(details.scanned_count, 3);
        assert_eq!(details.known_count, 1);
        assert_eq!(details.skipped_before_start_count, 1);
        assert_eq!(details.new_count, 1);
        assert_eq!(details.items[0].action, "known");
        assert_eq!(details.items[1].action, "skip");
        assert_eq!(details.items[2].action, "new");
    }

    #[test]
    fn test_build_check_details_marks_duplicate_episode_video() {
        let (service, _, _) = make_service();
        let sub = make_subscription();
        let files = vec![
            ProbeFile {
                name: "178.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "ep178".to_string(),
            },
            ProbeFile {
                name: "178-4k.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "ep178-4k".to_string(),
            },
        ];

        let details = service.build_check_details(&sub, &files);

        assert_eq!(details.new_count, 1);
        assert_eq!(details.skipped_duplicate_episode_count, 1);
        assert_eq!(details.items[0].action, "skip");
        assert_eq!(
            details.items[0].reason,
            "同集重复视频，已保留清晰度最高版本"
        );
        assert_eq!(details.items[1].action, "new");
    }

    #[test]
    fn test_transfer_candidates_retry_known_untransferred_episode() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();
        sub.media_type = "anime".to_string();
        sub.start_episode_number = Some(144);
        sub.known_episodes = vec![144, 145, 146, 147];
        sub.transferred_files = vec![
            "145.mkv".to_string(),
            "146.mkv".to_string(),
            "S01E144.2025.2160p.WEB-DL.HQ.H265.30fps.10bit.AAC.mp4".to_string(),
        ];
        sub.transferred_file_keys = vec![];
        let files = vec![
            ProbeFile {
                name: "144-1.mp4".to_string(),
                size: 1,
                updated_at: None,
                file_key: "ep144-new-name".to_string(),
            },
            ProbeFile {
                name: "145.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "ep145".to_string(),
            },
            ProbeFile {
                name: "146.mkv".to_string(),
                size: 1,
                updated_at: None,
                file_key: "ep146".to_string(),
            },
            ProbeFile {
                name: "147.mp4".to_string(),
                size: 1,
                updated_at: None,
                file_key: "ep147".to_string(),
            },
        ];

        let candidates = service.transfer_candidate_file_names(&sub, &files, &[]);

        assert_eq!(candidates, vec!["147.mp4".to_string()]);
    }

    #[test]
    fn test_mock_probe_result_reads_fixture() {
        let path = test_path("mock_probe");
        let fixture = r#"{
            "https://pan.quark.cn/s/mock": {
                "ok": true,
                "state": "ok",
                "message": "",
                "files": [
                    {"name": "Show.S01E01.mkv", "size": 1, "file_key": "fid1"}
                ]
            }
        }"#;
        std::fs::write(&path, fixture).unwrap();
        std::env::set_var("MOCK_QUARK_SHARE_FIXTURE", &path);

        let result = mock_probe_result("https://pan.quark.cn/s/mock")
            .unwrap()
            .unwrap();
        let missing = mock_probe_result("https://pan.quark.cn/s/missing")
            .unwrap()
            .unwrap();

        std::env::remove_var("MOCK_QUARK_SHARE_FIXTURE");
        let _ = std::fs::remove_file(&path);

        assert!(result.ok);
        assert_eq!(result.files.len(), 1);
        assert!(!missing.ok);
        assert_eq!(missing.state, "mock_missing");
    }

    #[tokio::test]
    async fn test_auto_transfer_disabled_reason_respects_switches() {
        let (service, _, _) = make_service();
        let mut sub = make_subscription();

        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, false).await,
            Some("自动下载新订阅项未启用")
        );
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, true).await,
            Some("全局自动转存未启用")
        );

        service
            .settings_store
            .update(|settings| {
                settings.auto_download_new_subscription_items = true;
                settings.quark_save_enabled = false;
            })
            .await
            .unwrap();
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, false).await,
            Some("全局自动转存未启用")
        );
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, true).await,
            Some("全局自动转存未启用")
        );

        service
            .settings_store
            .update(|settings| {
                settings.quark_save_enabled = true;
            })
            .await
            .unwrap();
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, false).await,
            None
        );

        service
            .settings_store
            .update(|settings| {
                settings.auto_download_new_subscription_items = false;
            })
            .await
            .unwrap();
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, false).await,
            Some("自动下载新订阅项未启用")
        );
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, true).await,
            None
        );

        sub.notify_only = true;
        assert_eq!(
            service.auto_transfer_disabled_reason(&sub, true).await,
            Some("订阅设置为仅通知模式")
        );
    }

    #[tokio::test]
    async fn test_update_subscription_after_check_records_new_files() {
        let (service, store, _) = make_service();
        let mut sub = make_subscription();
        sub.known_file_keys = vec!["old-key".to_string()];
        sub.status = "invalid".to_string();
        sub.invalid_since = Some(1);
        store.create(sub.clone()).await.unwrap();

        let probe = ProbeResult {
            ok: true,
            state: "ok".to_string(),
            message: String::new(),
            files: vec![
                ProbeFile {
                    name: "Show.S01E01.mkv".to_string(),
                    size: 1,
                    updated_at: None,
                    file_key: "old-key".to_string(),
                },
                ProbeFile {
                    name: "Show.S01E02.mkv".to_string(),
                    size: 1,
                    updated_at: None,
                    file_key: "new-key".to_string(),
                },
            ],
        };
        let new_files = service.find_new_files(&sub, &probe.files);
        let new_names = new_files
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>();
        let new_episodes = service.parse_episodes(&new_names);

        service
            .update_subscription_after_check(
                &sub,
                &probe,
                &new_names,
                &new_episodes,
                "发现 1 个新文件",
                false,
            )
            .await
            .unwrap();

        let updated = store.get("sub1").await.unwrap();
        assert_eq!(new_names, vec!["Show.S01E02.mkv"]);
        assert_eq!(new_episodes, vec![2]);
        assert_eq!(updated.current_episode_number, 2);
        assert_eq!(updated.status, "active");
        assert!(updated.invalid_since.is_none());
        assert!(updated.known_file_keys.contains(&"new-key".to_string()));
    }

    #[tokio::test]
    async fn test_start_episode_skips_old_files_but_records_known_keys() {
        let (service, store, _) = make_service();
        let mut sub = make_subscription();
        sub.start_episode_number = Some(5);
        store.create(sub.clone()).await.unwrap();

        let probe = ProbeResult {
            ok: true,
            state: "ok".to_string(),
            message: String::new(),
            files: vec![
                ProbeFile {
                    name: "Show.S01E04.mkv".to_string(),
                    size: 1,
                    updated_at: None,
                    file_key: "ep4-key".to_string(),
                },
                ProbeFile {
                    name: "Show.S01E05.mkv".to_string(),
                    size: 1,
                    updated_at: None,
                    file_key: "ep5-key".to_string(),
                },
            ],
        };
        let new_files = service.find_new_files(&sub, &probe.files);
        let new_names = new_files
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>();
        let new_episodes = service.parse_episodes(&new_names);

        service
            .update_subscription_after_check(
                &sub,
                &probe,
                &new_names,
                &new_episodes,
                "发现 1 个新文件",
                false,
            )
            .await
            .unwrap();

        let updated = store.get("sub1").await.unwrap();
        assert_eq!(new_names, vec!["Show.S01E05.mkv"]);
        assert_eq!(new_episodes, vec![5]);
        assert!(updated.known_file_keys.contains(&"ep4-key".to_string()));
        assert!(updated.known_file_keys.contains(&"ep5-key".to_string()));
        assert_eq!(updated.last_new_files, vec!["Show.S01E05.mkv"]);
    }

    #[tokio::test]
    async fn test_mark_subscription_invalid_sets_status() {
        let (service, store, _) = make_service();
        let mut sub = make_subscription();
        sub.rules.notify_on_invalid = false;
        store.create(sub.clone()).await.unwrap();

        service
            .mark_subscription_invalid(&sub, "invalid share")
            .await
            .unwrap();

        let updated = store.get("sub1").await.unwrap();
        assert_eq!(updated.status, "invalid");
        assert_eq!(updated.last_error, "invalid share");
        assert!(updated.invalid_since.is_some());
    }

    #[test]
    fn test_should_mark_completed() {
        let mut sub = Subscription {
            id: "sub1".to_string(),
            title: "Show".to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season: 1,
            start_episode_number: None,
            current_episode_number: 11,
            total_episode_number: None,
            source_group: String::new(),
            metadata: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            strm_enabled: false,
            enabled: true,
            completed: false,
            rules: crate::models::rules::TransferRules {
                finish_after_episode: Some(12),
                ..Default::default()
            },
            created_at: 0,
            updated_at: 0,
            last_checked_at: 0,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
            known_episodes: vec![1, 2, 11],
        };

        assert!(should_mark_completed_from_known_episodes(&sub, &[12]));
        assert!(!should_mark_completed_from_known_episodes(&sub, &[10]));

        sub.completed = true;
        assert!(!should_mark_completed_from_known_episodes(&sub, &[12]));
    }
}
