use axum::{
    extract::{Path as AxumPath, Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::warn;

use crate::clients::aria2::{Aria2Task, Aria2Version};
use crate::clients::{Aria2Client, NormalizedItem, QuarkSaveClient, QuarkSigninResult};
use crate::error::{AppError, Result};
use crate::jobs::JobQueue;
use crate::models::{Notification, Settings, Subscription};
use crate::services::notification::{
    add_notification, dispatch_push_event_for_notification, PushDispatchRequest,
};
use crate::services::push::{PushEvent, PushLevel};
use crate::services::quark_signin::signin_message;
use crate::services::subscription_progress::{
    completion_target_episode, should_mark_completed_from_file_names,
};
use crate::services::QuarkSigninService;
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};
use crate::utils::unix_now;

/// 网盘状态
pub struct DriveState {
    pub settings_store: Arc<SettingsStore>,
    pub subscription_store: Arc<SubscriptionStore>,
    pub notification_store: Arc<NotificationStore>,
    pub job_queue: Arc<JobQueue>,
    pub quark_signin_service: Arc<QuarkSigninService>,
    pub drive_cache: RwLock<HashMap<String, CachedDriveList>>,
    pub notified_completed_downloads: RwLock<HashSet<String>>,
}

#[derive(Clone)]
pub struct CachedDriveList {
    pub created_at: Instant,
    pub items: Vec<NormalizedItem>,
}

const DRIVE_CACHE_TTL: Duration = Duration::from_secs(20);

/// 列出目录请求
#[derive(Debug, Deserialize)]
pub struct ListRequest {
    pub path: Option<String>,
    pub fid: Option<String>,
    #[serde(default)]
    pub refresh: bool,
}

/// 查找路径请求
#[derive(Debug, Deserialize)]
pub struct FindPathRequest {
    pub path: String,
}

/// 查找路径响应
#[derive(Serialize)]
pub struct FindPathResponse {
    pub fid: String,
    pub found: bool,
}

/// 列出目录响应
#[derive(Serialize)]
pub struct ListResponse {
    pub list: Vec<NormalizedItem>,
}

/// 创建文件夹请求
#[derive(Debug, Deserialize)]
pub struct MkdirRequest {
    #[serde(default)]
    pub parent_fid: String,
    #[serde(default)]
    pub path: String,
    pub name: String,
}

/// 删除文件请求
#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    #[serde(default)]
    pub fid: String,
    #[serde(default)]
    pub fids: Vec<String>,
}

/// 重命名文件请求
#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub fid: String,
    pub name: String,
    #[serde(default)]
    pub parent_fid: String,
}

#[derive(Debug, Deserialize)]
pub struct Aria2DownloadRequest {
    #[serde(default)]
    pub fid: String,
    #[serde(default)]
    pub fids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Aria2TasksRequest {
    #[serde(default = "default_stopped_limit")]
    pub stopped_limit: u64,
}

#[derive(Debug, Deserialize)]
pub struct Aria2BrowseRequest {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub media_type: String,
}

/// 通用操作响应
#[derive(Serialize)]
pub struct ActionResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fid: Option<String>,
}

#[derive(Serialize)]
pub struct Aria2DownloadItem {
    pub fid: String,
    pub file_name: String,
    pub size: i64,
    pub gid: String,
}

#[derive(Serialize)]
pub struct Aria2DownloadResponse {
    pub success: bool,
    pub count: usize,
    pub message: String,
    pub items: Vec<Aria2DownloadItem>,
}

#[derive(Serialize)]
pub struct Aria2TasksResponse {
    pub success: bool,
    pub active: Vec<Aria2Task>,
    pub waiting: Vec<Aria2Task>,
    pub stopped: Vec<Aria2Task>,
}

#[derive(Serialize)]
pub struct Aria2TaskActionResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gid: Option<String>,
    pub affected_count: usize,
}

#[derive(Serialize)]
pub struct Aria2DirectoryItem {
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct Aria2BrowseResponse {
    pub success: bool,
    pub root: String,
    pub current: String,
    pub parent: Option<String>,
    pub items: Vec<Aria2DirectoryItem>,
}

#[derive(Serialize)]
pub struct Aria2TestResponse {
    pub success: bool,
    pub message: String,
    pub version: String,
    pub enabled_features: Vec<String>,
}

/// 测试夸克连接
#[derive(Debug, Deserialize)]
pub struct TestRequest {
    #[serde(default)]
    pub cookie: String,
}

/// 测试响应
#[derive(Serialize)]
pub struct TestResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub cookie_configured: bool,
    pub save_enabled: bool,
    pub signin_enabled: bool,
    pub signin_cookie_configured: bool,
    pub root_configured: bool,
    pub strm_enabled: bool,
    pub strm_ready: bool,
    pub directories: HashMap<String, String>,
    pub issues: Vec<String>,
    pub total_capacity_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_capacity_bytes: Option<i64>,
    pub member_type: String,
    pub sign_progress: i64,
    pub sign_target: i64,
}

