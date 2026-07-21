use chrono::Datelike;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use tracing::{info, warn};

use crate::error::{AppError, Result};
use crate::jobs::{JobQueue, SubscriptionTransferPayload};
use crate::models::subscription::{CheckHistoryItem, ProbeFile, ProbeResult, Subscription};
use crate::providers::CloudDriveProviderRegistry;
use crate::services::episode::{
    episode_video_key, is_better_episode_duplicate_candidate, is_video_name,
    matches_subscription_season, normalize_duplicate_episode_strategy, EpisodeDuplicateCandidate,
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
use crate::store::{AutomationEventStore, NotificationStore, SettingsStore, SubscriptionStore};
use crate::utils::{metrics::global_metrics, unix_now};

include!("subscription_check/file_filter_methods.rs");

/// 订阅检查服务
#[derive(Clone)]
pub struct SubscriptionCheckService {
    subscription_store: Arc<SubscriptionStore>,
    settings_store: Arc<SettingsStore>,
    notification_store: Arc<NotificationStore>,
    automation_event_store: Option<Arc<AutomationEventStore>>,
    job_queue: Option<Arc<JobQueue>>,
    transfer_service: Option<Arc<SubscriptionTransferService>>,
    subscription_locks: Arc<tokio::sync::Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
    share_locks: Arc<tokio::sync::Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>>,
    batch_probe_cache: Option<Arc<tokio::sync::Mutex<HashMap<String, ProbeResult>>>>,
    provider_registry: Arc<CloudDriveProviderRegistry>,
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
            automation_event_store: None,
            job_queue: None,
            transfer_service: None,
            subscription_locks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            share_locks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            batch_probe_cache: None,
            provider_registry: Arc::new(CloudDriveProviderRegistry::new()),
        }
    }

    fn with_subscription_store(&self, subscription_store: Arc<SubscriptionStore>) -> Self {
        Self {
            subscription_store,
            settings_store: self.settings_store.clone(),
            notification_store: self.notification_store.clone(),
            automation_event_store: self.automation_event_store.clone(),
            job_queue: self.job_queue.clone(),
            transfer_service: self.transfer_service.clone(),
            subscription_locks: self.subscription_locks.clone(),
            share_locks: self.share_locks.clone(),
            batch_probe_cache: self.batch_probe_cache.clone(),
            provider_registry: self.provider_registry.clone(),
        }
    }

    fn with_batch_probe_cache(mut self) -> Self {
        self.batch_probe_cache = Some(Arc::new(tokio::sync::Mutex::new(HashMap::new())));
        self
    }

    pub fn with_event_store(mut self, store: Arc<AutomationEventStore>) -> Self {
        self.automation_event_store = Some(store);
        self
    }

    /// Override provider resolution (primarily for deterministic service tests).
    pub fn with_provider_registry(mut self, registry: Arc<CloudDriveProviderRegistry>) -> Self {
        self.provider_registry = registry;
        self
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

    async fn named_lock(
        locks: &tokio::sync::Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
        key: &str,
    ) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = locks.lock().await;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(key).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(key.to_string(), Arc::downgrade(&lock));
        lock
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
        let _timer = metrics.start_timer(crate::utils::metrics::MetricTimerKind::SubscriptionCheck);
        let subscription_lock = Self::named_lock(&self.subscription_locks, subscription_id).await;
        let _subscription_guard = subscription_lock.lock().await;
        metrics.increment_subscription_checks();
        let ambient = crate::observability::current_context();
        let correlation_id = ambient
            .correlation_id
            .clone()
            .unwrap_or_else(|| format!("check:{}:{}", subscription_id, unix_now()));
        let context = crate::observability::LogContext {
            request_id: ambient.request_id,
            correlation_id: Some(correlation_id.clone()),
            subscription_id: Some(subscription_id.to_string()),
            job_id: ambient.job_id,
        };
        let span = crate::observability::subscription_span(&context);
        let result = crate::observability::in_context(
            context,
            span,
            self.do_check_subscription_with_options(
                subscription_id,
                cookie,
                force_transfer,
                &correlation_id,
            ),
        )
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
        correlation_id: &str,
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

        crate::services::automation_events::record_stage_event(
            self.automation_event_store.as_ref(),
            correlation_id,
            Some(&sub.id),
            None,
            None,
            crate::models::AutomationStage::SourceCheck,
            crate::models::AutomationStatus::Running,
            "正在探测订阅来源",
            "",
            std::collections::BTreeMap::new(),
        )
        .await;

        // 1. 探测分享链接
        info!("检查订阅: {} ({})", sub.title, sub.id);
        let probe_result = match self.probe_share(&sub, cookie).await {
            Ok(result) => result,
            Err(error) => {
                crate::services::automation_events::record_stage_event(
                    self.automation_event_store.as_ref(),
                    correlation_id,
                    Some(&sub.id),
                    None,
                    None,
                    crate::models::AutomationStage::SourceCheck,
                    crate::models::AutomationStatus::Failed,
                    "订阅来源探测异常",
                    error.to_string(),
                    std::collections::BTreeMap::new(),
                )
                .await;
                return Err(error);
            }
        };

        if !probe_result.ok {
            crate::services::automation_events::record_stage_event(
                self.automation_event_store.as_ref(),
                correlation_id,
                Some(&sub.id),
                None,
                None,
                crate::models::AutomationStage::SourceCheck,
                crate::models::AutomationStatus::Failed,
                "订阅来源探测失败",
                &probe_result.message,
                std::collections::BTreeMap::new(),
            )
            .await;
            // 探测失败，标记为失效
            self.mark_subscription_invalid(&sub, &probe_result.message)
                .await?;

            // 【新增】自动搜索换源候选
            let candidates_count = if self.should_search_source_candidates(&sub).await {
                match self.search_and_save_candidates(&sub, cookie).await {
                    Ok(candidates) if !candidates.is_empty() => {
                        info!("为订阅 {} 找到 {} 个换源候选", sub.title, candidates.len());

                        if let Err(e) = self.notify_source_candidates_found(&sub, &candidates).await
                        {
                            warn!("发送换源通知失败: {}", e);
                        }

                        match self
                            .try_auto_apply_source_candidate(&sub.id, &candidates, cookie)
                            .await
                        {
                            Ok(Some(candidate_id)) => {
                                info!(
                                    "订阅 {} 已自动应用候选 {}，立即重新检查",
                                    sub.title, candidate_id
                                );
                                return Box::pin(self.do_check_subscription_with_options(
                                    subscription_id,
                                    cookie,
                                    force_transfer,
                                    correlation_id,
                                ))
                                .await;
                            }
                            Ok(None) => {}
                            Err(error) => warn!("自动换源失败: {}", error),
                        }

                        candidates.len()
                    }
                    Ok(_) => {
                        info!("未找到换源候选");
                        0
                    }
                    Err(e) => {
                        warn!("搜索换源候选失败: {}", e);
                        0
                    }
                }
            } else {
                0
            };

            if let Some(latest) = self.subscription_store.get(&sub.id).await {
                if !latest.source_candidates.is_empty() {
                    match self
                        .try_auto_apply_source_candidate(&sub.id, &latest.source_candidates, cookie)
                        .await
                    {
                        Ok(Some(candidate_id)) => {
                            info!(
                                "订阅 {} 已从冷却期候选中自动应用 {}，立即重新检查",
                                sub.title, candidate_id
                            );
                            return Box::pin(self.do_check_subscription_with_options(
                                subscription_id,
                                cookie,
                                force_transfer,
                                correlation_id,
                            ))
                            .await;
                        }
                        Ok(None) => {}
                        Err(error) => warn!("应用已有换源候选失败: {}", error),
                    }
                }
            }

            return Ok(CheckResult {
                subscription_id: sub.id.clone(),
                subscription_title: sub.title.clone(),
                new_files: vec![],
                new_episodes: vec![],
                details: CheckDetails::default(),
                became_invalid: true,
                became_completed: false,
                summary: if candidates_count > 0 {
                    format!(
                        "链接失效: {}，已找到 {} 个替代源",
                        probe_result.message, candidates_count
                    )
                } else {
                    format!("链接失效: {}", probe_result.message)
                },
            });
        }

        crate::services::automation_events::record_stage_event(
            self.automation_event_store.as_ref(),
            correlation_id,
            Some(&sub.id),
            None,
            None,
            crate::models::AutomationStage::SourceCheck,
            crate::models::AutomationStatus::Succeeded,
            format!("来源探测成功，共 {} 个项目", probe_result.files.len()),
            "",
            std::collections::BTreeMap::new(),
        )
        .await;

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
        crate::services::automation_events::record_stage_event(
            self.automation_event_store.as_ref(),
            correlation_id,
            Some(&sub.id),
            None,
            None,
            crate::models::AutomationStage::FileFilter,
            crate::models::AutomationStatus::Succeeded,
            format!(
                "扫描 {} 项，识别 {} 个新增文件",
                details.scanned_count, details.new_count
            ),
            "",
            std::collections::BTreeMap::from([
                (
                    "scanned_count".to_string(),
                    serde_json::json!(details.scanned_count),
                ),
                (
                    "new_count".to_string(),
                    serde_json::json!(details.new_count),
                ),
                (
                    "known_count".to_string(),
                    serde_json::json!(details.known_count),
                ),
            ]),
        )
        .await;

        crate::services::automation_events::record_stage_event(
            self.automation_event_store.as_ref(),
            correlation_id,
            Some(&sub.id),
            None,
            None,
            crate::models::AutomationStage::VersionSelect,
            if transfer_file_names.is_empty() {
                crate::models::AutomationStatus::Skipped
            } else {
                crate::models::AutomationStatus::Succeeded
            },
            if transfer_file_names.is_empty() {
                "没有需要选择的新增版本".to_string()
            } else {
                format!("已选择 {} 个待转存版本", transfer_file_names.len())
            },
            "",
            std::collections::BTreeMap::from([(
                "selected_count".to_string(),
                serde_json::json!(transfer_file_names.len()),
            )]),
        )
        .await;
        let became_completed = if sub.notify_only {
            should_mark_completed_from_known_episodes(&sub, &new_episodes)
        } else if sub.sync_download_enabled {
            false
        } else if transfer_file_names.is_empty() {
            // When the provider reports no new files and the known snapshot
            // already contains the configured final episode, completion can
            // be reconciled even if legacy transferred filenames lack a
            // parseable SxxExx marker.
            should_mark_completed_from_known_episodes(&sub, &new_episodes)
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
                        correlation_id: correlation_id.to_string(),
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
                                        subscription_id: Some(sub.id.clone()),
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

    /// 检查所有启用的订阅（手动/API：忽略单订阅间隔）
    pub async fn check_all_subscriptions(&self, cookie: &str) -> Result<Vec<CheckResult>> {
        self.check_subscriptions_internal(cookie, false).await
    }

    /// 仅检查已到期的订阅（定时调度：尊重 `rules.check_interval_minutes` / `check_weekdays`）
    pub async fn check_due_subscriptions(&self, cookie: &str) -> Result<Vec<CheckResult>> {
        self.check_subscriptions_internal(cookie, true).await
    }

    async fn check_subscriptions_internal(
        &self,
        cookie: &str,
        due_only: bool,
    ) -> Result<Vec<CheckResult>> {
        let subscriptions = self.subscription_store.list().await;
        let settings = self.settings_store.get().await;
        let global_interval = crate::models::settings::normalize_check_interval_minutes(i64::from(
            settings.subscription_check_interval_minutes,
        ));
        let now = unix_now();
        let weekday = chrono::Local::now().weekday().number_from_monday() as i32;
        let eligible_ids = subscriptions
            .iter()
            .filter(|sub| sub.enabled)
            .filter(|sub| !sub.completed || should_reopen_completed_subscription(sub))
            .filter(|sub| {
                !due_only || subscription_due_for_check(sub, global_interval, now, weekday)
            })
            .map(|sub| sub.id.clone())
            .collect::<Vec<_>>();
        if eligible_ids.is_empty() {
            return Ok(Vec::new());
        }

        // 批量检查在内存快照中执行；所有订阅完成后再一次性写回真实 Store。
        // 保留批量开始时的原始记录，用于写回时做三方对比，避免覆盖并发修改。
        let originals: HashMap<String, Subscription> = subscriptions
            .iter()
            .filter(|sub| eligible_ids.contains(&sub.id))
            .map(|sub| (sub.id.clone(), sub.clone()))
            .collect();
        let batch_store = Arc::new(SubscriptionStore::from_snapshot(subscriptions));
        let batch_service = self
            .with_subscription_store(batch_store.clone())
            .with_batch_probe_cache();
        let concurrency = settings
            .subscription_check_max_concurrency
            .min(settings.external_api_max_concurrency)
            .max(1);
        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let mut tasks = tokio::task::JoinSet::new();
        for (index, subscription_id) in eligible_ids.iter().cloned().enumerate() {
            let service = batch_service.clone();
            let cookie = cookie.to_string();
            let semaphore = semaphore.clone();
            tasks.spawn(async move {
                let _permit = semaphore.acquire_owned().await.expect("semaphore closed");
                let result = service.check_subscription(&subscription_id, &cookie).await;
                (index, subscription_id, result)
            });
        }

        let mut indexed_results = Vec::new();
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok((index, _, Ok(result))) => indexed_results.push((index, result)),
                Ok((_, subscription_id, Err(error))) => {
                    warn!("检查订阅 {} 失败: {}", subscription_id, error);
                }
                Err(error) => warn!("订阅检查任务异常结束: {}", error),
            }
        }
        indexed_results.sort_by_key(|(index, _)| *index);
        let results = indexed_results
            .into_iter()
            .map(|(_, result)| result)
            .collect::<Vec<_>>();

        // 写回时仅合并检查流程产生的字段，绝不整条替换：
        // - transferred_files/transferred_file_keys 等由转存任务并发维护的字段保持当前值；
        // - 标题、规则、启用状态等用户可编辑字段保持当前值；
        // - 批量期间被删除的订阅跳过写回，不影响其余订阅的持久化。
        let updates: Vec<Subscription> = batch_store
            .list()
            .await
            .into_iter()
            .filter(|sub| {
                originals.get(&sub.id).is_some_and(|original| {
                    sub.last_checked_at != original.last_checked_at
                        || sub.updated_at != original.updated_at
                })
            })
            .collect();
        self.subscription_store
            .merge_many(updates, |current, checked| {
                merge_check_results(current, originals.get(&checked.id), checked);
            })
            .await?;
        Ok(results)
    }

    /// 探测分享链接
    async fn probe_share(&self, sub: &Subscription, cookie: &str) -> Result<ProbeResult> {
        let share_lock = Self::named_lock(&self.share_locks, &sub.url).await;
        let _share_guard = share_lock.lock().await;
        let cache_key = format!("{}\0{}", sub.url.trim(), sub.password);
        if let Some(cache) = self.batch_probe_cache.as_ref() {
            if let Some(cached) = cache.lock().await.get(&cache_key).cloned() {
                return Ok(cached);
            }
        }

        let result = self.probe_share_uncached(sub, cookie).await?;
        if let Some(cache) = self.batch_probe_cache.as_ref() {
            cache.lock().await.insert(cache_key, result.clone());
        }
        Ok(result)
    }

    async fn probe_share_uncached(&self, sub: &Subscription, cookie: &str) -> Result<ProbeResult> {
        let uses_quark =
            sub.cloud_type.trim().is_empty() || sub.cloud_type.eq_ignore_ascii_case("quark");
        if uses_quark {
            if let Some(mock_result) = mock_probe_result(&sub.url)? {
                return Ok(mock_result);
            }
        }

        let provider = self
            .provider_registry
            .resolve_with_quark_cookie(&sub.cloud_type, cookie.to_string())?;
        let provider_probe = provider.probe(&sub.url, &sub.password, 200).await?;
        if provider_probe.state == "rate_limited" {
            return Err(AppError::RateLimited(provider_probe.message));
        }

        let files: Vec<ProbeFile> = provider_probe
            .files
            .into_iter()
            .map(|file| ProbeFile {
                name: file.name,
                is_dir: file.is_dir,
                parent_path: file.parent_path,
                size: file.size,
                updated_at: file.updated_at,
                file_key: file.id,
            })
            .collect();

        Ok(ProbeResult {
            ok: provider_probe.ok,
            state: provider_probe.state,
            message: provider_probe.message,
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
                    if !self.should_record_known_probe_file(s, file) {
                        continue;
                    }
                    if !s.known_file_keys.contains(&file.file_key) {
                        s.known_files.push(file.name.clone());
                        s.known_file_keys.push(file.file_key.clone());
                    }
                }

                // 已知总集数存在时，清理历史误识别出的超范围集数。
                if let Some(target) = completion_target_episode(s) {
                    s.known_episodes.retain(|episode| *episode <= target);
                }

                // 更新已知集数
                for ep in new_episodes {
                    if !s.known_episodes.contains(ep) {
                        s.known_episodes.push(*ep);
                    }
                }
                s.known_episodes.sort();

                // 更新当前集数；历史异常值被清理后也必须允许进度回落。
                s.current_episode_number = s.known_episodes.iter().max().copied().unwrap_or(0);

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
                    s.source_failure_count = 0;
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
                s.source_failure_count = s.source_failure_count.saturating_add(1);
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
                    subscription_id: Some(sub.id.clone()),
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
                subscription_id: Some(sub.id.clone()),
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
                subscription_id: Some(sub.id.clone()),
                event: PushEvent::SubscriptionCompleted,
                title,
                message,
                level: PushLevel::Success,
            },
        )
        .await;
    }

    /// 判断是否应该搜索换源候选
    async fn should_search_source_candidates(&self, sub: &Subscription) -> bool {
        let settings = self.settings_store.get().await;
        let cooldown = i64::from(settings.source_switch_cooldown_hours.max(1)) * 3600;
        if let Some(last_search) = sub.last_source_search_time {
            let now = unix_now();
            if now.saturating_sub(last_search) < cooldown {
                info!("订阅 {} 仍在换源搜索冷却期，跳过", sub.title);
                return false;
            }
        }
        true
    }

    /// 搜索并保存候选项
    async fn search_and_save_candidates(
        &self,
        sub: &Subscription,
        cookie: &str,
    ) -> Result<Vec<crate::models::subscription::SourceCandidate>> {
        use crate::services::subscription_source_switch::SubscriptionSourceSwitchService;

        let settings = self.settings_store.get().await;
        let pansou_api_url = pansou_api_url_option(&settings.pansou_api_url);
        let quark_probe = Arc::new(crate::clients::quark::QuarkShareProbe::new(cookie));
        let source_switch_service =
            SubscriptionSourceSwitchService::with_pansou_api_url(quark_probe, pansou_api_url);
        let candidates = source_switch_service.search_source_candidates(sub).await?;

        // 更新订阅
        let updated_candidates = candidates.clone();
        let now = unix_now();
        self.subscription_store
            .update(&sub.id, |s| {
                s.source_candidates = updated_candidates;
                s.last_source_search_time = Some(now);
            })
            .await?;

        Ok(candidates)
    }

    async fn try_auto_apply_source_candidate(
        &self,
        subscription_id: &str,
        candidates: &[crate::models::subscription::SourceCandidate],
        cookie: &str,
    ) -> Result<Option<String>> {
        use crate::services::subscription_source_switch::SubscriptionSourceSwitchService;

        let settings = self.settings_store.get().await;
        if !settings.auto_source_switch_enabled || settings.auto_source_switch_mode != "apply" {
            return Ok(None);
        }
        let mut subscription = self
            .subscription_store
            .get(subscription_id)
            .await
            .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
        if subscription.source_failure_count
            < settings.source_switch_failure_threshold.max(1) as u32
        {
            return Ok(None);
        }

        let service = SubscriptionSourceSwitchService::with_pansou_api_url(
            Arc::new(crate::clients::quark::QuarkShareProbe::new(cookie)),
            pansou_api_url_option(&settings.pansou_api_url),
        );
        let now = unix_now();
        let mut scored_candidates = Vec::new();

        for candidate in candidates.iter().take(5) {
            let scored = service
                .probe_and_score_candidate(candidate, cookie, now * 1000)
                .await?;
            let preview = service.preview_candidate(&subscription, scored.clone(), &settings, now);
            if !preview.probe_ok {
                service.record_candidate_failure(
                    &mut subscription,
                    &scored,
                    scored
                        .probe_info
                        .as_ref()
                        .map(|probe| probe.message.as_str())
                        .unwrap_or("候选探测失败"),
                    true,
                );
            }
            scored_candidates.push(scored);
        }
        subscription.source_candidates = scored_candidates;

        let Some(preview) = service.best_auto_candidate(
            &subscription,
            &subscription.source_candidates,
            &settings,
            now,
        ) else {
            let snapshot = subscription.clone();
            self.subscription_store
                .update(subscription_id, |current| *current = snapshot)
                .await?;
            return Ok(None);
        };
        let candidate = preview.candidate.clone();

        service.apply_source_switch_with_audit(
            &mut subscription,
            &candidate.id,
            true,
            &format!(
                "连续失效达到阈值，候选 {} 分，分差 {}",
                candidate.quality.score, preview.score_delta
            ),
        )?;
        subscription.updated_at = now;
        let snapshot = subscription.clone();
        self.subscription_store
            .update(subscription_id, |current| *current = snapshot)
            .await?;

        let mut meta = HashMap::new();
        meta.insert(
            "subscription_id".to_string(),
            serde_json::Value::String(subscription.id.clone()),
        );
        meta.insert(
            "candidate_id".to_string(),
            serde_json::Value::String(candidate.id.clone()),
        );
        meta.insert(
            "quality_score".to_string(),
            serde_json::Value::from(candidate.quality.score),
        );
        meta.insert("automatic".to_string(), serde_json::Value::Bool(true));
        let _ = add_notification(
            &self.notification_store,
            "success",
            "subscription_source_switched",
            "自动换源成功".to_string(),
            format!(
                "订阅「{}」已自动切换到质量分 {} 的候选来源",
                subscription.title, candidate.quality.score
            ),
            meta,
        )
        .await;
        Ok(Some(candidate.id))
    }

    /// 发送换源候选通知
    async fn notify_source_candidates_found(
        &self,
        sub: &Subscription,
        candidates: &[crate::models::subscription::SourceCandidate],
    ) -> Result<()> {
        let title = format!("订阅链接失效: {}", sub.title);
        let message = format!(
            "订阅「{}」链接失效，已自动找到 {} 个替代源，请在 WebUI 中选择换源。",
            sub.title,
            candidates.len()
        );

        let mut meta = std::collections::HashMap::new();
        meta.insert(
            "subscription_id".to_string(),
            serde_json::Value::String(sub.id.clone()),
        );
        meta.insert(
            "candidates_count".to_string(),
            serde_json::Value::String(candidates.len().to_string()),
        );

        let notification = add_notification(
            &self.notification_store,
            "warning",
            "subscription_invalid_with_candidates",
            title.clone(),
            message.clone(),
            meta,
        )
        .await;

        dispatch_push_event_for_notification(
            self.settings_store.clone(),
            self.notification_store.clone(),
            self.job_queue.clone(),
            PushDispatchRequest {
                notification_id: notification.ok().map(|n| n.id),
                subscription_id: Some(sub.id.clone()),
                event: PushEvent::SubscriptionFailed,
                title,
                message,
                level: PushLevel::Warning,
            },
        )
        .await;

        Ok(())
    }
}

