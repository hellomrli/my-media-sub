use super::response::{json_ok, ApiResponse as Response};
use crate::clients::aria2::{Aria2Task, Aria2Version};
use crate::clients::{Aria2Client, NormalizedItem, QuarkSaveClient, QuarkSigninResult};
use crate::error::{AppError, Result};
use crate::models::{Notification, Settings, Subscription};
#[cfg(test)]
use crate::services::push::PushEvent;
use crate::services::quark_signin::signin_message;
use crate::services::{DownloadMonitorService, QuarkSigninService};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};
use axum::{
    extract::{Path as AxumPath, Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
#[cfg(test)]
use std::collections::HashSet;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

mod actions;
mod aria2;
mod automation;
mod browse;

use actions::{delete_items, mkdir, quark_signin, rename_item, test_quark};
use aria2::{
    browse_aria2_dir, default_stopped_limit, delete_aria2_task, list_aria2_tasks,
    pause_all_aria2_tasks, pause_aria2_task, resume_aria2_task, send_to_aria2,
    stop_all_aria2_tasks, stop_aria2_task, test_aria2,
};
use automation::aria2_automation_contexts;
use browse::{clear_drive_cache, find_path, list_drive};

#[cfg(test)]
use aria2::{normalize_fids, validate_aria2_batch_size};
#[cfg(test)]
use automation::{
    completed_download_already_recorded, completed_subscription_download_files,
    download_completed_title_message, format_bytes, subscription_id_for_download_gid,
};

/// 网盘状态
pub struct DriveState {
    pub settings_store: Arc<SettingsStore>,
    pub quark_signin_service: Arc<QuarkSigninService>,
    pub download_monitor: Arc<DownloadMonitorService>,
    pub subscription_store: Arc<SubscriptionStore>,
    pub notification_store: Arc<NotificationStore>,
    pub drive_cache: RwLock<HashMap<String, CachedDriveList>>,
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
    pub active: Vec<Aria2TaskView>,
    pub waiting: Vec<Aria2TaskView>,
    pub stopped: Vec<Aria2TaskView>,
}

#[derive(Serialize)]
pub struct Aria2TaskView {
    #[serde(flatten)]
    pub task: Aria2Task,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation: Option<Aria2AutomationContext>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Aria2AutomationContext {
    pub subscription_id: String,
    pub subscription_title: String,
    pub target_dir: String,
    pub submitted_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode: Option<i32>,
    pub transfer_status: String,
    pub rename_status: String,
    pub strm_status: String,
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
async fn drive_client(state: &DriveState) -> Result<QuarkSaveClient> {
    let cookie = state.settings_store.get().await.quark_cookie;
    if cookie.trim().is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }
    Ok(QuarkSaveClient::new(cookie))
}

/// 创建网盘路由
pub fn routes(
    settings_store: Arc<SettingsStore>,
    quark_signin_service: Arc<QuarkSigninService>,
    download_monitor: Arc<DownloadMonitorService>,
    subscription_store: Arc<SubscriptionStore>,
    notification_store: Arc<NotificationStore>,
) -> Router {
    let state = Arc::new(DriveState {
        settings_store,
        quark_signin_service,
        download_monitor,
        subscription_store,
        notification_store,
        drive_cache: RwLock::new(HashMap::new()),
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
    fn aria2_batch_limit_rejects_oversized_submission() {
        assert!(validate_aria2_batch_size(20, 20).is_ok());
        let error = validate_aria2_batch_size(21, 20).unwrap_err();
        assert!(matches!(error, AppError::RateLimited(_)));
        assert!(error.to_string().contains("当前选择 21 个"));
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
    fn aria2_context_links_subscription_pipeline() {
        let subscription: Subscription = serde_json::from_value(json!({
            "id": "sub1",
            "title": "Show",
            "media_type": "series",
            "season": 1,
            "url": "https://pan.quark.cn/s/test",
            "enabled": true,
            "completed": false,
            "created_at": 1,
            "updated_at": 1,
            "last_checked_at": 1,
            "status": "active"
        }))
        .unwrap();
        let notifications = vec![Notification {
            id: "n1".to_string(),
            level: "success".to_string(),
            event: "subscription_transferred".to_string(),
            title: "转存".to_string(),
            message: "已生成 1 个 STRM 文件".to_string(),
            meta: HashMap::from([
                ("subscription_id".to_string(), json!("sub1")),
                ("subscription_title".to_string(), json!("Show")),
                ("target_dir".to_string(), json!("/series/Show/Season 1")),
                ("strm_generated_count".to_string(), json!(1)),
                (
                    "sync_downloads".to_string(),
                    json!([{"gid": "gid-12", "file_name": "Show.S01E12.mkv"}]),
                ),
            ]),
            read: false,
            created_at: 10,
        }];

        let contexts = aria2_automation_contexts(&notifications, &[subscription]);
        let context = contexts.get("gid-12").unwrap();
        assert_eq!(context.subscription_id, "sub1");
        assert_eq!(context.subscription_title, "Show");
        assert_eq!(context.episode, Some(12));
        assert_eq!(context.strm_status, "generated");
        assert_eq!(context.rename_status, "completed");
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