#[derive(Serialize)]
pub struct QuarkSigninResponse {
    pub success: bool,
    pub message: String,
    pub result: QuarkSigninResult,
}

/// 列出目录
async fn list_drive(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<ListRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie.clone();

    if cookie.is_empty() {
        return Ok(Json(ListResponse { list: vec![] }));
    }

    // 优先使用 fid，如果没有则使用 path
    let fid = if let Some(f) = req.fid {
        f
    } else {
        let path = req.path.unwrap_or_else(|| "/".to_string());
        if path == "/" || path.is_empty() {
            "0".to_string()
        } else {
            // 暂时只支持根目录
            "0".to_string()
        }
    };
    let cache_key = drive_cache_key(&cookie, &fid);
    if !req.refresh {
        if let Some(items) = cached_drive_items(&state, &cache_key).await {
            return Ok(Json(ListResponse { list: items }));
        }
    }

    let client = QuarkSaveClient::new(cookie);

    match client.list_dir(&fid).await {
        Ok(items) => {
            cache_drive_items(&state, cache_key, items.clone()).await;
            Ok(Json(ListResponse { list: items }))
        }
        Err(e) => {
            tracing::error!("列出目录失败: {}", e);
            Ok(Json(ListResponse { list: vec![] }))
        }
    }
}

async fn cached_drive_items(state: &DriveState, key: &str) -> Option<Vec<NormalizedItem>> {
    let cache = state.drive_cache.read().await;
    let cached = cache.get(key)?;
    if cached.created_at.elapsed() > DRIVE_CACHE_TTL {
        return None;
    }
    Some(cached.items.clone())
}

async fn cache_drive_items(state: &DriveState, key: String, items: Vec<NormalizedItem>) {
    let mut cache = state.drive_cache.write().await;
    cache.insert(
        key,
        CachedDriveList {
            created_at: Instant::now(),
            items,
        },
    );
}

async fn clear_drive_cache(state: &DriveState) {
    state.drive_cache.write().await.clear();
}

fn drive_cache_key(cookie: &str, fid: &str) -> String {
    let mut hasher = DefaultHasher::new();
    cookie.hash(&mut hasher);
    format!("{}:{}", hasher.finish(), fid.trim())
}

/// 测试夸克连接
async fn test_quark(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<TestRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let mut health = quark_health_snapshot(&settings);
    let request_cookie = req.cookie.trim().to_string();
    let cookie = if request_cookie.is_empty() {
        settings.quark_cookie.clone()
    } else {
        request_cookie
    };
    let capacity_cookie = if !settings.quark_signin_cookie.trim().is_empty() {
        settings.quark_signin_cookie.clone()
    } else {
        cookie.clone()
    };

    if cookie.trim().is_empty() {
        return Ok(Json(TestResponse {
            success: false,
            nickname: None,
            error: Some("未配置夸克 Cookie".to_string()),
            ..health
        }));
    }

    let client = QuarkSaveClient::new(cookie);
    if !capacity_cookie.trim().is_empty() {
        let capacity_client = QuarkSaveClient::new(capacity_cookie);
        match capacity_client.growth_info().await {
            Ok(info) => {
                health.total_capacity_bytes = info.total_capacity_bytes;
                health.used_capacity_bytes = info.used_capacity_bytes;
                health.member_type = info.member_type;
                health.sign_progress = info.sign_progress;
                health.sign_target = info.sign_target;
            }
            Err(err) => {
                health.issues.push(format!("容量读取失败: {}", err));
            }
        }
    }
    match client.storage_usage().await {
        Ok(usage) => {
            if let Some(total) = usage.total_capacity_bytes {
                health.total_capacity_bytes = total;
            }
            if usage.used_capacity_bytes.is_some() {
                health.used_capacity_bytes = usage.used_capacity_bytes;
            }
        }
        Err(err) => {
            tracing::debug!("读取夸克容量使用量失败: {}", err);
        }
    }

    match client.list_dir("0").await {
        Ok(_) => Ok(Json(TestResponse {
            success: true,
            nickname: Some("夸克用户".to_string()),
            error: None,
            ..health
        })),
        Err(e) => Ok(Json(TestResponse {
            success: false,
            nickname: None,
            error: Some(format!("连接失败: {}", e)),
            ..health
        })),
    }
}

