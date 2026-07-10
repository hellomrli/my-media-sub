use super::*;

/// 测试夸克连接
pub(super) async fn test_quark(
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
        return Ok(json_ok(TestResponse {
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
        Ok(_) => Ok(json_ok(TestResponse {
            success: true,
            nickname: Some("夸克用户".to_string()),
            error: None,
            ..health
        })),
        Err(e) => Ok(json_ok(TestResponse {
            success: false,
            nickname: None,
            error: Some(format!("连接失败: {}", e)),
            ..health
        })),
    }
}

pub(super) fn quark_health_snapshot(settings: &Settings) -> TestResponse {
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

pub(super) async fn quark_signin(
    State(state): State<Arc<DriveState>>,
) -> Result<Json<Response<QuarkSigninResponse>>> {
    let result = state
        .quark_signin_service
        .signin_with_failure_notice()
        .await?;
    let message = signin_message(&result);
    Ok(json_ok(QuarkSigninResponse {
        success: true,
        message,
        result,
    }))
}

/// 创建文件夹
pub(super) async fn mkdir(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<MkdirRequest>,
) -> Result<Json<Response<ActionResponse>>> {
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
    Ok(json_ok(ActionResponse {
        success: true,
        message: Some("创建成功".to_string()),
        fid: Some(fid),
    }))
}

/// 删除文件/文件夹
pub(super) async fn delete_items(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<DeleteRequest>,
) -> Result<Json<Response<ActionResponse>>> {
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
    Ok(json_ok(ActionResponse {
        success: true,
        message: Some(format!("已删除 {} 项", fids.len())),
        fid: None,
    }))
}

/// 重命名文件/文件夹
pub(super) async fn rename_item(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<RenameRequest>,
) -> Result<Json<Response<ActionResponse>>> {
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
    Ok(json_ok(ActionResponse {
        success: true,
        message: Some("重命名成功".to_string()),
        fid: Some(req.fid),
    }))
}
