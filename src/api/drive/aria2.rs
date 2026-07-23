use super::*;

/// 发送夸克网盘文件到 Aria2
pub(super) async fn send_to_aria2(
    State(state): State<Arc<DriveState>>,
    Json(req): Json<Aria2DownloadRequest>,
) -> Result<Json<Response<Aria2DownloadResponse>>> {
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
    let batch_limit = settings.aria2_batch_submit_limit.max(1);
    let mut items = Vec::with_capacity(fids.len());

    for (batch_index, chunk) in fids.chunks(batch_limit).enumerate() {
        if batch_index > 0 {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let download_infos = quark.download_infos(chunk).await?;
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
    }

    Ok(json_ok(Aria2DownloadResponse {
        success: true,
        count: items.len(),
        message: format!("已提交 {} 个 Aria2 下载任务", items.len()),
        items,
    }))
}

/// 获取 Aria2 下载任务状态
pub(super) async fn list_aria2_tasks(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<Aria2TasksRequest>,
) -> Result<Json<Response<Aria2TasksResponse>>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let tasks = aria2.list_tasks(req.stopped_limit.clamp(1, 50)).await?;
    state
        .download_monitor
        .notify_completed_downloads(&tasks.stopped)
        .await;

    let (subscriptions, notifications) = tokio::join!(
        state.subscription_store.list(),
        state.notification_store.list(true),
    );
    let contexts = aria2_automation_contexts(&notifications, &subscriptions);

    Ok(json_ok(Aria2TasksResponse {
        success: true,
        active: enrich_aria2_tasks(tasks.active, &contexts),
        waiting: enrich_aria2_tasks(tasks.waiting, &contexts),
        stopped: enrich_aria2_tasks(tasks.stopped, &contexts),
    }))
}

pub(super) fn enrich_aria2_tasks(
    tasks: Vec<Aria2Task>,
    contexts: &HashMap<String, Aria2AutomationContext>,
) -> Vec<Aria2TaskView> {
    tasks
        .into_iter()
        .map(|task| Aria2TaskView {
            automation: contexts.get(&task.gid).cloned(),
            task,
        })
        .collect()
}

pub(super) fn aria2_client(settings: &Settings) -> Result<Aria2Client> {
    if settings.aria2_rpc_url.trim().is_empty() {
        return Err(AppError::Validation("未配置 Aria2 RPC URL".to_string()));
    }

    Ok(Aria2Client::new(
        settings.aria2_rpc_url.clone(),
        settings.aria2_secret.clone(),
        String::new(),
    ))
}

pub(super) async fn pause_aria2_task(
    State(state): State<Arc<DriveState>>,
    AxumPath(gid): AxumPath<String>,
) -> Result<Json<Response<Aria2TaskActionResponse>>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let gid = aria2.pause(&gid).await?;
    Ok(json_ok(Aria2TaskActionResponse {
        success: true,
        message: "已暂停下载任务".to_string(),
        gid: Some(gid),
        affected_count: 1,
    }))
}

pub(super) async fn resume_aria2_task(
    State(state): State<Arc<DriveState>>,
    AxumPath(gid): AxumPath<String>,
) -> Result<Json<Response<Aria2TaskActionResponse>>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let gid = aria2.unpause(&gid).await?;
    Ok(json_ok(Aria2TaskActionResponse {
        success: true,
        message: "已继续下载任务".to_string(),
        gid: Some(gid),
        affected_count: 1,
    }))
}

pub(super) async fn stop_aria2_task(
    State(state): State<Arc<DriveState>>,
    AxumPath(gid): AxumPath<String>,
) -> Result<Json<Response<Aria2TaskActionResponse>>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let gid = aria2.force_remove(&gid).await?;
    Ok(json_ok(Aria2TaskActionResponse {
        success: true,
        message: "已停止下载任务".to_string(),
        gid: Some(gid),
        affected_count: 1,
    }))
}

pub(super) async fn delete_aria2_task(
    State(state): State<Arc<DriveState>>,
    AxumPath(gid): AxumPath<String>,
) -> Result<Json<Response<Aria2TaskActionResponse>>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let gid = gid.trim().to_string();
    if gid.is_empty() {
        return Err(AppError::Validation("Aria2 任务 GID 为空".to_string()));
    }

    if aria2.remove_download_result(&gid).await.is_err() {
        aria2.force_remove(&gid).await?;
        let _ = aria2.remove_download_result(&gid).await;
    }

    Ok(json_ok(Aria2TaskActionResponse {
        success: true,
        message: "已删除下载任务记录".to_string(),
        gid: Some(gid),
        affected_count: 1,
    }))
}

pub(super) async fn pause_all_aria2_tasks(
    State(state): State<Arc<DriveState>>,
) -> Result<Json<Response<Aria2TaskActionResponse>>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    aria2.pause_all().await?;
    Ok(json_ok(Aria2TaskActionResponse {
        success: true,
        message: "已暂停全部下载任务".to_string(),
        gid: None,
        affected_count: 0,
    }))
}

pub(super) async fn stop_all_aria2_tasks(
    State(state): State<Arc<DriveState>>,
) -> Result<Json<Response<Aria2TaskActionResponse>>> {
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

    Ok(json_ok(Aria2TaskActionResponse {
        success: true,
        message: format!("已停止 {} 个下载任务", affected_count),
        gid: None,
        affected_count,
    }))
}

/// 测试 Aria2 RPC 连接
pub(super) async fn test_aria2(
    State(state): State<Arc<DriveState>>,
) -> Result<Json<Response<Aria2TestResponse>>> {
    let aria2 = aria2_client(&state.settings_store.get().await)?;
    let Aria2Version {
        version,
        enabled_features,
    } = aria2.get_version().await?;

    Ok(json_ok(Aria2TestResponse {
        success: true,
        message: format!("Aria2 连接成功，版本 {}", version),
        version,
        enabled_features,
    }))
}

/// 浏览指定媒体类型 Aria2 下载目录下的文件夹。
pub(super) async fn browse_aria2_dir(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<Aria2BrowseRequest>,
) -> Result<Json<Response<Aria2BrowseResponse>>> {
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

    Ok(json_ok(Aria2BrowseResponse {
        success: true,
        root: root.display().to_string(),
        current: requested.display().to_string(),
        parent,
        items,
    }))
}

pub(super) fn aria2_browse_root(settings: &Settings, media_type: &str) -> String {
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

pub(super) fn canonical_dir(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path
        .as_ref()
        .canonicalize()
        .map_err(|e| AppError::Validation(format!("目录不存在或不可访问: {}", e)))?;
    if !path.is_dir() {
        return Err(AppError::Validation("路径不是目录".to_string()));
    }
    Ok(path)
}

pub(super) fn normalize_fids(fids: Vec<String>) -> Vec<String> {
    let mut fids: Vec<String> = fids
        .into_iter()
        .map(|fid| fid.trim().to_string())
        .filter(|fid| !fid.is_empty())
        .collect();
    fids.sort();
    fids.dedup();
    fids
}

pub(super) fn default_stopped_limit() -> u64 {
    10
}