fn quark_health_snapshot(settings: &Settings) -> TestResponse {
    let mut directories = HashMap::new();
    directories.insert("movie".to_string(), settings.quark_save_movie_dir.clone());
    directories.insert("series".to_string(), settings.quark_save_series_dir.clone());
    directories.insert("anime".to_string(), settings.quark_save_anime_dir.clone());

    let cookie_configured = !settings.quark_cookie.trim().is_empty();
    let strm_ready = !settings.strm_output_dir.trim().is_empty()
        && !settings.strm_public_base_url.trim().is_empty();
    let mut issues = Vec::new();
    if !cookie_configured {
        issues.push("未配置夸克 Cookie".to_string());
    }
    if settings.strm_enabled && !strm_ready {
        issues.push("已启用 STRM，但输出目录或访问地址未配置完整".to_string());
    }

    TestResponse {
        success: false,
        nickname: None,
        error: None,
        cookie_configured,
        save_enabled: settings.quark_save_enabled,
        signin_enabled: settings.quark_signin_enabled,
        signin_cookie_configured: !settings.quark_signin_cookie.trim().is_empty(),
        root_configured: true,
        strm_enabled: settings.strm_enabled,
        strm_ready,
        directories,
        issues,
        total_capacity_bytes: 0,
        used_capacity_bytes: None,
        member_type: String::new(),
        sign_progress: 0,
        sign_target: 0,
    }
}

async fn quark_signin(State(state): State<Arc<DriveState>>) -> Result<Json<QuarkSigninResponse>> {
    let result = state
        .quark_signin_service
        .signin_with_failure_notice()
        .await?;
    let message = signin_message(&result);
    Ok(Json(QuarkSigninResponse {
        success: true,
        message,
        result,
    }))
}

async fn drive_client(state: &DriveState) -> Result<QuarkSaveClient> {
    let cookie = state.settings_store.get().await.quark_cookie;
    if cookie.trim().is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }
    Ok(QuarkSaveClient::new(cookie))
}

/// 创建文件夹
async fn mkdir(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<MkdirRequest>,
) -> Result<Json<ActionResponse>> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("文件夹名称不能为空".to_string()));
    }

    let client = drive_client(&state).await?;
    let parent_fid = if !req.parent_fid.trim().is_empty() {
        req.parent_fid
    } else if req.path.trim().is_empty() || req.path.trim() == "/" {
        "0".to_string()
    } else {
        client.ensure_dir_path(&req.path).await?
    };

    let fid = client.create_dir(&parent_fid, name).await?;
    clear_drive_cache(&state).await;
    Ok(Json(ActionResponse {
        success: true,
        message: Some("创建成功".to_string()),
        fid: Some(fid),
    }))
}

/// 删除文件/文件夹
async fn delete_items(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<DeleteRequest>,
) -> Result<Json<ActionResponse>> {
    let mut fids = req.fids;
    if !req.fid.trim().is_empty() {
        fids.push(req.fid);
    }
    fids.retain(|fid| !fid.trim().is_empty());
    fids.sort();
    fids.dedup();

    if fids.is_empty() {
        return Err(AppError::Validation("未选择要删除的项目".to_string()));
    }

    let client = drive_client(&state).await?;
    client.delete_items(&fids).await?;
    clear_drive_cache(&state).await;
    Ok(Json(ActionResponse {
        success: true,
        message: Some(format!("已删除 {} 项", fids.len())),
        fid: None,
    }))
}

/// 重命名文件/文件夹
async fn rename_item(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<RenameRequest>,
) -> Result<Json<ActionResponse>> {
    if req.fid.trim().is_empty() {
        return Err(AppError::Validation("缺少文件 ID".to_string()));
    }
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("名称不能为空".to_string()));
    }

    let client = drive_client(&state).await?;
    let parent_fid = req.parent_fid.trim();
    client
        .rename_item(
            &req.fid,
            name,
            if parent_fid.is_empty() {
                None
            } else {
                Some(parent_fid)
            },
        )
        .await?;
    clear_drive_cache(&state).await;
    Ok(Json(ActionResponse {
        success: true,
        message: Some("重命名成功".to_string()),
        fid: Some(req.fid),
    }))
}

/// 发送夸克网盘文件到 Aria2
async fn send_to_aria2(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<Aria2DownloadRequest>,
) -> Result<Json<Aria2DownloadResponse>> {
    let mut fids = req.fids;
    if !req.fid.trim().is_empty() {
        fids.push(req.fid);
    }
    fids = normalize_fids(fids);
    if fids.is_empty() {
        return Err(AppError::Validation("未选择要下载的文件".to_string()));
    }

    let settings = state.settings_store.get().await;
    if settings.quark_cookie.trim().is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }
    if settings.aria2_rpc_url.trim().is_empty() {
        return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
    }

    let quark = QuarkSaveClient::new(settings.quark_cookie);
    let aria2 = Aria2Client::new(settings.aria2_rpc_url, settings.aria2_secret, String::new());
    let download_infos = quark.download_infos(&fids).await?;
    let mut items = Vec::with_capacity(download_infos.len());

    for info in download_infos {
        let gid = aria2
            .add_uri(&info.download_url, Some(&info.file_name), &info.headers)
            .await?;
        items.push(Aria2DownloadItem {
            fid: info.fid,
            file_name: info.file_name,
            size: info.size,
            gid,
        });
    }

    Ok(Json(Aria2DownloadResponse {
        success: true,
        count: items.len(),
        message: format!("已提交 {} 个 Aria2 下载任务", items.len()),
        items,
    }))
}

