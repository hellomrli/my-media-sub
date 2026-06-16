use std::sync::Arc;
use tracing::{info, warn};

use crate::clients::quark::QuarkShareProbe;
use crate::error::{AppError, Result};
use crate::models::subscription::{CheckHistoryItem, ProbeFile, ProbeResult, Subscription};
use crate::services::SubscriptionTransferService;
use crate::store::{NotificationStore, SubscriptionStore};

/// 订阅检查服务
pub struct SubscriptionCheckService {
    subscription_store: Arc<SubscriptionStore>,
    notification_store: Arc<NotificationStore>,
    transfer_service: Option<Arc<SubscriptionTransferService>>,
}

impl SubscriptionCheckService {
    pub fn new(
        subscription_store: Arc<SubscriptionStore>,
        notification_store: Arc<NotificationStore>,
    ) -> Self {
        Self {
            subscription_store,
            notification_store,
            transfer_service: None,
        }
    }

    /// 设置转存服务（可选，用于自动转存）
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
                summary: format!("链接失效: {}", probe_result.message),
            });
        }

        // 2. 对比文件，找出新增文件
        let new_files = self.find_new_files(&sub, &probe_result.files);
        let new_file_names: Vec<String> = new_files.iter().map(|f| f.name.clone()).collect();

        // 3. 解析集数
        let new_episodes = self.parse_episodes(&new_file_names);

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
        )
        .await?;

        // 5. 发送通知
        if !new_file_names.is_empty() {
            self.send_update_notification(&sub, &new_file_names, &new_episodes)
                .await;
        }

        // 6. 自动转存（如果配置了转存服务）
        if !new_file_names.is_empty() {
            if let Some(transfer_service) = &self.transfer_service {
                match transfer_service
                    .auto_transfer_new_files(&sub.id, &new_file_names)
                    .await
                {
                    Ok(result) => {
                        if !result.skipped {
                            info!("自动转存成功: {}", result.reason);
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
            .filter(|f| !sub.known_file_keys.contains(&f.file_key))
            .cloned()
            .collect()
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
                    s.status = "active".to_string();
                    s.last_error = String::new();
                    s.invalid_since = None;
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
        let notification = crate::models::Notification {
            id: uuid::Uuid::new_v4().to_string(),
            level: "warning".to_string(),
            event: "subscription_invalid".to_string(),
            title: format!("订阅链接疑似失效: {}", sub.title),
            message: error.to_string(),
            meta: std::collections::HashMap::new(),
            read: false,
            created_at: now,
        };

        self.notification_store.add(notification).await?;

        Ok(())
    }

    /// 发送更新通知
    async fn send_update_notification(
        &self,
        sub: &Subscription,
        new_files: &[String],
        new_episodes: &[i32],
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

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

        let notification = crate::models::Notification {
            id: uuid::Uuid::new_v4().to_string(),
            level: "info".to_string(),
            event: "subscription_updated".to_string(),
            title: format!("订阅有更新: {}", sub.title),
            message,
            meta: std::collections::HashMap::new(),
            read: false,
            created_at: now,
        };

        let _ = self.notification_store.add(notification).await;
    }
}

/// 检查结果
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub subscription_id: String,
    pub new_files: Vec<String>,
    pub new_episodes: Vec<i32>,
    pub became_invalid: bool,
    pub summary: String,
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
        assert_eq!(extract_episode_number("Movie.2024.mkv"), None);
    }
}
