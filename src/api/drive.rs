use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::clients::aria2::{Aria2Task, Aria2Version};
use crate::clients::{Aria2Client, NormalizedItem, QuarkSaveClient};
use crate::error::{AppError, Result};
use crate::models::Settings;
use crate::store::SettingsStore;

/// 网盘状态
pub struct DriveState {
    pub settings_store: Arc<SettingsStore>,
}

/// 列出目录请求
#[derive(Debug, Deserialize)]
pub struct ListRequest {
    pub path: Option<String>,
    pub fid: Option<String>,
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

    let client = QuarkSaveClient::new(cookie);

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

    match client.list_dir(&fid).await {
        Ok(items) => {
            // items 已经是 Vec<NormalizedItem>，直接返回
            Ok(Json(ListResponse { list: items }))
        }
        Err(e) => {
            tracing::error!("列出目录失败: {}", e);
            Ok(Json(ListResponse { list: vec![] }))
        }
    }
}

/// 测试夸克连接
async fn test_quark(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<TestRequest>,
) -> Result<impl IntoResponse> {
    let cookie = if req.cookie.trim().is_empty() {
        state.settings_store.get().await.quark_cookie
    } else {
        req.cookie
    };

    if cookie.trim().is_empty() {
        return Ok(Json(TestResponse {
            success: false,
            nickname: None,
            error: Some("未配置夸克 Cookie".to_string()),
        }));
    }

    let client = QuarkSaveClient::new(cookie);

    match client.list_dir("0").await {
        Ok(_) => Ok(Json(TestResponse {
            success: true,
            nickname: Some("夸克用户".to_string()),
            error: None,
        })),
        Err(e) => Ok(Json(TestResponse {
            success: false,
            nickname: None,
            error: Some(format!("连接失败: {}", e)),
        })),
    }
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
    let settings = state.settings_store.get().await;
    if settings.aria2_rpc_url.trim().is_empty() {
        return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
    }

    let aria2 = Aria2Client::new(settings.aria2_rpc_url, settings.aria2_secret, String::new());
    let tasks = aria2.list_tasks(req.stopped_limit.clamp(1, 50)).await?;

    Ok(Json(Aria2TasksResponse {
        success: true,
        active: tasks.active,
        waiting: tasks.waiting,
        stopped: tasks.stopped,
    }))
}

/// 测试 Aria2 RPC 连接
async fn test_aria2(State(state): State<Arc<DriveState>>) -> Result<Json<Aria2TestResponse>> {
    let settings = state.settings_store.get().await;
    if settings.aria2_rpc_url.trim().is_empty() {
        return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
    }

    let aria2 = Aria2Client::new(
        settings.aria2_rpc_url.clone(),
        settings.aria2_secret.clone(),
        String::new(),
    );
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
        Ok(fid) => Ok(Json(FindPathResponse { fid, found: true })),
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
pub fn routes(settings_store: Arc<SettingsStore>) -> Router {
    let state = Arc::new(DriveState { settings_store });

    Router::new()
        .route("/api/drive", get(list_drive))
        .route("/api/drive/find-path", get(find_path))
        .route("/api/drive/mkdir", post(mkdir))
        .route("/api/drive/delete", post(delete_items))
        .route("/api/drive/rename", post(rename_item))
        .route("/api/drive/aria2", post(send_to_aria2))
        .route("/api/drive/aria2/tasks", get(list_aria2_tasks))
        .route("/api/drive/aria2/test", get(test_aria2))
        .route("/api/drive/aria2/browse", get(browse_aria2_dir))
        .route("/api/quark/test", post(test_quark))
        .with_state(state)
}

fn default_stopped_limit() -> u64 {
    10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_fids_trims_and_dedups() {
        let fids = normalize_fids(vec![" a ".to_string(), "".to_string(), "a".to_string()]);
        assert_eq!(fids, vec!["a".to_string()]);
    }
}