/// 获取 Aria2 下载任务状态
async fn list_aria2_tasks(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<Aria2TasksRequest>,
) -> Result<Json<Aria2TasksResponse>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let tasks = aria2.list_tasks(req.stopped_limit.clamp(1, 50)).await?;
    notify_completed_downloads(&state, &tasks.stopped).await;

    Ok(Json(Aria2TasksResponse {
        success: true,
        active: tasks.active,
        waiting: tasks.waiting,
        stopped: tasks.stopped,
    }))
}

async fn notify_completed_downloads(state: &DriveState, tasks: &[Aria2Task]) {
    let history = state.notification_store.list(true).await;
    let pushed_downloads = state
        .job_queue
        .successful_push_dispatch_messages(PushEvent::DownloadCompleted.as_str())
        .await;
    let known_keys = state.notified_completed_downloads.read().await.clone();
    let pending_tasks = tasks
        .iter()
        .filter(|task| task.status == "complete")
        .filter(|task| !task.gid.trim().is_empty())
        .filter(|task| {
            download_completed_dedupe_keys(task)
                .iter()
                .all(|key| !known_keys.contains(key))
        })
        .filter(|task| !completed_download_already_recorded(&history, &pushed_downloads, task))
        .collect::<Vec<_>>();

    if pending_tasks.is_empty() {
        return;
    }

    let mut inserted_gids = Vec::new();
    {
        let mut known = state.notified_completed_downloads.write().await;
        for task in pending_tasks {
            let keys = download_completed_dedupe_keys(task);
            if keys.iter().any(|key| known.contains(key)) {
                continue;
            }
            for key in keys {
                known.insert(key);
            }
            inserted_gids.push(task.gid.clone());
        }
    }

    for gid in inserted_gids {
        if let Some(task) = tasks.iter().find(|task| task.gid == gid) {
            if let Err(e) = notify_completed_download(state, task).await {
                warn!("记录 Aria2 下载完成通知失败 {}: {}", task.gid, e);
            }
        }
    }
}

async fn notify_completed_download(state: &DriveState, task: &Aria2Task) -> Result<()> {
    let (title, message) = download_completed_title_message(task);
    let meta = download_completed_meta(task);
    let notification = add_notification(
        &state.notification_store,
        "success",
        PushEvent::DownloadCompleted.as_str(),
        title.clone(),
        message.clone(),
        meta,
    )
    .await?;
    dispatch_push_event_for_notification(
        state.settings_store.clone(),
        state.notification_store.clone(),
        Some(state.job_queue.clone()),
        PushDispatchRequest {
            notification_id: Some(notification.id),
            event: PushEvent::DownloadCompleted,
            title,
            message,
            level: PushLevel::Success,
        },
    )
    .await;

    if let Err(e) = complete_subscription_for_download(state, task).await {
        warn!("根据下载完成更新订阅状态失败 {}: {}", task.gid, e);
    }

    Ok(())
}

fn download_completed_title_message(task: &Aria2Task) -> (String, String) {
    let file_name = if task.file_name.trim().is_empty() {
        task.gid.as_str()
    } else {
        task.file_name.trim()
    };
    let title = format!("下载完成: {}", file_name);
    let mut parts = vec![format!("文件：{}", file_name)];
    if !task.dir.trim().is_empty() {
        parts.push(format!("目录：{}", task.dir.trim()));
    }
    if task.total_length > 0 {
        parts.push(format!("大小：{}", format_bytes(task.total_length)));
    }
    let message = parts.join("\n");
    (title, message)
}

fn download_completed_meta(task: &Aria2Task) -> HashMap<String, Value> {
    HashMap::from([
        ("gid".to_string(), json!(task.gid)),
        ("file_name".to_string(), json!(task.file_name)),
        ("dir".to_string(), json!(task.dir)),
        ("total_length".to_string(), json!(task.total_length)),
        ("completed_length".to_string(), json!(task.completed_length)),
    ])
}

fn download_completed_dedupe_keys(task: &Aria2Task) -> Vec<String> {
    let mut keys = Vec::with_capacity(2);
    let gid = task.gid.trim();
    if !gid.is_empty() {
        keys.push(format!("gid:{}", gid));
    }
    keys.push(format!(
        "file:{}\n{}\n{}",
        task.file_name.trim(),
        task.dir.trim(),
        task.total_length
    ));
    keys
}

