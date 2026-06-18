use std::sync::Arc;
use tracing::{info, warn};

use crate::clients::quark::QuarkShareProbe;
use crate::error::{AppError, Result};
use crate::jobs::{JobQueue, SubscriptionTransferPayload};
use crate::models::subscription::{CheckHistoryItem, ProbeFile, ProbeResult, Subscription};
use crate::services::notification::{add_notification, dispatch_push_event};
use crate::services::push::{PushEvent, PushLevel};
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
        let sub = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

        if !sub.enabled {
            return Err(AppError::Validation("订阅未启用".to_string()));
        }

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
                new_files: vec![],
                new_episodes: vec![],
                became_invalid: true,
                became_completed: false,
                summary: format!("链接失效: {}", probe_result.message),
            });
        }

        // 2. 对比文件，找出新增文件
        let new_files = self.find_new_files(&sub, &probe_result.files);
        let new_file_names: Vec<String> = new_files.iter().map(|f| f.name.clone()).collect();

        // 3. 解析集数
        let new_episodes = self.parse_episodes(&new_file_names);
        let became_completed = should_mark_completed(&sub, &new_episodes);

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
        if !new_file_names.is_empty() {
            if let Some(job_queue) = &self.job_queue {
                match job_queue
                    .submit_subscription_transfer(SubscriptionTransferPayload {
                        subscription_id: sub.id.clone(),
                        file_names: new_file_names.clone(),
                    })
                    .await
                {
                    Ok(job) => info!("已创建订阅自动转存任务: {}", job.id),
                    Err(e) => warn!("创建订阅自动转存任务失败: {}", e),
                }
            } else if let Some(transfer_service) = &self.transfer_service {
                match transfer_service
                    .auto_transfer_new_files(&sub.id, &new_file_names)
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
            new_files: new_file_names,
            new_episodes,
            became_invalid: false,
            became_completed,
            summary,
        })
    }

    /// 检查所有启用的订阅
    pub async fn check_all_subscriptions(&self, cookie: &str) -> Result<Vec<CheckResult>> {
        let subscriptions = self.subscription_store.list().await;
        let mut results = Vec::new();

        for sub in subscriptions {
            if !sub.enabled || sub.completed {
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
        let probe = QuarkShareProbe::new(cookie.to_string());
        let share_info = probe.probe(&sub.url, &sub.password, 200).await;

        let files: Vec<ProbeFile> = share_info
            .files
            .iter()
            .map(|f| ProbeFile {
                name: f.name.clone(),
                size: f.size,
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
        files
            .iter()
            .filter(|f| {
                !sub.known_file_keys.contains(&f.file_key)
                    && !self.is_before_start_episode(sub, &f.name)
            })
            .cloned()
            .collect()
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
        let total = sub
            .rules
            .finish_after_episode
            .or(sub.total_episode_number)
            .unwrap_or(sub.current_episode_number);
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

/// 检查结果
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub subscription_id: String,
    pub new_files: Vec<String>,
    pub new_episodes: Vec<i32>,
    pub became_invalid: bool,
    pub became_completed: bool,
    pub summary: String,
}

fn should_mark_completed(sub: &Subscription, new_episodes: &[i32]) -> bool {
    if sub.completed {
        return false;
    }

    let Some(target_episode) = sub.rules.finish_after_episode else {
        return false;
    };

    sub.known_episodes
        .iter()
        .chain(new_episodes.iter())
        .copied()
        .max()
        .map(|episode| episode >= target_episode)
        .unwrap_or(false)
}

/// 从文件名提取集数
/// 支持常见格式: E01, EP01, 第01集, [01], S01E01 等
fn extract_episode_number(filename: &str) -> Option<i32> {
    use regex::Regex;

    // 常见集数匹配模式
    let patterns = [
        r"[Ee]([0-9]{1,3})",               // E01, e01
        r"[Ee][Pp]\.?\s*([0-9]{1,3})",     // EP01, ep 01
        r"第\s*([0-9]{1,3})\s*[集话話]",   // 第01集
        r"\[([0-9]{1,3})\]",               // [01]
        r"[Ss][0-9]{1,2}[Ee]([0-9]{1,3})", // S01E01
        r"(?i)(?:^|[^\d])([0-9]{1,3})\.(mkv|mp4|avi|ts|mov|wmv|flv|m4v|rmvb|webm)$", // 03.mkv
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(filename) {
                if let Some(num_str) = caps.get(1) {
                    if let Ok(num) = num_str.as_str().parse::<i32>() {
                        return Some(num);
                    }
                }
            }
        }
    }

    None
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
                file_key: "old-ep".to_string(),
            },
            ProbeFile {
                name: "Show.S01E05.mkv".to_string(),
                size: 1,
                file_key: "start-ep".to_string(),
            },
            ProbeFile {
                name: "special.mkv".to_string(),
                size: 1,
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
                    file_key: "old-key".to_string(),
                },
                ProbeFile {
                    name: "Show.S01E02.mkv".to_string(),
                    size: 1,
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
                    file_key: "ep4-key".to_string(),
                },
                ProbeFile {
                    name: "Show.S01E05.mkv".to_string(),
                    size: 1,
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

        assert!(should_mark_completed(&sub, &[12]));
        assert!(!should_mark_completed(&sub, &[10]));

        sub.completed = true;
        assert!(!should_mark_completed(&sub, &[12]));
    }
}
