use std::sync::Arc;
use tracing::info;

use crate::clients::pansou::PanSouClient;
use crate::clients::quark::QuarkShareProbe;
use crate::error::{AppError, Result};
use crate::models::subscription::{ProbeResult, SourceCandidate, Subscription};
use crate::utils::unix_now;

/// 订阅换源服务
pub struct SubscriptionSourceSwitchService {
    pansou_client: PanSouClient,
}

impl SubscriptionSourceSwitchService {
    pub fn new(_quark_probe: Arc<QuarkShareProbe>) -> Self {
        Self {
            pansou_client: PanSouClient::default(),
        }
    }

    /// 搜索换源候选
    pub async fn search_source_candidates(
        &self,
        subscription: &Subscription,
    ) -> Result<Vec<SourceCandidate>> {
        info!("为订阅 {} 搜索换源候选", subscription.title);

        // 构建搜索关键词（优先使用原标题，回退到当前标题）
        let keyword = if !subscription.source_title.is_empty() {
            &subscription.source_title
        } else {
            &subscription.title
        };

        // 如果是剧集，添加季度信息
        let search_keyword = if subscription.season > 0 {
            format!("{} S{:02}", keyword, subscription.season)
        } else {
            keyword.to_string()
        };

        info!("搜索关键词: {}", search_keyword);

        // 调用 PanSou 搜索
        let cloud_types = vec!["quark".to_string()];
        let search_results = self
            .pansou_client
            .search(&search_keyword, &cloud_types, 10)
            .await?;

        info!("找到 {} 个搜索结果", search_results.len());

        // 转换为候选项
        let mut candidates = Vec::new();
        for result in search_results {
            let candidate_id = result.unique_id.clone();

            candidates.push(SourceCandidate {
                id: candidate_id,
                source: result.source,
                url: result.url,
                password: result.password,
                note: result.note,
                discovered_at: unix_now(),
                probe_info: None, // 稍后按需探测
            });
        }

        Ok(candidates)
    }

    /// 探测候选项（验证链接有效性）
    pub async fn probe_candidate(
        &self,
        candidate: &SourceCandidate,
        cookie: &str,
    ) -> Result<ProbeResult> {
        info!("探测候选项: {}", candidate.url);

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

    /// 应用换源（替换订阅的分享链接）
    pub fn apply_source_switch(
        &self,
        subscription: &mut Subscription,
        candidate_id: &str,
    ) -> Result<()> {
        let candidate = subscription
            .source_candidates
            .iter()
            .find(|c| c.id == candidate_id)
            .ok_or_else(|| AppError::NotFound("候选项不存在".to_string()))?;

        info!("应用换源: {} -> {}", subscription.url, candidate.url);

        // 保存旧链接到历史
        subscription
            .previous_share_links
            .push(subscription.url.clone());

        // 替换为新链接
        subscription.url = candidate.url.clone();
        subscription.password = candidate.password.clone();

        // 重置失效状态
        subscription.status = "active".to_string();
        subscription.invalid_since = None;
        subscription.last_error = String::new();

        // 清空候选列表
        subscription.source_candidates.clear();

        // 清空已知文件（因为换了新源，需要重新检查）
        subscription.known_files.clear();
        subscription.known_file_keys.clear();
        subscription.known_episodes.clear();

        info!("换源成功，已重置订阅状态");

        Ok(())
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
            metadata: None,
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
        }
    }

    #[test]
    fn test_search_keyword_with_season() {
        let sub = test_subscription();
        let keyword = if !sub.source_title.is_empty() {
            &sub.source_title
        } else {
            &sub.title
        };

        let search_keyword = if sub.season > 0 {
            format!("{} S{:02}", keyword, sub.season)
        } else {
            keyword.to_string()
        };

        assert_eq!(search_keyword, "测试剧集 S02");
    }

    #[test]
    fn test_apply_source_switch_reactivates_subscription() {
        let mut sub = test_subscription();
        sub.url = "https://pan.quark.cn/s/old".to_string();
        sub.password = "oldpwd".to_string();
        sub.status = "invalid".to_string();
        sub.invalid_since = Some(123);
        sub.last_error = "分享已失效".to_string();
        sub.known_files = vec!["old.mkv".to_string()];
        sub.known_file_keys = vec!["old-key".to_string()];
        sub.known_episodes = vec![1];
        sub.source_candidates = vec![SourceCandidate {
            id: "candidate-1".to_string(),
            source: "pansou".to_string(),
            url: "https://pan.quark.cn/s/new".to_string(),
            password: "newpwd".to_string(),
            note: "新资源".to_string(),
            discovered_at: 456,
            probe_info: None,
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
        assert!(sub.source_candidates.is_empty());
        assert!(sub.known_files.is_empty());
        assert!(sub.known_file_keys.is_empty());
        assert!(sub.known_episodes.is_empty());
    }
}
