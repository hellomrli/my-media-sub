use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::clients::{Aria2Client, NormalizedItem, QuarkSaveClient};
use crate::error::{AppError, Result};
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
    let aria2 = Aria2Client::new(
        settings.aria2_rpc_url,
        settings.aria2_secret,
        settings.aria2_dir,
    );
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
        .route("/api/quark/test", post(test_quark))
        .with_state(state)
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
