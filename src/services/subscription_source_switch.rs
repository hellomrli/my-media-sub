use std::collections::HashSet;
use std::sync::Arc;

use serde::Serialize;
use tracing::info;

use crate::clients::pansou::PanSouClient;
use crate::clients::quark::QuarkShareProbe;
use crate::error::{AppError, Result};
use crate::models::subscription::{
    ProbeResult, SourceCandidate, SourceSwitchHistoryItem, Subscription,
};
use crate::models::{Settings, SourceQuality};
use crate::services::episode::{is_video_name, matches_subscription_season};
use crate::services::source_quality::{score_source, SourceQualityFile, SourceQualityInput};
use crate::utils::unix_now;

#[derive(Debug, Clone, Serialize)]
pub struct SourceSwitchPreview {
    pub candidate: SourceCandidate,
    pub current_quality: SourceQuality,
    pub score_delta: i32,
    pub probe_ok: bool,
    pub season_matches: bool,
    pub covers_progress: bool,
    pub historical_link: bool,
    pub recent_failure: bool,
    pub cooldown_active: bool,
    pub failure_threshold_met: bool,
    pub can_apply: bool,
    pub auto_eligible: bool,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceSwitchRollbackResult {
    pub success: bool,
    pub restored_url: String,
    pub message: String,
}

/// 订阅换源服务
pub struct SubscriptionSourceSwitchService {
    pansou_client: PanSouClient,
}

impl SubscriptionSourceSwitchService {
    pub fn new(_quark_probe: Arc<QuarkShareProbe>) -> Self {
        Self::with_pansou_api_url(_quark_probe, None)
    }

    pub fn with_pansou_api_url(
        _quark_probe: Arc<QuarkShareProbe>,
        pansou_api_url: Option<String>,
    ) -> Self {
        Self {
            pansou_client: PanSouClient::new(pansou_api_url),
        }
    }

    /// 搜索换源候选
    pub async fn search_source_candidates(
        &self,
        subscription: &Subscription,
    ) -> Result<Vec<SourceCandidate>> {
        info!("为订阅 {} 搜索换源候选", subscription.title);

        let cloud_types = vec!["quark".to_string()];
        let mut candidates = Vec::new();
        let mut seen_urls = HashSet::new();

        for keyword in source_search_keywords(subscription) {
            info!("PanSou 换源搜索关键词: {}", keyword);
            let search_results = self
                .pansou_client
                .search(&keyword, &cloud_types, 20)
                .await?;
            info!(
                "关键词 {} 找到 {} 个 PanSou 结果",
                keyword,
                search_results.len()
            );

            for result in search_results {
                let url = result.url.trim().to_string();
                if url.is_empty() || !seen_urls.insert(url.clone()) {
                    continue;
                }

                let quality = score_source(
                    &SourceQualityInput {
                        title: result.note.clone(),
                        datetime: result.datetime.clone(),
                        validity: result.is_valid,
                        ..SourceQualityInput::default()
                    },
                    chrono::Utc::now().timestamp_millis(),
                );
                candidates.push(SourceCandidate {
                    id: result.unique_id,
                    source: result.source,
                    url,
                    password: result.password,
                    note: result.note,
                    discovered_at: unix_now(),
                    probe_info: None,
                    quality,
                });

                if candidates.len() >= 20 {
                    return Ok(candidates);
                }
            }
        }

        Ok(candidates)
    }

