use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{info, warn};

use crate::clients::quark::QuarkShareProbe;
use crate::error::{AppError, Result};
use crate::jobs::{JobQueue, SubscriptionTransferPayload};
use crate::models::subscription::{CheckHistoryItem, ProbeFile, ProbeResult, Subscription};
use crate::services::episode::{
    episode_video_key, is_better_episode_duplicate_candidate, matches_subscription_season,
    normalize_duplicate_episode_strategy, EpisodeDuplicateCandidate,
};
use crate::services::notification::{
    add_notification, dispatch_push_event_for_notification, PushDispatchRequest,
};
use crate::services::push::{PushEvent, PushLevel};
use crate::services::subscription_progress::{
    completion_target_episode, reopen_completed_subscription_status,
    should_mark_completed_from_known_episodes, should_mark_completed_from_transferred_files,
    should_reopen_completed_subscription,
};
use crate::services::transfer_rule::transfer_state_key;
use crate::services::SubscriptionTransferService;
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};
use crate::utils::{metrics::global_metrics, unix_now};

include!("subscription_check/file_filter_methods.rs");

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
        let metrics = global_metrics();
        metrics.increment_subscription_checks();
        let result = self
            .do_check_subscription_with_options(subscription_id, cookie, force_transfer)
            .await;
        if result.is_err() {
            metrics.increment_subscription_check_failures();
        }
        result
    }

    async fn do_check_subscription_with_options(
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
                                dispatch_push_event_for_notification(
                                    self.settings_store.clone(),
                                    self.notification_store.clone(),
                                    None,
                                    PushDispatchRequest {
                                        notification_id: result.push_notification_id,
                                        event: PushEvent::TransferSaved,
                                        title,
                                        message,
                                        level: PushLevel::Success,
                                    },
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
        let now = unix_now();
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
                is_dir: f.is_dir,
                parent_path: f.parent_path.clone(),
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

    subscription_check_file_filter_methods!();

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
        let now = unix_now();
        let details = self.build_check_details(sub, &probe.files);

        self.subscription_store
            .update(&sub.id, |s| {
                // 更新已知文件列表
                for file in &probe.files {
                    if !Self::should_record_known_probe_file(s, file) {
                        continue;
                    }
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
                        scanned_count: details.scanned_count as i32,
                        new_count: details.new_count as i32,
                        known_count: details.known_count as i32,
                        skipped_directory_count: details.skipped_directory_count as i32,
                        skipped_other_season_count: details.skipped_other_season_count as i32,
                        skipped_before_start_count: details.skipped_before_start_count as i32,
                        skipped_duplicate_episode_count: details.skipped_duplicate_episode_count
                            as i32,
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
        let now = unix_now();

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
            let notification = add_notification(
                &self.notification_store,
                "warning",
                "subscription_invalid",
                title.clone(),
                message.clone(),
                std::collections::HashMap::new(),
            )
            .await?;
            dispatch_push_event_for_notification(
                self.settings_store.clone(),
                self.notification_store.clone(),
                self.job_queue.clone(),
                PushDispatchRequest {
                    notification_id: Some(notification.id),
                    event: PushEvent::SubscriptionFailed,
                    title,
                    message,
                    level: PushLevel::Warning,
                },
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
        let notification = add_notification(
            &self.notification_store,
            "info",
            "subscription_updated",
            title.clone(),
            message.clone(),
            std::collections::HashMap::new(),
        )
        .await;
        dispatch_push_event_for_notification(
            self.settings_store.clone(),
            self.notification_store.clone(),
            self.job_queue.clone(),
            PushDispatchRequest {
                notification_id: notification.ok().map(|notification| notification.id),
                event: PushEvent::SubscriptionUpdated,
                title,
                message,
                level: PushLevel::Info,
            },
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

        let notification = add_notification(
            &self.notification_store,
            "success",
            "subscription_completed",
            title.clone(),
            message.clone(),
            std::collections::HashMap::new(),
        )
        .await;
        dispatch_push_event_for_notification(
            self.settings_store.clone(),
            self.notification_store.clone(),
            self.job_queue.clone(),
            PushDispatchRequest {
                notification_id: notification.ok().map(|notification| notification.id),
                event: PushEvent::SubscriptionCompleted,
                title,
                message,
                level: PushLevel::Success,
            },
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
    pub skipped_directory_count: usize,
    pub skipped_other_season_count: usize,
    pub skipped_before_start_count: usize,
    pub skipped_duplicate_episode_count: usize,
    pub items: Vec<CheckDetailItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckDetailItem {
    pub name: String,
    pub episode: Option<i32>,
    pub is_dir: bool,
    pub parent_path: String,
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
include!("subscription_check/tests.rs");