fn completed_download_already_recorded(
    history: &[Notification],
    pushed_downloads: &HashSet<(String, String)>,
    task: &Aria2Task,
) -> bool {
    let (title, message) = download_completed_title_message(task);
    pushed_downloads.contains(&(title.clone(), message.clone()))
        || history.iter().any(|notification| {
            notification_matches_completed_download(notification, task, &title, &message)
        })
}

fn notification_matches_completed_download(
    notification: &Notification,
    task: &Aria2Task,
    title: &str,
    message: &str,
) -> bool {
    if notification.event != PushEvent::DownloadCompleted.as_str() {
        return false;
    }
    if notification.meta.get("gid").and_then(Value::as_str) == Some(task.gid.as_str()) {
        return true;
    }
    if notification.title == title && notification.message == message {
        return true;
    }
    let same_file =
        notification.meta.get("file_name").and_then(Value::as_str) == Some(task.file_name.as_str());
    let same_dir = notification.meta.get("dir").and_then(Value::as_str) == Some(task.dir.as_str());
    let same_size = notification
        .meta
        .get("total_length")
        .and_then(Value::as_u64)
        == Some(task.total_length);
    same_file && same_dir && same_size
}

async fn complete_subscription_for_download(state: &DriveState, task: &Aria2Task) -> Result<()> {
    let history = state.notification_store.list(true).await;
    let gid = task.gid.trim();
    if gid.is_empty() {
        return Ok(());
    }

    let completed_gids = download_completed_gids(&history, gid);
    let Some(subscription_id) = subscription_id_for_download_gid(&history, gid) else {
        return Ok(());
    };
    let Some(sub) = state.subscription_store.get(&subscription_id).await else {
        return Ok(());
    };
    if sub.completed || !sub.sync_download_enabled {
        return Ok(());
    }

    let completed_files =
        completed_subscription_download_files(&history, &subscription_id, &completed_gids);
    if !should_mark_completed_from_file_names(&sub, &completed_files) {
        return Ok(());
    }

    mark_subscription_completed_after_download(state, &sub, &completed_files).await?;
    Ok(())
}

async fn mark_subscription_completed_after_download(
    state: &DriveState,
    sub: &Subscription,
    completed_files: &[String],
) -> Result<bool> {
    let target_episode = completion_target_episode(sub);
    let now = now_ts();
    let updated = state
        .subscription_store
        .update(&sub.id, |sub| {
            if sub.completed {
                return;
            }
            sub.completed = true;
            sub.status = "completed".to_string();
            sub.invalid_since = None;
            sub.last_error = String::new();
            if let Some(target_episode) = target_episode {
                sub.current_episode_number = sub.current_episode_number.max(target_episode);
            }
            if sub.total_episode_number.is_none() {
                sub.total_episode_number = sub.rules.finish_after_episode;
            }
            sub.updated_at = now;
        })
        .await?
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

    if sub.completed || !updated.completed {
        return Ok(false);
    }

    let total = completion_target_episode(&updated).unwrap_or(updated.current_episode_number);
    let title = format!("订阅已完结: {}", updated.title);
    let message = if total > 0 {
        format!("已下载到第 {} 集", total)
    } else {
        "订阅已标记为完结".to_string()
    };
    let meta: HashMap<String, Value> = HashMap::from([
        ("subscription_id".to_string(), json!(updated.id)),
        ("subscription_title".to_string(), json!(updated.title)),
        (
            "completed_download_files".to_string(),
            json!(completed_files),
        ),
    ]);

    let notification = add_notification(
        &state.notification_store,
        "success",
        PushEvent::SubscriptionCompleted.as_str(),
        title.clone(),
        message.clone(),
        meta,
    )
    .await?;
    dispatch_push_event_for_notification(
        state.settings_store.clone(),
        state.notification_store.clone(),
        Some(state.job_queue.clone()),
        PushDispatchRequest {
            notification_id: Some(notification.id),
            event: PushEvent::SubscriptionCompleted,
            title,
            message,
            level: PushLevel::Success,
        },
    )
    .await;

    Ok(true)
}

