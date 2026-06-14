use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::clients::{QuarkSaveClient, NormalizedItem};
use crate::error::Result;
use crate::store::SettingsStore;

/// 网盘状态
pub struct DriveState {
    pub settings_store: Arc<SettingsStore>,
}

/// 列出目录请求
#[derive(Debug, Deserialize)]
pub struct ListRequest {
    pub path: String,
}

/// 列出目录响应
#[derive(Serialize)]
pub struct ListResponse {
    pub list: Vec<NormalizedItem>,
}

/// 创建文件夹请求
#[derive(Debug, Deserialize)]
pub struct MkdirRequest {
    pub path: String,
    pub name: String,
}

/// 删除文件请求
#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub fid: String,
}

/// 重命名文件请求
#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub fid: String,
    pub name: String,
}

/// 测试夸克连接
#[derive(Debug, Deserialize)]
pub struct TestRequest {
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

    // 解析路径获取 fid
    let fid = if req.path == "/" || req.path.is_empty() {
        "0".to_string()
    } else {
        // 简化处理：暂时只支持根目录
        "0".to_string()
    };

    match client.list_dir(&fid).await {
        Ok(items) => {
            // 将 HashMap 转换为 NormalizedItem
            let normalized: Vec<NormalizedItem> = items
                .iter()
                .filter_map(|item| {
                    let fid = item.get("fid")?.as_str()?.to_string();
                    let name = item.get("file_name")?.as_str()?.to_string();
                    let file = item.get("file")?.as_bool().unwrap_or(true);
                    let size = item.get("size")?.as_i64().unwrap_or(0);
                    let updated_at = item
                        .get("updated_at")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                        .to_string();

                    Some(NormalizedItem {
                        fid,
                        name,
                        is_dir: !file,
                        size,
                        updated_at,
                    })
                })
                .collect();

            Ok(Json(ListResponse { list: normalized }))
        }
        Err(e) => {
            tracing::error!("列出目录失败: {}", e);
            Ok(Json(ListResponse { list: vec![] }))
        }
    }
}

/// 测试夸克连接
async fn test_quark(Json(req): Json<TestRequest>) -> Result<impl IntoResponse> {
    let client = QuarkSaveClient::new(req.cookie);

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

/// 创建网盘路由
pub fn routes(settings_store: Arc<SettingsStore>) -> Router {
    let state = Arc::new(DriveState { settings_store });

    Router::new()
        .route("/api/drive", get(list_drive))
        .route("/api/quark/test", post(test_quark))
        .with_state(state)
}