/// 是否到了该订阅的检查时间。
///
/// - `rules.check_interval_minutes > 0` 时覆盖全局间隔；
/// - `rules.check_weekdays` 非空时仅在指定星期（1=周一 … 7=周日）检查；
/// - 从未检查过（`last_checked_at == 0`）始终视为到期。
fn subscription_due_for_check(
    sub: &Subscription,
    global_interval_minutes: i32,
    now: i64,
    weekday: i32,
) -> bool {
    if !sub.rules.check_weekdays.is_empty()
        && !sub.rules.check_weekdays.iter().any(|day| *day == weekday)
    {
        return false;
    }

    if sub.last_checked_at <= 0 {
        return true;
    }

    let interval_minutes = if sub.rules.check_interval_minutes > 0 {
        crate::models::settings::normalize_check_interval_minutes(i64::from(
            sub.rules.check_interval_minutes,
        ))
    } else {
        global_interval_minutes
    };
    let interval_secs = i64::from(interval_minutes).saturating_mul(60);
    now.saturating_sub(sub.last_checked_at) >= interval_secs
}

#[cfg(test)]
mod due_check_tests {
    use super::subscription_due_for_check;
    use crate::models::rules::TransferRules;
    use crate::models::subscription::Subscription;

    fn sample_sub(interval: i32, last_checked: i64, weekdays: Vec<i32>) -> Subscription {
        Subscription {
            id: "s1".into(),
            title: "t".into(),
            source_title: String::new(),
            media_type: "series".into(),
            season: 1,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            tags: vec![],
            metadata: None,
            manual_schedule: None,
            cloud_type: "quark".into(),
            url: "u".into(),
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
            sync_downloads: vec![],
            strm_enabled: false,
            enabled: true,
            completed: false,
            rules: TransferRules {
                check_interval_minutes: interval,
                check_weekdays: weekdays,
                ..TransferRules::default()
            },
            rule_preset_id: String::new(),
            created_at: 0,
            updated_at: 0,
            last_checked_at: last_checked,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".into(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
            source_candidates: vec![],
            last_source_search_time: None,
            previous_share_links: vec![],
            source_failure_count: 0,
            last_source_switch_at: None,
            source_switch_history: vec![],
        }
    }

