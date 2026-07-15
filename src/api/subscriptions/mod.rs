use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::Arc;

use super::response::ApiResponse as Response;
use crate::error::{AppError, Result};
use crate::jobs::{JobQueue, JobStore, MetadataScrapePayload};
use crate::models::{
    episode_count_for_season, merge_refreshed_metadata, MediaMetadata, MediaScheduleOverride,
    Settings, Subscription, TransferRules,
};
use crate::providers::validate_cloud_type;
use crate::services::media_calendar::validate_manual_schedule;
use crate::services::subscription_check::CheckDetails;
use crate::services::subscription_progress::reconcile_completed_subscription_status;
use crate::services::subscription_status::{build_subscription_detail, SubscriptionDetail};
use crate::services::transfer_rule::{
    build_transfer_plan, effective_rules, summarize_rules, ProbeFile as RuleProbeFile,
};
use crate::services::{SubscriptionCheckService, SubscriptionTransferService};
use crate::store::{AutomationEventStore, NotificationStore, SettingsStore, SubscriptionStore};
use crate::utils::unix_now;

mod actions;
mod crud;
mod metadata;
mod source;
mod status;

use actions::{
    audit_existing_strm_files, check_all_subscriptions, check_subscription,
    generate_existing_strm_files, rename_existing_files,
};
use crud::{
    create_subscription, delete_subscription, get_subscription, list_subscriptions,
    update_subscription,
};
use metadata::{
    preview_subscription_rename, scrape_all_subscription_metadata, scrape_subscription_metadata,
};
use source::{
    apply_source_change_options, continue_from_current_episode_default,
    normalize_start_episode_number, reconcile_completion_status, reset_progress_for_content_change,
};
use status::get_subscription_status;

/// 订阅路由状态
pub struct SubscriptionState {
    pub store: Arc<SubscriptionStore>,
    pub settings_store: Arc<SettingsStore>,
    pub check_service: Arc<SubscriptionCheckService>,
    pub transfer_service: Arc<SubscriptionTransferService>,
    pub job_queue: Arc<JobQueue>,
    pub job_store: Arc<JobStore>,
    pub notification_store: Arc<NotificationStore>,
    pub automation_event_store: Arc<AutomationEventStore>,
}

/// 创建订阅请求
#[derive(Debug, Deserialize)]
pub struct CreateSubscriptionRequest {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub media_type: String,
    #[serde(default)]
    pub season: i32,
    #[serde(default)]
    pub start_episode_number: Option<i32>,
    #[serde(default)]
    pub cloud_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub target_dir: String,
    #[serde(default)]
    pub rename_template: String,
    #[serde(default)]
    pub notify_only: bool,
    #[serde(default)]
    pub sync_download_enabled: bool,
    #[serde(default)]
    pub sync_download_dir: String,
    #[serde(default)]
    pub strm_enabled: bool,
    #[serde(default)]
    pub metadata: Option<MediaMetadata>,
    #[serde(default)]
    pub manual_schedule: Option<MediaScheduleOverride>,
    #[serde(default)]
    pub rules: Option<TransferRules>,
    #[serde(default)]
    pub rule_preset_id: String,
}

/// 更新订阅请求
#[derive(Debug, Deserialize)]
pub struct UpdateSubscriptionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_episode_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_download_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_download_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strm_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_progress_on_source_change: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_from_current_episode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_episode_number: Option<Option<i32>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub metadata: Option<Option<MediaMetadata>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub manual_schedule: Option<Option<MediaScheduleOverride>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<TransferRules>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rename_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_preset_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ScrapeMetadataRequest {
    #[serde(default)]
    pub overwrite: bool,
}

fn deserialize_present_option<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct RenamePreviewRequest {
    #[serde(default)]
    pub subscription_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub season: Option<i32>,
    #[serde(default)]
    pub start_episode_number: Option<i32>,
    #[serde(default)]
    pub rules: Option<TransferRules>,
    #[serde(default)]
    pub target_dir: Option<String>,
    #[serde(default)]
    pub rename_template: Option<String>,
    #[serde(default)]
    pub sample_files: Vec<RenamePreviewFile>,
}