    /// 探测候选项（验证链接有效性）
    pub async fn probe_candidate(
        &self,
        candidate: &SourceCandidate,
        cookie: &str,
    ) -> Result<ProbeResult> {
        info!("探测候选项: {}", crate::utils::redact_url(&candidate.url));

        // 创建新的 probe 实例用于探测
        let probe = crate::clients::quark::QuarkShareProbe::new(cookie);
        let share_info = probe.probe(&candidate.url, &candidate.password, 100).await;

        // 转换为 ProbeResult
        if share_info.ok {
            Ok(ProbeResult {
                ok: true,
                state: "success".to_string(),
                message: format!("找到 {} 个文件", share_info.files.len()),
                files: share_info
                    .files
                    .into_iter()
                    .map(|item| crate::models::subscription::ProbeFile {
                        name: item.name,
                        is_dir: item.is_dir,
                        parent_path: item.parent_path,
                        size: item.size,
                        updated_at: item.updated_at,
                        file_key: item.fid,
                    })
                    .collect(),
            })
        } else {
            Ok(ProbeResult {
                ok: false,
                state: "failed".to_string(),
                message: share_info.message,
                files: vec![],
            })
        }
    }

    pub async fn probe_and_score_candidate(
        &self,
        candidate: &SourceCandidate,
        cookie: &str,
        now_ms: i64,
    ) -> Result<SourceCandidate> {
        let now = now_ms / 1000;
        let probe = if candidate.probe_info.is_some()
            && now.saturating_sub(candidate.discovered_at) <= 300
        {
            candidate.probe_info.clone().unwrap()
        } else {
            self.probe_candidate(candidate, cookie).await?
        };
        let mut candidate = candidate.clone();
        candidate.probe_info = Some(probe);
        candidate.quality = quality_for_candidate(&candidate, now_ms);
        Ok(candidate)
    }

    pub fn preview_candidate(
        &self,
        subscription: &Subscription,
        candidate: SourceCandidate,
        settings: &Settings,
        now: i64,
    ) -> SourceSwitchPreview {
        let current_quality = quality_for_current_source(subscription, now * 1000);
        let score_delta = i32::from(candidate.quality.score) - i32::from(current_quality.score);
        let probe_ok = candidate.probe_info.as_ref().is_some_and(|probe| probe.ok);
        let season_matches = candidate_matches_season(subscription, &candidate);
        let covers_progress = candidate_covers_progress(subscription, &candidate);
        let historical_link = subscription.previous_share_links.contains(&candidate.url)
            || subscription.url == candidate.url;
        let cooldown_seconds = i64::from(settings.source_switch_cooldown_hours.max(1)) * 3600;
        let recent_failure = subscription.source_switch_history.iter().any(|item| {
            item.to_url == candidate.url
                && item.status == "failed"
                && now.saturating_sub(item.created_at) < cooldown_seconds
        });
        let cooldown_active = subscription
            .last_source_switch_at
            .is_some_and(|last| now.saturating_sub(last) < cooldown_seconds);
        let failure_threshold_met = subscription.source_failure_count
            >= settings.source_switch_failure_threshold.max(1) as u32;
        let score_ok = i32::from(candidate.quality.score) >= settings.source_switch_min_score;
        let delta_ok = score_delta >= settings.source_switch_min_score_delta;
        let can_apply =
            probe_ok && season_matches && covers_progress && !historical_link && !recent_failure;
        let auto_eligible =
            can_apply && score_ok && delta_ok && !cooldown_active && failure_threshold_met;

        let mut reasons = candidate.quality.recommendation_reasons.clone();
        if score_ok {
            reasons.push(format!("质量分达到 {}", candidate.quality.score));
        }
        if delta_ok {
            reasons.push(format!("比当前来源高 {} 分", score_delta));
        }
        if covers_progress && subscription.media_type != "movie" {
            reasons.push("候选覆盖当前追更进度".to_string());
        }
        reasons.sort();
        reasons.dedup();

        let mut warnings = Vec::new();
        if !probe_ok {
            warnings.push("候选尚未探测成功".to_string());
        }
        if !season_matches {
            warnings.push("候选季度与订阅不匹配".to_string());
        }
        if !covers_progress {
            warnings.push("候选未覆盖当前追更进度".to_string());
        }
        if historical_link {
            warnings.push("候选属于当前或历史分享链接".to_string());
        }
        if recent_failure {
            warnings.push("候选近期应用或探测失败，仍在冷却期".to_string());
        }
        if cooldown_active {
            warnings.push("订阅仍在换源冷却期".to_string());
        }
        if !failure_threshold_met {
            warnings.push(format!(
                "连续失效 {} 次，尚未达到阈值 {}",
                subscription.source_failure_count,
                settings.source_switch_failure_threshold.max(1)
            ));
        }
        if !score_ok {
            warnings.push(format!(
                "候选质量分 {} 低于阈值 {}",
                candidate.quality.score, settings.source_switch_min_score
            ));
        }
        if !delta_ok {
            warnings.push(format!(
                "候选分差 {} 低于阈值 {}",
                score_delta, settings.source_switch_min_score_delta
            ));
        }

        SourceSwitchPreview {
            candidate,
            current_quality,
            score_delta,
            probe_ok,
            season_matches,
            covers_progress,
            historical_link,
            recent_failure,
            cooldown_active,
            failure_threshold_met,
            can_apply,
            auto_eligible,
            reasons,
            warnings,
        }
    }