fn subscription_id_for_download_gid(history: &[Notification], gid: &str) -> Option<String> {
    history
        .iter()
        .filter(|notification| notification.event == "subscription_transferred")
        .find_map(|notification| {
            let downloads = notification.meta.get("sync_downloads")?.as_array()?;
            let matched = downloads
                .iter()
                .any(|item| item.get("gid").and_then(Value::as_str) == Some(gid));
            if !matched {
                return None;
            }
            notification
                .meta
                .get("subscription_id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
}

fn download_completed_gids(history: &[Notification], current_gid: &str) -> HashSet<String> {
    let mut gids = history
        .iter()
        .filter(|notification| notification.event == "download_completed")
        .filter_map(|notification| notification.meta.get("gid").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<HashSet<_>>();
    gids.insert(current_gid.to_string());
    gids
}

fn completed_subscription_download_files(
    history: &[Notification],
    subscription_id: &str,
    completed_gids: &HashSet<String>,
) -> Vec<String> {
    let mut files = history
        .iter()
        .filter(|notification| notification.event == "subscription_transferred")
        .filter(|notification| {
            notification
                .meta
                .get("subscription_id")
                .and_then(Value::as_str)
                == Some(subscription_id)
        })
        .filter_map(|notification| notification.meta.get("sync_downloads")?.as_array())
        .flat_map(|downloads| downloads.iter())
        .filter(|item| {
            item.get("gid")
                .and_then(Value::as_str)
                .map(|gid| completed_gids.contains(gid))
                .unwrap_or(false)
        })
        .filter_map(|item| item.get("file_name").and_then(Value::as_str))
        .filter(|file_name| !file_name.trim().is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

fn now_ts() -> i64 {
    unix_now()
}

fn aria2_client(settings: &Settings) -> Result<Aria2Client> {
    if settings.aria2_rpc_url.trim().is_empty() {
        return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
    }

    Ok(Aria2Client::new(
        settings.aria2_rpc_url.clone(),
        settings.aria2_secret.clone(),
        String::new(),
    ))
}

async fn pause_aria2_task(
    State(state): State<Arc<DriveState>>,
    AxumPath(gid): AxumPath<String>,
) -> Result<Json<Aria2TaskActionResponse>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let gid = aria2.pause(&gid).await?;
    Ok(Json(Aria2TaskActionResponse {
        success: true,
        message: "已暂停下载任务".to_string(),
        gid: Some(gid),
        affected_count: 1,
    }))
}

async fn resume_aria2_task(
    State(state): State<Arc<DriveState>>,
    AxumPath(gid): AxumPath<String>,
) -> Result<Json<Aria2TaskActionResponse>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let gid = aria2.unpause(&gid).await?;
    Ok(Json(Aria2TaskActionResponse {
        success: true,
        message: "已继续下载任务".to_string(),
        gid: Some(gid),
        affected_count: 1,
    }))
}

async fn stop_aria2_task(
    State(state): State<Arc<DriveState>>,
    AxumPath(gid): AxumPath<String>,
) -> Result<Json<Aria2TaskActionResponse>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let gid = aria2.force_remove(&gid).await?;
    Ok(Json(Aria2TaskActionResponse {
        success: true,
        message: "已停止下载任务".to_string(),
        gid: Some(gid),
        affected_count: 1,
    }))
}

async fn delete_aria2_task(
    State(state): State<Arc<DriveState>>,
    AxumPath(gid): AxumPath<String>,
) -> Result<Json<Aria2TaskActionResponse>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let gid = gid.trim().to_string();
    if gid.is_empty() {
        return Err(AppError::Validation("Aria2 任务 GID 为空".to_string()));
    }

    if aria2.remove_download_result(&gid).await.is_err() {
        aria2.force_remove(&gid).await?;
        let _ = aria2.remove_download_result(&gid).await;
    }

    Ok(Json(Aria2TaskActionResponse {
        success: true,
        message: "已删除下载任务记录".to_string(),
        gid: Some(gid),
        affected_count: 1,
    }))
}

async fn pause_all_aria2_tasks(
    State(state): State<Arc<DriveState>>,
) -> Result<Json<Aria2TaskActionResponse>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    aria2.pause_all().await?;
    Ok(Json(Aria2TaskActionResponse {
        success: true,
        message: "已暂停全部下载任务".to_string(),
        gid: None,
        affected_count: 0,
    }))
}

async fn stop_all_aria2_tasks(
    State(state): State<Arc<DriveState>>,
) -> Result<Json<Aria2TaskActionResponse>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let tasks = aria2.list_tasks(1).await?;
    let gids: Vec<String> = tasks
        .active
        .into_iter()
        .chain(tasks.waiting)
        .map(|task| task.gid)
        .filter(|gid| !gid.trim().is_empty())
        .collect();

    let mut affected_count = 0usize;
    for gid in gids {
        aria2.force_remove(&gid).await?;
        affected_count += 1;
    }

    Ok(Json(Aria2TaskActionResponse {
        success: true,
        message: format!("已停止 {} 个下载任务", affected_count),
        gid: None,
        affected_count,
    }))
}

/// 测试 Aria2 RPC 连接
async fn test_aria2(State(state): State<Arc<DriveState>>) -> Result<Json<Aria2TestResponse>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let Aria2Version {
        version,
        enabled_features,
    } = aria2.get_version().await?;

    Ok(Json(Aria2TestResponse {
        success: true,
        message: format!("Aria2 连接成功，版本 {}", version),
        version,
        enabled_features,
    }))
}