#[derive(Debug, Deserialize)]
pub struct RenamePreviewFile {
    pub name: String,
    #[serde(default)]
    pub fid: String,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub parent_path: String,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CheckSubscriptionRequest {
    #[serde(default)]
    pub force_transfer: bool,
}

#[derive(Debug, Deserialize, Default)]
struct ListSubscriptionsQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct RenamePreviewResponse {
    summary: String,
    target_dir: String,
    transfer_count: usize,
    skip_count: usize,
    matched_count: usize,
    current_episode_number: i32,
    episodes: Vec<i32>,
    missing_episodes: Vec<i32>,
    duplicate_episodes: Vec<i32>,
    items: Vec<RenamePreviewItem>,
}

#[derive(Serialize)]
struct RenamePreviewItem {
    source_name: String,
    target_name: String,
    action: String,
    skip_reason: String,
    episode: Option<i32>,
    episodes: Vec<i32>,
    season: Option<i32>,
    target_dir: String,
}

fn preset_rules(settings: &Settings, preset_id: &str) -> Option<TransferRules> {
    let preset_id = preset_id.trim();
    if preset_id.is_empty() {
        return None;
    }
    settings
        .rule_presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .map(|preset| preset.rules.clone())
}

fn create_rules(req: &CreateSubscriptionRequest, settings: &Settings) -> TransferRules {
    let mut rules = req
        .rules
        .clone()
        .or_else(|| preset_rules(settings, &req.rule_preset_id))
        .unwrap_or_default();
    if !req.target_dir.trim().is_empty() {
        rules.target_dir = req.target_dir.clone();
    }
    if !req.rename_template.trim().is_empty() {
        rules.rename_template = req.rename_template.clone();
    }
    rules
}

fn preview_rules(req: &RenamePreviewRequest, base: Option<&Subscription>) -> TransferRules {
    let mut rules = req
        .rules
        .clone()
        .or_else(|| base.map(|sub| sub.rules.clone()))
        .unwrap_or_default();
    if let Some(target_dir) = &req.target_dir {
        rules.target_dir = target_dir.clone();
    }
    if let Some(rename_template) = &req.rename_template {
        rules.rename_template = rename_template.clone();
    }
    rules
}

fn preview_subscription(req: &RenamePreviewRequest, base: Option<&Subscription>) -> Subscription {
    let now = unix_now();
    let rules = preview_rules(req, base);
    let title = req
        .title
        .clone()
        .or_else(|| base.map(|sub| sub.title.clone()))
        .unwrap_or_else(|| "未命名".to_string());

    Subscription {
        id: base
            .map(|sub| sub.id.clone())
            .unwrap_or_else(|| "preview".to_string()),
        title,
        source_title: base.map(|sub| sub.source_title.clone()).unwrap_or_default(),
        media_type: req
            .media_type
            .clone()
            .or_else(|| base.map(|sub| sub.media_type.clone()))
            .unwrap_or_else(|| "series".to_string()),
        season: req
            .season
            .or_else(|| base.map(|sub| sub.season))
            .filter(|season| *season > 0)
            .unwrap_or(1),
        start_episode_number: normalize_start_episode_number(
            req.start_episode_number
                .or_else(|| base.and_then(|sub| sub.start_episode_number)),
            req.media_type
                .as_deref()
                .or_else(|| base.map(|sub| sub.media_type.as_str()))
                .unwrap_or("series"),
        ),
        current_episode_number: base.map(|sub| sub.current_episode_number).unwrap_or(0),
        total_episode_number: base.and_then(|sub| sub.total_episode_number),
        source_group: base.map(|sub| sub.source_group.clone()).unwrap_or_default(),
        tags: base.map(|sub| sub.tags.clone()).unwrap_or_default(),
        metadata: base.and_then(|sub| sub.metadata.clone()),
        manual_schedule: base.and_then(|sub| sub.manual_schedule.clone()),
        cloud_type: base
            .map(|sub| sub.cloud_type.clone())
            .unwrap_or_else(|| "quark".to_string()),
        url: req
            .url
            .clone()
            .or_else(|| base.map(|sub| sub.url.clone()))
            .unwrap_or_default(),
        password: req
            .password
            .clone()
            .or_else(|| base.map(|sub| sub.password.clone()))
            .unwrap_or_default(),
        known_files: vec![],
        known_file_keys: vec![],
        known_episodes: vec![],
        transferred_files: base
            .map(|sub| sub.transferred_files.clone())
            .unwrap_or_default(),
        transferred_file_keys: base
            .map(|sub| sub.transferred_file_keys.clone())
            .unwrap_or_default(),
        last_probe: base.and_then(|sub| sub.last_probe.clone()),
        last_plan_summary: String::new(),
        notify_only: base.map(|sub| sub.notify_only).unwrap_or(false),
        sync_download_enabled: base.map(|sub| sub.sync_download_enabled).unwrap_or(false),
        sync_download_dir: base
            .map(|sub| sub.sync_download_dir.clone())
            .unwrap_or_default(),
        sync_downloads: base
            .map(|sub| sub.sync_downloads.clone())
            .unwrap_or_default(),
        strm_enabled: base.map(|sub| sub.strm_enabled).unwrap_or(false),
        enabled: true,
        completed: false,
        rules,
        rule_preset_id: base
            .map(|sub| sub.rule_preset_id.clone())
            .unwrap_or_default(),
        created_at: base.map(|sub| sub.created_at).unwrap_or(now),
        updated_at: now,
        last_checked_at: base.map(|sub| sub.last_checked_at).unwrap_or(now),
        last_new_files: vec![],
        last_new_episodes: vec![],
        last_check_summary: String::new(),
        check_history: vec![],
        status: base
            .map(|sub| sub.status.clone())
            .unwrap_or_else(|| "active".to_string()),
        invalid_since: None,
        last_error: String::new(),
        rule_summary: String::new(),
        source_candidates: base
            .map(|sub| sub.source_candidates.clone())
            .unwrap_or_default(),
        last_source_search_time: base.and_then(|sub| sub.last_source_search_time),
        previous_share_links: base
            .map(|sub| sub.previous_share_links.clone())
            .unwrap_or_default(),
        source_failure_count: base.map(|sub| sub.source_failure_count).unwrap_or_default(),
        last_source_switch_at: base.and_then(|sub| sub.last_source_switch_at),
        source_switch_history: base
            .map(|sub| sub.source_switch_history.clone())
            .unwrap_or_default(),
    }
}

fn preview_files(req: &RenamePreviewRequest, sub: &Subscription) -> Vec<RuleProbeFile> {
    if !req.sample_files.is_empty() {
        return req
            .sample_files
            .iter()
            .map(|file| RuleProbeFile {
                name: file.name.clone(),
                fid: file.fid.clone(),
                is_dir: file.is_dir,
                size: file.size,
                parent_path: file.parent_path.clone(),
                updated_at: file.updated_at.clone(),
            })
            .collect();
    }

    sub.last_probe
        .as_ref()
        .map(|probe| {
            probe
                .files
                .iter()
                .map(|file| RuleProbeFile {
                    name: file.name.clone(),
                    fid: file.file_key.clone(),
                    is_dir: false,
                    size: file.size,
                    parent_path: file.parent_path.clone(),
                    updated_at: file.updated_at.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 创建订阅路由
#[allow(clippy::too_many_arguments)]
pub fn routes(
    store: Arc<SubscriptionStore>,
    settings_store: Arc<SettingsStore>,
    check_service: Arc<SubscriptionCheckService>,
    transfer_service: Arc<SubscriptionTransferService>,
    job_queue: Arc<JobQueue>,
    job_store: Arc<JobStore>,
    notification_store: Arc<NotificationStore>,
    automation_event_store: Arc<AutomationEventStore>,
) -> Router {
    let state = Arc::new(SubscriptionState {
        store,
        settings_store,
        check_service,
        transfer_service,
        job_queue,
        job_store,
        notification_store,
        automation_event_store,
    });

    Router::new()
        .route("/api/subscriptions", get(list_subscriptions))
        .route("/api/subscriptions", post(create_subscription))
        .route("/api/subscriptions/check", post(check_all_subscriptions))
        .route(
            "/api/subscriptions/rename-preview",
            post(preview_subscription_rename),
        )
        .route(
            "/api/subscriptions/metadata/scrape",
            post(scrape_all_subscription_metadata),
        )
        .route("/api/subscriptions/{id}", get(get_subscription))
        .route(
            "/api/subscriptions/{id}/status",
            get(get_subscription_status),
        )
        .route("/api/subscriptions/{id}", put(update_subscription))
        .route("/api/subscriptions/{id}", delete(delete_subscription))
        .route("/api/subscriptions/{id}/check", post(check_subscription))
        .route(
            "/api/subscriptions/{id}/rename-existing",
            post(rename_existing_files),
        )
        .route(
            "/api/subscriptions/{id}/strm",
            post(generate_existing_strm_files),
        )
        .route(
            "/api/subscriptions/{id}/strm/audit",
            get(audit_existing_strm_files),
        )
        .route(
            "/api/subscriptions/{id}/metadata/scrape",
            post(scrape_subscription_metadata),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn subscription_for_source_change() -> Subscription {
        let mut sub: Subscription = serde_json::from_value(json!({
            "id": "sub1",
            "title": "Show",
            "media_type": "series",
            "season": 1,
            "url": "https://pan.quark.cn/s/old",
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1
        }))
        .unwrap();
        sub.status = "invalid".to_string();
        sub.invalid_since = Some(10);
        sub.last_error = "share expired".to_string();
        sub.current_episode_number = 12;
        sub.known_files = vec!["Show.S01E12.mkv".to_string()];
        sub.known_file_keys = vec!["fid12".to_string()];
        sub.known_episodes = vec![12];
        sub.transferred_files = vec!["Show.S01E12.mkv".to_string()];
        sub.transferred_file_keys = vec!["ep:12".to_string()];
        sub.last_new_files = vec!["Show.S01E12.mkv".to_string()];
        sub.last_new_episodes = vec![12];
        sub.last_check_summary = "链接失效".to_string();
        sub
    }

    #[test]
    fn apply_source_change_options_reactivates_and_continues_from_next_episode() {
        let mut sub = subscription_for_source_change();

        apply_source_change_options(&mut sub, true, true, true);

        assert_eq!(sub.status, "active");
        assert_eq!(sub.invalid_since, None);
        assert!(sub.last_error.is_empty());
        assert!(!sub.completed);
        assert_eq!(sub.current_episode_number, 12);
        assert_eq!(sub.start_episode_number, Some(13));
        assert_eq!(sub.known_episodes, vec![12]);
        assert_eq!(sub.transferred_file_keys, vec!["ep:12"]);
        assert!(sub.last_new_files.is_empty());
        assert_eq!(sub.last_check_summary, "已更换订阅资源，等待下次检查");
    }

    #[test]
    fn source_change_continue_from_current_defaults_to_enabled() {
        assert!(continue_from_current_episode_default(None));
        assert!(continue_from_current_episode_default(Some(true)));
        assert!(!continue_from_current_episode_default(Some(false)));
    }

    #[test]
    fn apply_source_change_options_can_reset_progress() {
        let mut sub = subscription_for_source_change();

        apply_source_change_options(&mut sub, true, false, true);

        assert_eq!(sub.status, "active");
        assert_eq!(sub.current_episode_number, 0);
        assert_eq!(sub.start_episode_number, None);
        assert!(sub.known_files.is_empty());
        assert!(sub.known_file_keys.is_empty());
        assert!(sub.known_episodes.is_empty());
        assert!(sub.transferred_files.is_empty());
        assert!(sub.transferred_file_keys.is_empty());
        assert!(sub.sync_downloads.is_empty());
    }

    #[test]
    fn content_change_resets_previous_season_progress() {
        let mut sub = subscription_for_source_change();
        sub.total_episode_number = Some(12);
        sub.sync_downloads = vec![crate::models::SyncDownloadRecord {
            gid: "gid-12".to_string(),
            file_name: "Show.S01E12.mkv".to_string(),
            download_dir: "/downloads".to_string(),
            target_dir: "/series/Show/Season 1".to_string(),
            submitted_at: 1,
            completed_at: Some(2),
        }];

        reset_progress_for_content_change(&mut sub);

        assert_eq!(sub.current_episode_number, 0);
        assert_eq!(sub.total_episode_number, None);
        assert_eq!(sub.start_episode_number, None);
        assert!(sub.known_files.is_empty());
        assert!(sub.known_episodes.is_empty());
        assert!(sub.transferred_files.is_empty());
        assert!(sub.sync_downloads.is_empty());
        assert!(sub.check_history.is_empty());
        assert!(!sub.completed);
        assert_eq!(sub.status, "active");
        assert_eq!(sub.last_checked_at, 0);
    }

    #[test]
    fn apply_source_change_options_ignores_unchanged_source() {
        let mut sub = subscription_for_source_change();

        apply_source_change_options(&mut sub, false, false, true);

        assert_eq!(sub.status, "invalid");
        assert_eq!(sub.current_episode_number, 12);
        assert_eq!(sub.known_episodes, vec![12]);
    }

    #[test]
    fn reconcile_completion_status_reopens_when_total_increased() {
        let mut sub = subscription_for_source_change();
        sub.status = "completed".to_string();
        sub.completed = true;
        sub.current_episode_number = 178;
        sub.known_episodes = vec![177, 178];
        sub.total_episode_number = Some(190);
        sub.invalid_since = Some(10);
        sub.last_error = "completed".to_string();

        reconcile_completion_status(&mut sub);

        assert_eq!(sub.status, "active");
        assert!(!sub.completed);
        assert_eq!(sub.invalid_since, None);
        assert!(sub.last_error.is_empty());
    }
}
