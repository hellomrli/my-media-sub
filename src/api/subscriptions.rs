use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::jobs::{JobQueue, MetadataScrapePayload};
use crate::models::{episode_count_for_season, MediaMetadata, Subscription, TransferRules};
use crate::services::subscription_check::CheckDetails;
use crate::services::subscription_progress::reopen_completed_subscription_status;
use crate::services::transfer_rule::{
    build_transfer_plan, summarize_rules, ProbeFile as RuleProbeFile,
};
use crate::services::{SubscriptionCheckService, SubscriptionTransferService};
use crate::store::{SettingsStore, SubscriptionStore};

/// 订阅路由状态
pub struct SubscriptionState {
    pub store: Arc<SubscriptionStore>,
    pub settings_store: Arc<SettingsStore>,
    pub check_service: Arc<SubscriptionCheckService>,
    pub transfer_service: Arc<SubscriptionTransferService>,
    pub job_queue: Arc<JobQueue>,
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
    pub rules: Option<TransferRules>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Option<MediaMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<TransferRules>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rename_template: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ScrapeMetadataRequest {
    #[serde(default)]
    pub overwrite: bool,
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
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CheckSubscriptionRequest {
    #[serde(default)]
    pub force_transfer: bool,
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
    items: Vec<RenamePreviewItem>,
}

#[derive(Serialize)]
struct RenamePreviewItem {
    source_name: String,
    target_name: String,
    action: String,
    skip_reason: String,
    episode: Option<i32>,
    season: Option<i32>,
    target_dir: String,
}

/// 通用响应
#[derive(Serialize)]
struct Response<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl<T> Response<T> {
    fn ok(data: T) -> Self {
        Self {
            data: Some(data),
            message: None,
        }
    }
}

fn create_rules(req: &CreateSubscriptionRequest) -> TransferRules {
    let mut rules = req.rules.clone().unwrap_or_default();
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
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
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
        metadata: base.and_then(|sub| sub.metadata.clone()),
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
        strm_enabled: base.map(|sub| sub.strm_enabled).unwrap_or(false),
        enabled: true,
        completed: false,
        rules,
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
    }
}

fn normalize_start_episode_number(value: Option<i32>, media_type: &str) -> Option<i32> {
    if media_type == "movie" {
        return None;
    }

    value.and_then(|episode| {
        let episode = episode.max(0);
        if episode > 0 {
            Some(episode)
        } else {
            None
        }
    })
}

fn apply_source_change_options(
    sub: &mut Subscription,
    source_changed: bool,
    keep_progress: bool,
    continue_from_current: bool,
) {
    if !source_changed {
        return;
    }

    sub.status = "active".to_string();
    sub.invalid_since = None;
    sub.last_error = String::new();
    sub.completed = false;
    sub.last_probe = None;
    sub.last_new_files.clear();
    sub.last_new_episodes.clear();
    sub.last_check_summary = "已更换订阅资源，等待下次检查".to_string();

    if !keep_progress {
        sub.current_episode_number = 0;
        sub.known_files.clear();
        sub.known_file_keys.clear();
        sub.known_episodes.clear();
        sub.transferred_files.clear();
        sub.transferred_file_keys.clear();
        sub.start_episode_number = None;
        return;
    }

    if continue_from_current && sub.media_type != "movie" && sub.current_episode_number > 0 {
        sub.start_episode_number = Some(sub.current_episode_number + 1);
    }
}

fn continue_from_current_episode_default(value: Option<bool>) -> bool {
    value.unwrap_or(true)
}

fn reconcile_completion_status(sub: &mut Subscription) {
    reopen_completed_subscription_status(sub);
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
                    updated_at: file.updated_at.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 列出所有订阅
async fn list_subscriptions(
    State(state): State<Arc<SubscriptionState>>,
) -> Result<Json<Response<Vec<Subscription>>>> {
    let subscriptions = state.store.list().await;
    Ok(Json(Response::ok(subscriptions)))
}

/// 获取单个订阅
async fn get_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    match state.store.get(&id).await {
        Some(sub) => Ok(Json(Response::ok(sub))),
        None => Err(AppError::NotFound("订阅不存在".to_string())),
    }
}

/// 创建订阅
async fn create_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Json(req): Json<CreateSubscriptionRequest>,
) -> Result<impl IntoResponse> {
    let rules = create_rules(&req);
    let rule_summary = summarize_rules(Some(&rules));
    let id = format!("{:x}", md5::compute(format!("{}:{}", req.url, req.title)));
    let id = &id[..12];

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let season = req.season.max(1);
    let media_type = if req.media_type.is_empty() {
        "series".to_string()
    } else {
        req.media_type
    };
    let start_episode_number =
        normalize_start_episode_number(req.start_episode_number, &media_type);
    let total_episode_number =
        episode_count_for_season(req.metadata.as_ref(), season).or(rules.finish_after_episode);

    let subscription = Subscription {
        id: id.to_string(),
        title: req.title,
        source_title: String::new(),
        media_type,
        season,
        start_episode_number,
        current_episode_number: 0,
        total_episode_number,
        source_group: String::new(),
        metadata: req.metadata,
        cloud_type: if req.cloud_type.is_empty() {
            "quark".to_string()
        } else {
            req.cloud_type
        },
        url: req.url,
        password: req.password,
        known_files: vec![],
        known_file_keys: vec![],
        known_episodes: vec![],
        transferred_files: vec![],
        transferred_file_keys: vec![],
        last_probe: None,
        last_plan_summary: String::new(),
        notify_only: req.notify_only,
        sync_download_enabled: req.sync_download_enabled,
        sync_download_dir: req.sync_download_dir,
        strm_enabled: req.strm_enabled,
        enabled: true,
        completed: false,
        rules,
        created_at: now,
        updated_at: now,
        last_checked_at: now,
        last_new_files: vec![],
        last_new_episodes: vec![],
        last_check_summary: String::new(),
        check_history: vec![],
        status: "active".to_string(),
        invalid_since: None,
        last_error: String::new(),
        rule_summary,
    };

    let created = state.store.create(subscription).await?;
    Ok((StatusCode::CREATED, Json(Response::ok(created))))
}

