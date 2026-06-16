use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::clients::{QuarkSaveClient, QuarkShareProbe};
use crate::error::Result;
use crate::store::SettingsStore;

/// 转存状态
pub struct TransferState {
    pub settings_store: Arc<SettingsStore>,
}

/// 转存请求
#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    /// 分享链接
    pub url: String,
    /// 提取码（可选）
    #[serde(default)]
    pub passcode: String,
    /// 目标目录 fid（可选，默认根目录）
    #[serde(default)]
    pub target_fid: String,
}

/// 转存响应
#[derive(Serialize)]
pub struct TransferResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_count: Option<usize>,
}

/// 转存分享链接到夸克网盘
async fn transfer_share(
    State(state): State<Arc<TransferState>>,
    Json(req): Json<TransferRequest>,
) -> Result<impl IntoResponse> {
    // 获取设置
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie.clone();

    if cookie.is_empty() {
        return Ok(Json(TransferResponse {
            success: false,
            message: Some("未配置夸克 Cookie".to_string()),
            file_count: None,
            saved_count: None,
        }));
    }

    // 1. 探测分享链接
    tracing::info!("探测分享链接: {}", req.url);
    let quark_probe = QuarkShareProbe::new(cookie.clone());
    let share_info = quark_probe.probe(&req.url, &req.passcode, 200).await;

    if !share_info.ok {
        return Ok(Json(TransferResponse {
            success: false,
            message: Some(format!("链接探测失败: {}", share_info.message)),
            file_count: None,
            saved_count: None,
        }));
    }

    if share_info.files.is_empty() {
        return Ok(Json(TransferResponse {
            success: false,
            message: Some("链接中没有可转存的文件".to_string()),
            file_count: Some(0),
            saved_count: None,
        }));
    }

    tracing::info!(
        "探测到 {} 个文件，其中 {} 个疑似视频集数",
        share_info.file_count,
        share_info.episode_count
    );

    // 2. 提取 pwd_id 和 stoken（重新获取）
    let pwd_id = match QuarkShareProbe::extract_pwd_id(&req.url) {
        Some(id) => id,
        None => {
            return Ok(Json(TransferResponse {
                success: false,
                message: Some("无法提取分享链接 ID".to_string()),
                file_count: Some(share_info.file_count),
                saved_count: None,
            }));
        }
    };

    // 3. 使用 QuarkSaveClient 转存
    let save_client = QuarkSaveClient::new(cookie);

    // 确定目标目录
    let target_fid = if req.target_fid.is_empty() {
        "0".to_string() // 根目录
    } else {
        req.target_fid
    };

    // 4. 调用转存接口
    // 注意：这里需要重新获取 stoken，因为探测时的 token 可能已过期
    // 我们需要一个辅助方法来获取 stoken
    match save_with_probe(
        &save_client,
        &quark_probe,
        &pwd_id,
        &req.passcode,
        &share_info.files,
        &target_fid,
    )
    .await
    {
        Ok(count) => {
            tracing::info!("成功转存 {} 个文件", count);
            Ok(Json(TransferResponse {
                success: true,
                message: Some(format!("成功转存 {} 个文件到网盘", count)),
                file_count: Some(share_info.file_count),
                saved_count: Some(count),
            }))
        }
        Err(e) => {
            tracing::error!("转存失败: {}", e);
            Ok(Json(TransferResponse {
                success: false,
                message: Some(format!("转存失败: {}", e)),
                file_count: Some(share_info.file_count),
                saved_count: None,
            }))
        }
    }
}

/// 辅助函数：使用探测到的文件信息进行转存
async fn save_with_probe(
    save_client: &QuarkSaveClient,
    probe: &QuarkShareProbe,
    pwd_id: &str,
    passcode: &str,
    _files: &[crate::clients::quark::QuarkFile],
    target_fid: &str,
) -> Result<usize> {
    // 1. 获取 stoken
    let (stoken, err) = probe.get_share_token(pwd_id, passcode).await?;

    if let Some(err_msg) = err {
        return Err(crate::error::AppError::Http(format!(
            "获取分享 token 失败: {}",
            err_msg
        )));
    }

    let stoken =
        stoken.ok_or_else(|| crate::error::AppError::Http("未能获取分享 token".to_string()))?;

    tracing::info!(
        "转存: pwd_id={}, stoken={}, target_fid={}",
        pwd_id,
        stoken,
        target_fid
    );

    // 2. 重新列出分享文件以获取最新的 share_fid_token
    // 这是关键：不能使用探测时的 token，需要重新获取
    let (fresh_files, err) = probe.list_share_files(pwd_id, &stoken, "0").await?;

    if let Some(err_msg) = err {
        return Err(crate::error::AppError::Http(format!(
            "重新获取文件列表失败: {}",
            err_msg
        )));
    }

    // 3. 收集顶层文件的 fid 和 share_fid_token
    let mut fid_list = Vec::new();
    let mut fid_token_list = Vec::new();

    for item in &fresh_files {
        let fid = item
            .get("fid")
            .or_else(|| item.get("file_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let share_fid_token = item
            .get("share_fid_token")
            .or_else(|| item.get("file_token"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !fid.is_empty() && !share_fid_token.is_empty() {
            fid_list.push(fid.to_string());
            fid_token_list.push(share_fid_token.to_string());
            tracing::debug!("文件: {} -> token: {}", fid, share_fid_token);
        }
    }

    if fid_list.is_empty() {
        return Err(crate::error::AppError::Validation(
            "没有可转存的文件（缺少 fid 或 token）".to_string(),
        ));
    }

    tracing::info!("准备转存 {} 个文件/文件夹", fid_list.len());

    // 4. 调用转存 API
    save_client
        .save_share_files(pwd_id, &stoken, &fid_list, &fid_token_list, target_fid)
        .await?;

    Ok(fid_list.len())
}

/// 创建转存路由
pub fn routes(settings_store: Arc<SettingsStore>) -> Router {
    let state = Arc::new(TransferState { settings_store });

    Router::new()
        .route("/api/transfer", post(transfer_share))
        .with_state(state)
}