    #[test]
    fn never_checked_is_due() {
        let sub = sample_sub(60, 0, vec![]);
        assert!(subscription_due_for_check(&sub, 60, 1_000, 1));
    }

    #[test]
    fn respects_per_subscription_interval() {
        let sub = sample_sub(30, 1_000, vec![]);
        assert!(!subscription_due_for_check(&sub, 60, 1_000 + 29 * 60, 1));
        assert!(subscription_due_for_check(&sub, 60, 1_000 + 30 * 60, 1));
    }

    #[test]
    fn respects_check_weekdays() {
        let sub = sample_sub(5, 0, vec![1, 3, 5]);
        assert!(subscription_due_for_check(&sub, 60, 1, 1));
        assert!(!subscription_due_for_check(&sub, 60, 1, 2));
    }
}

/// 将批量检查快照中由检查流程产生的字段合并到当前记录。
///
/// 只覆盖检查流程负责维护的字段；transferred_files/transferred_file_keys
/// （由并发的转存任务维护）以及标题、规则、启用状态等用户可编辑字段
/// 一律保留当前 Store 中的值，避免快照写回覆盖并发更新。
fn merge_check_results(
    current: &mut Subscription,
    original: Option<&Subscription>,
    checked: &Subscription,
) {
    current.known_files = checked.known_files.clone();
    current.known_file_keys = checked.known_file_keys.clone();
    current.known_episodes = checked.known_episodes.clone();
    current.current_episode_number = checked.current_episode_number;
    current.last_checked_at = checked.last_checked_at;
    current.last_new_files = checked.last_new_files.clone();
    current.last_new_episodes = checked.last_new_episodes.clone();
    current.last_check_summary = checked.last_check_summary.clone();
    current.last_probe = checked.last_probe.clone();
    current.check_history = checked.check_history.clone();
    current.status = checked.status.clone();
    current.invalid_since = checked.invalid_since;
    current.last_error = checked.last_error.clone();
    current.source_failure_count = checked.source_failure_count;
    current.completed = checked.completed;
    current.total_episode_number = checked.total_episode_number;
    current.source_candidates = checked.source_candidates.clone();
    current.last_source_search_time = checked.last_source_search_time;
    current.updated_at = current.updated_at.max(checked.updated_at);

    // 仅当批量检查期间确实发生了自动换源时，才同步换源相关字段；
    // 否则保留当前值，避免覆盖用户对分享链接的并发编辑。
    let switched_in_batch = original.is_none_or(|original| {
        checked.url != original.url
            || checked.password != original.password
            || checked.last_source_switch_at != original.last_source_switch_at
    });
    if switched_in_batch {
        current.url = checked.url.clone();
        current.password = checked.password.clone();
        current.previous_share_links = checked.previous_share_links.clone();
        current.last_source_switch_at = checked.last_source_switch_at;
        current.source_switch_history = checked.source_switch_history.clone();
    }
}

fn pansou_api_url_option(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
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
    pub episodes: Vec<i32>,
    pub special_kind: Option<String>,
    pub detection_method: String,
    pub detection_confidence: String,
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
