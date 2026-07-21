use super::*;

/// 列出目录
pub(super) async fn list_drive(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<ListRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie.clone();

    if cookie.is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }

    let client = QuarkSaveClient::new(cookie.clone());

    // 优先使用 fid；否则按 path 只读解析（不创建目录）
    let fid = if let Some(f) = req.fid.filter(|value| !value.trim().is_empty()) {
        f
    } else {
        let path = req.path.unwrap_or_else(|| "/".to_string());
        if path.trim().is_empty() || path.trim() == "/" {
            "0".to_string()
        } else {
            client
                .resolve_dir_path(&path)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("目录不存在: {path}")))?
        }
    };
    let cache_key = drive_cache_key(&cookie, &fid);
    if !req.refresh {
        if let Some(items) = cached_drive_items(&state, &cache_key).await {
            return Ok(json_ok(ListResponse { list: items }));
        }
    }

    let items = client.list_dir(&fid).await.map_err(|error| {
        tracing::error!("列出目录失败: {}", error);
        error
    })?;
    cache_drive_items(&state, cache_key, items.clone()).await;
    Ok(json_ok(ListResponse { list: items }))
}

pub(super) async fn cached_drive_items(
    state: &DriveState,
    key: &str,
) -> Option<Vec<NormalizedItem>> {
    let cache = state.drive_cache.read().await;
    let cached = cache.get(key)?;
    if cached.created_at.elapsed() > DRIVE_CACHE_TTL {
        return None;
    }
    Some(cached.items.clone())
}

pub(super) async fn cache_drive_items(state: &DriveState, key: String, items: Vec<NormalizedItem>) {
    let mut cache = state.drive_cache.write().await;
    cache.insert(
        key,
        CachedDriveList {
            created_at: Instant::now(),
            items,
        },
    );
}

pub(super) async fn clear_drive_cache(state: &DriveState) {
    state.drive_cache.write().await.clear();
}

pub(super) fn drive_cache_key(cookie: &str, fid: &str) -> String {
    let mut hasher = DefaultHasher::new();
    cookie.hash(&mut hasher);
    format!("{}:{}", hasher.finish(), fid.trim())
}

/// 测试夸克连接
/// 根据路径查找目录 fid
pub(super) async fn find_path(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<FindPathRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie.clone();

    if cookie.is_empty() {
        return Ok(json_ok(FindPathResponse {
            fid: "0".to_string(),
            found: false,
        }));
    }

    let client = QuarkSaveClient::new(cookie);

    // 只读查找，不创建缺失目录
    match client.resolve_dir_path(&req.path).await {
        Ok(Some(fid)) => Ok(json_ok(FindPathResponse { fid, found: true })),
        Ok(None) => Ok(json_ok(FindPathResponse {
            fid: "0".to_string(),
            found: false,
        })),
        Err(e) => {
            tracing::warn!("查找路径 {} 失败: {}", req.path, e);
            Err(e)
        }
    }
}