/// 更新订阅
async fn update_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSubscriptionRequest>,
) -> Result<impl IntoResponse> {
    let has_explicit_total_episode_number = req.total_episode_number.is_some();
    let keep_progress_on_source_change = req.keep_progress_on_source_change.unwrap_or(true);
    let continue_from_current_episode =
        continue_from_current_episode_default(req.continue_from_current_episode);
    let updated = state
        .store
        .update(&id, |sub| {
            let mut source_changed = false;
            if let Some(title) = req.title {
                sub.title = title;
            }
            if let Some(url) = req.url {
                source_changed |= url != sub.url;
                sub.url = url;
            }
            if let Some(password) = req.password {
                source_changed |= password != sub.password;
                sub.password = password;
            }
            if let Some(media_type) = req.media_type {
                sub.media_type = media_type;
            }
            if let Some(season) = req.season {
                sub.season = season.max(1);
            }
            if let Some(start_episode_number) = req.start_episode_number {
                sub.start_episode_number =
                    normalize_start_episode_number(Some(start_episode_number), &sub.media_type);
            }
            if sub.media_type == "movie" {
                sub.start_episode_number = None;
            }
            if let Some(cloud_type) = req.cloud_type {
                sub.cloud_type = cloud_type;
            }
            if let Some(enabled) = req.enabled {
                sub.enabled = enabled;
            }
            if let Some(notify_only) = req.notify_only {
                sub.notify_only = notify_only;
            }
            if let Some(sync_download_enabled) = req.sync_download_enabled {
                sub.sync_download_enabled = sync_download_enabled;
            }
            if let Some(sync_download_dir) = req.sync_download_dir {
                sub.sync_download_dir = sync_download_dir;
            }
            if let Some(strm_enabled) = req.strm_enabled {
                sub.strm_enabled = strm_enabled;
            }
            if let Some(total_episode_number) = req.total_episode_number {
                sub.total_episode_number = total_episode_number;
            }
            if let Some(metadata) = req.metadata {
                sub.metadata = metadata;
            }
            if let Some(rules) = req.rules {
                sub.rules = rules;
            }
            if let Some(target_dir) = req.target_dir {
                sub.rules.target_dir = target_dir;
            }
            if let Some(rename_template) = req.rename_template {
                sub.rules.rename_template = rename_template;
            }
            apply_source_change_options(
                sub,
                source_changed,
                keep_progress_on_source_change,
                continue_from_current_episode,
            );
            if !has_explicit_total_episode_number {
                if let Some(count) = episode_count_for_season(sub.metadata.as_ref(), sub.season) {
                    sub.total_episode_number = Some(count);
                } else if sub.total_episode_number.is_none() {
                    sub.total_episode_number = sub.rules.finish_after_episode;
                }
            }
            reconcile_completion_status(sub);
            sub.rule_summary = summarize_rules(Some(&sub.rules));
            sub.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
        })
        .await?;

    match updated {
        Some(sub) => Ok(Json(Response::ok(sub))),
        None => Err(AppError::NotFound("订阅不存在".to_string())),
    }
}

/// 删除订阅
async fn delete_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let deleted = state.store.delete(&id).await?;
    if deleted {
        Ok((StatusCode::NO_CONTENT, ()))
    } else {
        Err(AppError::NotFound("订阅不存在".to_string()))
    }
}

/// 检查响应
#[derive(Serialize)]
struct CheckResponse {
    subscription_id: String,
    new_files: Vec<String>,
    new_episodes: Vec<i32>,
    details: CheckDetails,
    became_invalid: bool,
    became_completed: bool,
    summary: String,
}

/// 重命名修复响应
#[derive(Serialize)]
struct RenameExistingResponse {
    subscription_id: String,
    renamed_count: usize,
}

/// STRM 生成响应
#[derive(Serialize)]
struct GenerateStrmResponse {
    subscription_id: String,
    generated_count: usize,
    skipped_count: usize,
    output_dir: String,
    files: Vec<GenerateStrmFile>,
}

#[derive(Serialize)]
struct GenerateStrmFile {
    fid: String,
    file_name: String,
    strm_path: String,
    url: String,
}