/// 浏览指定媒体类型 Aria2 下载目录下的文件夹。
async fn browse_aria2_dir(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<Aria2BrowseRequest>,
) -> Result<Json<Aria2BrowseResponse>> {
    let settings = state.settings_store.get().await;
    let root = aria2_browse_root(&settings, req.media_type.trim());
    if root.is_empty() {
        return Err(AppError::Validation(
            "未配置当前媒体类型的 Aria2 下载目录".to_string(),
        ));
    }

    let root = canonical_dir(root)?;
    let requested = if req.path.trim().is_empty() {
        root.clone()
    } else {
        canonical_dir(req.path.trim())?
    };
    if !requested.starts_with(&root) {
        return Err(AppError::Validation(
            "只能浏览当前媒体类型 Aria2 下载目录下的路径".to_string(),
        ));
    }

    let mut items = Vec::new();
    for entry in std::fs::read_dir(&requested)
        .map_err(|e| AppError::Internal(format!("读取目录失败: {}", e)))?
    {
        let entry = entry.map_err(|e| AppError::Internal(format!("读取目录项失败: {}", e)))?;
        let file_type = entry
            .file_type()
            .map_err(|e| AppError::Internal(format!("读取目录项类型失败: {}", e)))?;
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        let canonical = match path.canonicalize() {
            Ok(path) if path.starts_with(&root) => path,
            _ => continue,
        };
        items.push(Aria2DirectoryItem {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: canonical.display().to_string(),
        });
    }
    items.sort_by(|left, right| left.name.cmp(&right.name));

    let parent = requested
        .parent()
        .filter(|parent| requested != root && parent.starts_with(&root))
        .map(|parent| parent.display().to_string());

    Ok(Json(Aria2BrowseResponse {
        success: true,
        root: root.display().to_string(),
        current: requested.display().to_string(),
        parent,
        items,
    }))
}