    pub fn best_auto_candidate(
        &self,
        subscription: &Subscription,
        candidates: &[SourceCandidate],
        settings: &Settings,
        now: i64,
    ) -> Option<SourceSwitchPreview> {
        candidates
            .iter()
            .cloned()
            .map(|candidate| self.preview_candidate(subscription, candidate, settings, now))
            .filter(|preview| preview.auto_eligible)
            .max_by_key(|preview| preview.candidate.quality.score)
    }

    /// 应用换源（替换订阅的分享链接），保留原进度并写入审计历史。
    pub fn apply_source_switch(
        &self,
        subscription: &mut Subscription,
        candidate_id: &str,
    ) -> Result<()> {
        self.apply_source_switch_with_audit(subscription, candidate_id, false, "手动应用候选")
    }

    pub fn apply_source_switch_with_audit(
        &self,
        subscription: &mut Subscription,
        candidate_id: &str,
        automatic: bool,
        reason: &str,
    ) -> Result<()> {
        let candidate = subscription
            .source_candidates
            .iter()
            .find(|candidate| candidate.id == candidate_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound("候选项不存在".to_string()))?;

        info!(
            "应用换源: {} -> {}",
            crate::utils::redact_url(&subscription.url),
            crate::utils::redact_url(&candidate.url)
        );
        let now = unix_now();
        let from_url = subscription.url.clone();
        let from_password = subscription.password.clone();

        if !subscription.previous_share_links.contains(&from_url) {
            subscription.previous_share_links.push(from_url.clone());
            if subscription.previous_share_links.len() > 50 {
                let remove = subscription.previous_share_links.len() - 50;
                subscription.previous_share_links.drain(0..remove);
            }
        }
        subscription.url = candidate.url.clone();
        subscription.password = candidate.password.clone();
        subscription.status = "active".to_string();
        subscription.invalid_since = None;
        subscription.last_error = String::new();
        subscription.completed = false;
        subscription.last_probe = None;
        subscription.last_new_files.clear();
        subscription.last_new_episodes.clear();
        subscription.last_check_summary = "已更换订阅资源，等待立即检查".to_string();
        subscription.source_failure_count = 0;
        subscription.last_source_switch_at = Some(now);
        if subscription.media_type != "movie" && subscription.current_episode_number > 0 {
            subscription.start_episode_number = Some(subscription.current_episode_number + 1);
        }
        subscription.source_switch_history.insert(
            0,
            SourceSwitchHistoryItem {
                id: format!("switch-{}-{}", now, candidate.id),
                candidate_id: candidate.id.clone(),
                from_url,
                from_password,
                to_url: candidate.url.clone(),
                to_password: candidate.password.clone(),
                quality: candidate.quality.clone(),
                reason: reason.to_string(),
                status: "succeeded".to_string(),
                error: String::new(),
                automatic,
                created_at: now,
                rolled_back_at: None,
            },
        );
        subscription.source_switch_history.truncate(50);
        subscription.source_candidates.clear();

        info!("换源成功，已保留追更进度并记录审计历史");
        Ok(())
    }

