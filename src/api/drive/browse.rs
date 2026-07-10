use super::*;

/// 列出目录
pub(super) async fn list_drive(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<ListRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie.clone();

    if cookie.is_empty() {
        return Ok(json_ok(ListResponse { list: vec![] }));
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
            return Ok(json_ok(ListResponse { list: items }));
        }
    }

    let client = QuarkSaveClient::new(cookie);

    match client.list_dir(&fid).await {
        Ok(items) => {
            cache_drive_items(&state, cache_key, items.clone()).await;
            Ok(json_ok(ListResponse { list: items }))
        }
        Err(e) => {
            tracing::error!("列出目录失败: {}", e);
            Ok(json_ok(ListResponse { list: vec![] }))
        }
    }
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

    // 使用 ensure_dir_path 查找或创建路径
    match client.ensure_dir_path(&req.path).await {
        Ok(fid) => {
            clear_drive_cache(&state).await;
            Ok(json_ok(FindPathResponse { fid, found: true }))
        }
        Err(e) => {
            tracing::warn!("查找路径 {} 失败: {}", req.path, e);
            Ok(json_ok(FindPathResponse {
                fid: "0".to_string(),
                found: false,
            }))
        }
    }
}