fn aria2_browse_root(settings: &Settings, media_type: &str) -> String {
    match media_type {
        "movie" => settings.aria2_movie_dir.trim().to_string(),
        "series" => settings.aria2_series_dir.trim().to_string(),
        "anime" => settings.aria2_anime_dir.trim().to_string(),
        media_type if media_type.starts_with("custom_") => {
            let id = media_type.trim_start_matches("custom_");
            settings
                .custom_categories
                .iter()
                .find(|category| category.id == id)
                .map(|category| category.aria2_dir.trim().to_string())
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

fn canonical_dir(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path
        .as_ref()
        .canonicalize()
        .map_err(|e| AppError::Validation(format!("目录不存在或不可访问: {}", e)))?;
    if !path.is_dir() {
        return Err(AppError::Validation("路径不是目录".to_string()));
    }
    Ok(path)
}

fn normalize_fids(fids: Vec<String>) -> Vec<String> {
    let mut fids: Vec<String> = fids
        .into_iter()
        .map(|fid| fid.trim().to_string())
        .filter(|fid| !fid.is_empty())
        .collect();
    fids.sort();
    fids.dedup();
    fids
}

/// 根据路径查找目录 fid
async fn find_path(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<FindPathRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie.clone();

    if cookie.is_empty() {
        return Ok(Json(FindPathResponse {
            fid: "0".to_string(),
            found: false,
        }));
    }

    let client = QuarkSaveClient::new(cookie);

    // 使用 ensure_dir_path 查找或创建路径
    match client.ensure_dir_path(&req.path).await {
        Ok(fid) => {
            clear_drive_cache(&state).await;
            Ok(Json(FindPathResponse { fid, found: true }))
        }
        Err(e) => {
            tracing::warn!("查找路径 {} 失败: {}", req.path, e);
            Ok(Json(FindPathResponse {
                fid: "0".to_string(),
                found: false,
            }))
        }
    }
}

/// 创建网盘路由
pub fn routes(
    settings_store: Arc<SettingsStore>,
    subscription_store: Arc<SubscriptionStore>,
    notification_store: Arc<NotificationStore>,
    job_queue: Arc<JobQueue>,
    quark_signin_service: Arc<QuarkSigninService>,
) -> Router {
    let state = Arc::new(DriveState {
        settings_store,
        subscription_store,
        notification_store,
        job_queue,
        quark_signin_service,
        drive_cache: RwLock::new(HashMap::new()),
        notified_completed_downloads: RwLock::new(HashSet::new()),
    });

    Router::new()
        .route("/api/drive", get(list_drive))
        .route("/api/drive/find-path", get(find_path))
        .route("/api/drive/mkdir", post(mkdir))
        .route("/api/drive/delete", post(delete_items))
        .route("/api/drive/rename", post(rename_item))
        .route("/api/drive/aria2", post(send_to_aria2))
        .route("/api/drive/aria2/tasks", get(list_aria2_tasks))
        .route(
            "/api/drive/aria2/tasks/pause-all",
            post(pause_all_aria2_tasks),
        )
        .route(
            "/api/drive/aria2/tasks/stop-all",
            post(stop_all_aria2_tasks),
        )
        .route("/api/drive/aria2/tasks/{gid}/pause", post(pause_aria2_task))
        .route(
            "/api/drive/aria2/tasks/{gid}/resume",
            post(resume_aria2_task),
        )
        .route("/api/drive/aria2/tasks/{gid}/stop", post(stop_aria2_task))
        .route(
            "/api/drive/aria2/tasks/{gid}/delete",
            post(delete_aria2_task),
        )
        .route("/api/drive/aria2/test", get(test_aria2))
        .route("/api/drive/aria2/browse", get(browse_aria2_dir))
        .route("/api/quark/test", post(test_quark))
        .route("/api/quark/signin", post(quark_signin))
        .with_state(state)
}

fn default_stopped_limit() -> u64 {
    10
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Notification;

    #[test]
    fn test_normalize_fids_trims_and_dedups() {
        let fids = normalize_fids(vec![" a ".to_string(), "".to_string(), "a".to_string()]);
        assert_eq!(fids, vec!["a".to_string()]);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    }

    fn completed_task(gid: &str) -> Aria2Task {
        Aria2Task {
            gid: gid.to_string(),
            status: "complete".to_string(),
            total_length: 1024,
            completed_length: 1024,
            download_speed: 0,
            upload_speed: 0,
            connections: 0,
            dir: "/downloads/anime".to_string(),
            file_name: "Show.S01E01.mkv".to_string(),
            error_code: String::new(),
            error_message: String::new(),
            progress: 100.0,
            eta_seconds: None,
            files: vec![],
        }
    }

    #[test]
    fn completed_download_history_matches_when_gid_changes() {
        let task = completed_task("new-gid");
        let notifications = vec![Notification {
            id: "n1".to_string(),
            level: "success".to_string(),
            event: "download_completed".to_string(),
            title: "下载完成: Show.S01E01.mkv".to_string(),
            message: "文件：Show.S01E01.mkv\n目录：/downloads/anime\n大小：1.00 KB".to_string(),
            meta: HashMap::from([
                ("gid".to_string(), json!("old-gid")),
                ("file_name".to_string(), json!("Show.S01E01.mkv")),
                ("dir".to_string(), json!("/downloads/anime")),
                ("total_length".to_string(), json!(1024)),
            ]),
            read: false,
            created_at: 1,
        }];

        assert!(completed_download_already_recorded(
            &notifications,
            &HashSet::new(),
            &task
        ));
    }

    #[test]
    fn completed_download_history_uses_push_jobs_when_notifications_were_cleared() {
        let task = completed_task("new-gid");
        let (title, message) = download_completed_title_message(&task);
        let pushed_downloads = HashSet::from([(title, message)]);

        assert!(completed_download_already_recorded(
            &[],
            &pushed_downloads,
            &task
        ));
    }

    #[test]
    fn test_subscription_download_lookup_from_notifications() {
        let notifications = vec![Notification {
            id: "n1".to_string(),
            level: "success".to_string(),
            event: "subscription_transferred".to_string(),
            title: "转存".to_string(),
            message: String::new(),
            meta: HashMap::from([
                ("subscription_id".to_string(), json!("sub1")),
                (
                    "sync_downloads".to_string(),
                    json!([
                        {"gid": "gid-1", "file_name": "Show.S01E11.mkv"},
                        {"gid": "gid-2", "file_name": "Show.S01E12.mkv"}
                    ]),
                ),
            ]),
            read: false,
            created_at: 1,
        }];

        assert_eq!(
            subscription_id_for_download_gid(&notifications, "gid-2"),
            Some("sub1".to_string())
        );
        assert_eq!(
            subscription_id_for_download_gid(&notifications, "missing"),
            None
        );
    }

    #[test]
    fn test_completed_subscription_download_files() {
        let notifications = vec![Notification {
            id: "n1".to_string(),
            level: "success".to_string(),
            event: "subscription_transferred".to_string(),
            title: "转存".to_string(),
            message: String::new(),
            meta: HashMap::from([
                ("subscription_id".to_string(), json!("sub1")),
                (
                    "sync_downloads".to_string(),
                    json!([
                        {"gid": "gid-1", "file_name": "Show.S01E11.mkv"},
                        {"gid": "gid-2", "file_name": "Show.S01E12.mkv"}
                    ]),
                ),
            ]),
            read: false,
            created_at: 1,
        }];
        let completed = HashSet::from(["gid-2".to_string()]);

        assert_eq!(
            completed_subscription_download_files(&notifications, "sub1", &completed),
            vec!["Show.S01E12.mkv".to_string()]
        );
    }
}