    pub fn record_candidate_failure(
        &self,
        subscription: &mut Subscription,
        candidate: &SourceCandidate,
        error: &str,
        automatic: bool,
    ) {
        let now = unix_now();
        subscription.source_switch_history.insert(
            0,
            SourceSwitchHistoryItem {
                id: format!("switch-failed-{}-{}", now, candidate.id),
                candidate_id: candidate.id.clone(),
                from_url: subscription.url.clone(),
                from_password: subscription.password.clone(),
                to_url: candidate.url.clone(),
                to_password: candidate.password.clone(),
                quality: candidate.quality.clone(),
                reason: "候选探测或策略应用失败".to_string(),
                status: "failed".to_string(),
                error: error.to_string(),
                automatic,
                created_at: now,
                rolled_back_at: None,
            },
        );
        subscription.source_switch_history.truncate(50);
    }

    pub fn rollback_last_source(
        &self,
        subscription: &mut Subscription,
    ) -> Result<SourceSwitchRollbackResult> {
        let index = subscription
            .source_switch_history
            .iter()
            .position(|item| item.status == "succeeded" && item.rolled_back_at.is_none())
            .ok_or_else(|| AppError::Validation("没有可回滚的换源历史".to_string()))?;
        let history = subscription.source_switch_history[index].clone();
        let now = unix_now();
        let current_url = subscription.url.clone();
        let current_password = subscription.password.clone();
        subscription.url = history.from_url.clone();
        subscription.password = history.from_password.clone();
        subscription.status = "active".to_string();
        subscription.invalid_since = None;
        subscription.last_error = String::new();
        subscription.completed = false;
        subscription.last_probe = None;
        subscription.last_new_files.clear();
        subscription.last_new_episodes.clear();
        subscription.last_check_summary = "已回滚到上一来源，等待立即检查".to_string();
        subscription.last_source_switch_at = Some(now);
        subscription.source_switch_history[index].rolled_back_at = Some(now);
        subscription.source_switch_history.insert(
            0,
            SourceSwitchHistoryItem {
                id: format!("switch-rollback-{}", now),
                candidate_id: history.candidate_id,
                from_url: current_url,
                from_password: current_password,
                to_url: history.from_url.clone(),
                to_password: history.from_password,
                quality: SourceQuality::default(),
                reason: "回滚上一来源".to_string(),
                status: "rolled_back".to_string(),
                error: String::new(),
                automatic: false,
                created_at: now,
                rolled_back_at: None,
            },
        );
        subscription.source_switch_history.truncate(50);
        subscription.source_candidates.clear();
        Ok(SourceSwitchRollbackResult {
            success: true,
            restored_url: history.from_url,
            message: "已回滚到上一来源".to_string(),
        })
    }
}

fn quality_for_candidate(candidate: &SourceCandidate, now_ms: i64) -> SourceQuality {
    let probe = candidate.probe_info.as_ref();
    score_source(
        &SourceQualityInput {
            title: candidate.note.clone(),
            datetime: candidate
                .quality
                .updated_at
                .clone()
                .unwrap_or_else(|| candidate.discovered_at.to_string()),
            validity: probe.map(|probe| probe.ok),
            probe_ok: probe.map(|probe| probe.ok),
            probe_file_count: probe.map(|probe| probe.files.len()).unwrap_or_default(),
            probe_episode_count: 0,
            files: probe
                .map(|probe| {
                    probe
                        .files
                        .iter()
                        .map(|file| SourceQualityFile {
                            name: file.name.clone(),
                            is_dir: file.is_dir,
                            size: file.size,
                            updated_at: file.updated_at.clone(),
                            category: None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        now_ms,
    )
}

fn quality_for_current_source(subscription: &Subscription, now_ms: i64) -> SourceQuality {
    let probe = subscription.last_probe.as_ref();
    score_source(
        &SourceQualityInput {
            title: if subscription.source_title.trim().is_empty() {
                subscription.title.clone()
            } else {
                subscription.source_title.clone()
            },
            datetime: subscription.last_checked_at.to_string(),
            validity: probe.map(|probe| probe.ok),
            probe_ok: probe.map(|probe| probe.ok),
            probe_file_count: probe.map(|probe| probe.files.len()).unwrap_or_default(),
            probe_episode_count: 0,
            files: probe
                .map(|probe| {
                    probe
                        .files
                        .iter()
                        .map(|file| SourceQualityFile {
                            name: file.name.clone(),
                            is_dir: file.is_dir,
                            size: file.size,
                            updated_at: file.updated_at.clone(),
                            category: None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        now_ms,
    )
}

fn candidate_matches_season(subscription: &Subscription, candidate: &SourceCandidate) -> bool {
    if subscription.media_type == "movie" {
        return true;
    }
    let Some(probe) = candidate.probe_info.as_ref().filter(|probe| probe.ok) else {
        return false;
    };
    probe.files.iter().any(|file| {
        is_video_name(&file.name)
            && matches_subscription_season(&file.name, &file.parent_path, subscription.season)
    })
}

fn candidate_covers_progress(subscription: &Subscription, candidate: &SourceCandidate) -> bool {
    if subscription.media_type == "movie" {
        return candidate.quality.video_count > 0;
    }
    let required_episode = subscription
        .start_episode_number
        .unwrap_or(subscription.current_episode_number.saturating_add(1).max(1));
    candidate
        .quality
        .episode_end
        .is_some_and(|episode| episode >= required_episode)
}

fn source_search_keywords(subscription: &Subscription) -> Vec<String> {
    let mut titles = Vec::new();
    push_unique(&mut titles, &subscription.source_title);
    push_unique(&mut titles, &subscription.title);
    if let Some(metadata) = &subscription.metadata {
        push_unique(&mut titles, &metadata.title);
        push_unique(&mut titles, &metadata.original_title);
    }

    let mut base_titles = Vec::new();
    for title in titles {
        push_unique(&mut base_titles, &clean_search_title(&title));
        push_unique(&mut base_titles, &title);
    }

    let season = if subscription.media_type == "movie" {
        0
    } else {
        subscription.season.max(1)
    };
    let mut keywords = Vec::new();
    for title in base_titles {
        push_unique(&mut keywords, &title);
        if season > 0 {
            push_unique(&mut keywords, &format!("{} S{:02}", title, season));
            push_unique(&mut keywords, &format!("{} Season {}", title, season));
            push_unique(&mut keywords, &format!("{} 第{}季", title, season));
            if let Some(chinese) = chinese_season_number(season) {
                push_unique(&mut keywords, &format!("{} 第{}季", title, chinese));
            }
        }
    }

    keywords
}

fn clean_search_title(title: &str) -> String {
    let mut output = String::new();
    let mut bracket_depth = 0usize;
    for ch in title.chars() {
        match ch {
            '[' | '【' | '(' | '（' => bracket_depth += 1,
            ']' | '】' | ')' | '）' => bracket_depth = bracket_depth.saturating_sub(1),
            '.' | '_' | '-' => {
                if bracket_depth == 0 {
                    output.push(' ');
                }
            }
            _ if bracket_depth == 0 => output.push(ch),
            _ => {}
        }
    }
    normalize_spaces(&output)
}

fn normalize_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn push_unique(items: &mut Vec<String>, value: &str) {
    let value = normalize_spaces(value);
    if value.is_empty() || items.iter().any(|item| item == &value) {
        return;
    }
    items.push(value);
}

fn chinese_season_number(season: i32) -> Option<&'static str> {
    match season {
        1 => Some("一"),
        2 => Some("二"),
        3 => Some("三"),
        4 => Some("四"),
        5 => Some("五"),
        6 => Some("六"),
        7 => Some("七"),
        8 => Some("八"),
        9 => Some("九"),
        10 => Some("十"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_subscription() -> Subscription {
        Subscription {
            id: "test".to_string(),
            title: "测试剧集".to_string(),
            source_title: String::new(),
            media_type: String::new(),
            season: 2,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            tags: vec![],
            metadata: None,
            manual_schedule: None,
            cloud_type: "quark".to_string(),
            url: "".to_string(),
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
            rules: Default::default(),
            rule_preset_id: String::new(),
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
            source_candidates: vec![],
            last_source_search_time: None,
            previous_share_links: vec![],
            source_failure_count: 0,
            last_source_switch_at: None,
            source_switch_history: vec![],
        }
    }

    #[test]
    fn test_source_search_keywords_include_plain_title_before_season_variants() {
        let sub = test_subscription();
        let keywords = source_search_keywords(&sub);

        assert_eq!(keywords[0], "测试剧集");
        assert!(keywords.contains(&"测试剧集 S02".to_string()));
        assert!(keywords.contains(&"测试剧集 第二季".to_string()));
    }

    #[test]
    fn test_source_search_keywords_clean_subtitle_group() {
        let mut sub = test_subscription();
        sub.source_title = "【字幕组】 测试剧集 - 1080p".to_string();

        let keywords = source_search_keywords(&sub);

        assert!(keywords.contains(&"测试剧集 1080p".to_string()));
        assert!(keywords.contains(&"测试剧集 1080p S02".to_string()));
    }

    #[test]
    fn test_apply_source_switch_reactivates_subscription() {
        let mut sub = test_subscription();
        sub.url = "https://pan.quark.cn/s/old".to_string();
        sub.password = "oldpwd".to_string();
        sub.status = "invalid".to_string();
        sub.invalid_since = Some(123);
        sub.last_error = "分享已失效".to_string();
        sub.media_type = "anime".to_string();
        sub.current_episode_number = 12;
        sub.start_episode_number = Some(10);
        sub.known_files = vec!["old.mkv".to_string()];
        sub.known_file_keys = vec!["old-key".to_string()];
        sub.known_episodes = vec![1, 12];
        sub.transferred_files = vec!["old.mkv".to_string()];
        sub.transferred_file_keys = vec!["ep:12".to_string()];
        sub.last_probe = Some(ProbeResult {
            ok: true,
            state: "success".to_string(),
            message: "old".to_string(),
            files: vec![],
        });
        sub.last_new_files = vec!["old.mkv".to_string()];
        sub.last_new_episodes = vec![12];
        sub.last_check_summary = "旧检查结果".to_string();
        sub.source_candidates = vec![SourceCandidate {
            id: "candidate-1".to_string(),
            source: "pansou".to_string(),
            url: "https://pan.quark.cn/s/new".to_string(),
            password: "newpwd".to_string(),
            note: "新资源".to_string(),
            discovered_at: 456,
            probe_info: None,
            quality: crate::models::SourceQuality::default(),
        }];

        let service = SubscriptionSourceSwitchService::new(Arc::new(QuarkShareProbe::new("")));
        service
            .apply_source_switch(&mut sub, "candidate-1")
            .unwrap();

        assert_eq!(sub.url, "https://pan.quark.cn/s/new");
        assert_eq!(sub.password, "newpwd");
        assert_eq!(sub.previous_share_links, vec!["https://pan.quark.cn/s/old"]);
        assert_eq!(sub.status, "active");
        assert_eq!(sub.invalid_since, None);
        assert!(sub.last_error.is_empty());
        assert!(!sub.completed);
        assert_eq!(sub.start_episode_number, Some(13));
        assert!(sub.source_candidates.is_empty());
        assert_eq!(sub.known_files, vec!["old.mkv"]);
        assert_eq!(sub.known_file_keys, vec!["old-key"]);
        assert_eq!(sub.known_episodes, vec![1, 12]);
        assert_eq!(sub.transferred_file_keys, vec!["ep:12"]);
        assert!(sub.last_probe.is_none());
        assert!(sub.last_new_files.is_empty());
        assert!(sub.last_new_episodes.is_empty());
        assert_eq!(sub.last_check_summary, "已更换订阅资源，等待立即检查");
        assert_eq!(sub.source_switch_history.len(), 1);
        assert_eq!(sub.source_switch_history[0].status, "succeeded");
    }

    fn eligible_candidate() -> SourceCandidate {
        SourceCandidate {
            id: "candidate-good".to_string(),
            source: "fixture".to_string(),
            url: "https://pan.quark.cn/s/good".to_string(),
            password: String::new(),
            note: "测试剧集 S02 2160P HDR H265".to_string(),
            discovered_at: 1_700_000_000,
            probe_info: Some(ProbeResult {
                ok: true,
                state: "success".to_string(),
                message: String::new(),
                files: vec![crate::models::subscription::ProbeFile {
                    name: "Show.S02E13.2160p.HDR.HEVC.mkv".to_string(),
                    is_dir: false,
                    parent_path: "Season 2".to_string(),
                    size: 4_000_000_000,
                    updated_at: Some("2026-07-10T00:00:00Z".to_string()),
                    file_key: "ep13".to_string(),
                }],
            }),
            quality: SourceQuality {
                score: 92,
                grade: "旗舰".to_string(),
                tone: "excellent".to_string(),
                tags: vec!["4K".to_string(), "HDR".to_string()],
                risks: vec![],
                resolution: "4K".to_string(),
                file_count: 1,
                video_count: 1,
                episode_count: 1,
                episode_start: Some(13),
                episode_end: Some(13),
                total_size: 4_000_000_000,
                updated_at: Some("2026-07-10T00:00:00Z".to_string()),
                recommendation_reasons: vec!["综合质量优秀".to_string()],
            },
        }
    }

    #[test]
    fn preview_requires_probe_season_progress_score_and_failure_threshold() {
        let service = SubscriptionSourceSwitchService::new(Arc::new(QuarkShareProbe::new("")));
        let mut sub = test_subscription();
        sub.url = "https://pan.quark.cn/s/current".to_string();
        sub.media_type = "series".to_string();
        sub.season = 2;
        sub.current_episode_number = 12;
        sub.start_episode_number = Some(13);
        sub.source_failure_count = 2;
        let settings = Settings {
            source_switch_min_score: 70,
            source_switch_min_score_delta: 10,
            source_switch_failure_threshold: 2,
            source_switch_cooldown_hours: 24,
            ..Settings::default()
        };

        let preview =
            service.preview_candidate(&sub, eligible_candidate(), &settings, 1_783_656_000);

        assert!(preview.probe_ok);
        assert!(preview.season_matches);
        assert!(preview.covers_progress);
        assert!(preview.can_apply);
        assert!(preview.auto_eligible);
        assert!(preview.score_delta >= 10);
    }

    #[test]
    fn preview_blocks_historical_and_recent_failed_candidates() {
        let service = SubscriptionSourceSwitchService::new(Arc::new(QuarkShareProbe::new("")));
        let mut sub = test_subscription();
        sub.url = "https://pan.quark.cn/s/current".to_string();
        sub.media_type = "series".to_string();
        sub.season = 2;
        sub.current_episode_number = 12;
        sub.source_failure_count = 3;
        let candidate = eligible_candidate();
        sub.previous_share_links.push(candidate.url.clone());
        service.record_candidate_failure(&mut sub, &candidate, "probe failed", true);
        let settings = Settings::default();
        let preview = service.preview_candidate(&sub, candidate, &settings, unix_now());

        assert!(preview.historical_link);
        assert!(preview.recent_failure);
        assert!(!preview.can_apply);
        assert!(!preview.auto_eligible);
    }

    #[test]
    fn rollback_restores_previous_url_and_preserves_progress() {
        let service = SubscriptionSourceSwitchService::new(Arc::new(QuarkShareProbe::new("")));
        let mut sub = test_subscription();
        sub.url = "https://pan.quark.cn/s/current".to_string();
        sub.password = "old".to_string();
        sub.known_episodes = vec![1, 12];
        sub.transferred_file_keys = vec!["ep:12".to_string()];
        sub.source_candidates = vec![eligible_candidate()];
        service
            .apply_source_switch_with_audit(&mut sub, "candidate-good", true, "自动策略")
            .unwrap();
        assert_eq!(sub.url, "https://pan.quark.cn/s/good");

        let result = service.rollback_last_source(&mut sub).unwrap();

        assert!(result.success);
        assert_eq!(sub.url, "https://pan.quark.cn/s/current");
        assert_eq!(sub.password, "old");
        assert_eq!(sub.known_episodes, vec![1, 12]);
        assert_eq!(sub.transferred_file_keys, vec!["ep:12"]);
        assert_eq!(sub.source_switch_history[0].status, "rolled_back");
        assert!(sub.source_switch_history[1].rolled_back_at.is_some());
    }

    #[test]
    fn automatic_selection_uses_highest_eligible_candidate() {
        let service = SubscriptionSourceSwitchService::new(Arc::new(QuarkShareProbe::new("")));
        let mut sub = test_subscription();
        sub.url = "https://pan.quark.cn/s/current".to_string();
        sub.media_type = "series".to_string();
        sub.season = 2;
        sub.current_episode_number = 12;
        sub.start_episode_number = Some(13);
        sub.source_failure_count = 2;
        let mut lower = eligible_candidate();
        lower.id = "lower".to_string();
        lower.url = "https://pan.quark.cn/s/lower".to_string();
        lower.quality.score = 86;
        let higher = eligible_candidate();
        let settings = Settings {
            source_switch_min_score: 70,
            source_switch_min_score_delta: 10,
            source_switch_failure_threshold: 2,
            source_switch_cooldown_hours: 24,
            ..Settings::default()
        };

        let best = service
            .best_auto_candidate(&sub, &[lower, higher], &settings, 1_783_656_000)
            .unwrap();

        assert_eq!(best.candidate.id, "candidate-good");
        assert!(best.auto_eligible);
    }

    #[test]
    fn cooldown_and_duplicate_application_are_enforced() {
        let service = SubscriptionSourceSwitchService::new(Arc::new(QuarkShareProbe::new("")));
        let now = 1_783_656_000;
        let mut sub = test_subscription();
        sub.url = "https://pan.quark.cn/s/current".to_string();
        sub.media_type = "series".to_string();
        sub.season = 2;
        sub.current_episode_number = 12;
        sub.start_episode_number = Some(13);
        sub.source_failure_count = 3;
        sub.last_source_switch_at = Some(now - 60);
        let candidate = eligible_candidate();
        sub.source_candidates = vec![candidate.clone()];
        let settings = Settings {
            source_switch_failure_threshold: 2,
            source_switch_cooldown_hours: 24,
            ..Settings::default()
        };
        let preview = service.preview_candidate(&sub, candidate, &settings, now);
        assert!(preview.can_apply);
        assert!(preview.cooldown_active);
        assert!(!preview.auto_eligible);

        service
            .apply_source_switch(&mut sub, "candidate-good")
            .unwrap();
        assert_eq!(sub.source_switch_history.len(), 1);
        assert!(service
            .apply_source_switch(&mut sub, "candidate-good")
            .is_err());
        assert_eq!(sub.source_switch_history.len(), 1);
    }
}