/// 检查单个订阅
async fn check_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
    body: Option<Json<CheckSubscriptionRequest>>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie;

    if cookie.is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }

    let force_transfer = body.map(|Json(req)| req.force_transfer).unwrap_or(false);
    let result = state
        .check_service
        .check_subscription_with_options(&id, &cookie, force_transfer)
        .await?;

    Ok(Json(Response::ok(CheckResponse {
        subscription_id: result.subscription_id,
        new_files: result.new_files,
        new_episodes: result.new_episodes,
        details: result.details,
        became_invalid: result.became_invalid,
        became_completed: result.became_completed,
        summary: result.summary,
    })))
}

/// 按订阅规则重命名目标目录中的已有文件
async fn rename_existing_files(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let renamed_count = state.transfer_service.rename_existing_files(&id).await?;

    Ok(Json(Response::ok(RenameExistingResponse {
        subscription_id: id,
        renamed_count,
    })))
}

/// 按订阅目标目录中的已有视频补齐 STRM 文件
async fn generate_existing_strm_files(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let result = state
        .transfer_service
        .generate_existing_strm_files(&id)
        .await?;

    Ok(Json(Response::ok(GenerateStrmResponse {
        subscription_id: id,
        generated_count: result.generated_count,
        skipped_count: result.skipped_count,
        output_dir: result.output_dir.display().to_string(),
        files: result
            .files
            .into_iter()
            .map(|file| GenerateStrmFile {
                fid: file.fid,
                file_name: file.file_name,
                strm_path: file.strm_path.display().to_string(),
                url: file.url,
            })
            .collect(),
    })))
}

/// 后台刮削单个订阅元数据
async fn scrape_subscription_metadata(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
    Json(req): Json<ScrapeMetadataRequest>,
) -> Result<impl IntoResponse> {
    if state.store.get(&id).await.is_none() {
        return Err(AppError::NotFound("订阅不存在".to_string()));
    }

    let job = state
        .job_queue
        .submit_metadata_scrape(MetadataScrapePayload {
            subscription_id: Some(id),
            overwrite: req.overwrite,
        })
        .await?;

    Ok((StatusCode::ACCEPTED, Json(Response::ok(job))))
}

/// 预览订阅规则产生的重命名和转存计划
async fn preview_subscription_rename(
    State(state): State<Arc<SubscriptionState>>,
    Json(req): Json<RenamePreviewRequest>,
) -> Result<impl IntoResponse> {
    let base = if let Some(id) = req.subscription_id.as_deref() {
        Some(
            state
                .store
                .get(id)
                .await
                .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?,
        )
    } else {
        None
    };
    let base_ref = base.as_ref();
    let sub = preview_subscription(&req, base_ref);
    let files = preview_files(&req, &sub);
    let plan = build_transfer_plan(&sub, Some(&files), None, None, None);
    let items = plan
        .items
        .into_iter()
        .map(|item| RenamePreviewItem {
            source_name: item.source_name,
            target_name: item.target_name,
            action: item.action,
            skip_reason: item.skip_reason,
            episode: item.episode,
            season: item.season,
            target_dir: item.target_dir,
        })
        .collect();

    Ok(Json(Response::ok(RenamePreviewResponse {
        summary: plan.summary,
        target_dir: plan.target_dir,
        transfer_count: plan.transfer_count,
        skip_count: plan.skip_count,
        matched_count: plan.matched_count,
        current_episode_number: plan.current_episode_number,
        episodes: plan.episodes,
        items,
    })))
}

/// 检查所有订阅
async fn check_all_subscriptions(
    State(state): State<Arc<SubscriptionState>>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie;

    if cookie.is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }

    let results = state.check_service.check_all_subscriptions(&cookie).await?;

    let responses: Vec<CheckResponse> = results
        .into_iter()
        .map(|r| CheckResponse {
            subscription_id: r.subscription_id,
            new_files: r.new_files,
            new_episodes: r.new_episodes,
            details: r.details,
            became_invalid: r.became_invalid,
            became_completed: r.became_completed,
            summary: r.summary,
        })
        .collect();

    Ok(Json(Response::ok(responses)))
}

/// 后台批量刮削订阅元数据
async fn scrape_all_subscription_metadata(
    State(state): State<Arc<SubscriptionState>>,
    Json(req): Json<ScrapeMetadataRequest>,
) -> Result<impl IntoResponse> {
    let job = state
        .job_queue
        .submit_metadata_scrape(MetadataScrapePayload {
            subscription_id: None,
            overwrite: req.overwrite,
        })
        .await?;

    Ok((StatusCode::ACCEPTED, Json(Response::ok(job))))
}

/// 创建订阅路由
pub fn routes(
    store: Arc<SubscriptionStore>,
    settings_store: Arc<SettingsStore>,
    check_service: Arc<SubscriptionCheckService>,
    transfer_service: Arc<SubscriptionTransferService>,
    job_queue: Arc<JobQueue>,
) -> Router {
    let state = Arc::new(SubscriptionState {
        store,
        settings_store,
        check_service,
        transfer_service,
        job_queue,
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
