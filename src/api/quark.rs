use axum::{
    extract::State,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::clients::QuarkSaveClient;
use crate::error::Result;

/// 测试请求
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

/// 测试夸克连接
async fn test_quark(
    Json(req): Json<TestRequest>,
) -> Result<impl IntoResponse> {
    let client = QuarkSaveClient::new(req.cookie);

    // 尝试列出根目录来测试连接
    match client.list_dir("0").await {
        Ok(_) => {
            // 连接成功，可以尝试获取用户信息（如果API支持）
            Ok(Json(TestResponse {
                success: true,
                nickname: Some("夸克用户".to_string()), // 暂时返回固定值
                error: None,
            }))
        }
        Err(e) => {
            Ok(Json(TestResponse {
                success: false,
                nickname: None,
                error: Some(format!("连接失败: {}", e)),
            }))
        }
    }
}

/// 创建夸克路由
pub fn routes() -> Router {
    Router::new()
        .route("/api/quark/test", post(test_quark))
}
