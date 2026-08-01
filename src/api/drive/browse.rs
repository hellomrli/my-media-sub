use super::*;

/// 列出目录
pub(super) async fn list_drive(
    State(state): State<Arc<DriveState>>,
    Query(req): Query<ListRequest>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie.clone();

    // 未配置 Cookie 时返回空列表（与历史契约一致），避免设置页未完成时整页报错。
    if cookie.is_empty() {
        return Ok(json_ok(ListResponse { list: vec![] }));
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

    // Cookie 已配置但列举失败时必须返回错误，不能伪装成空目录。
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
    prune_drive_cache(&mut cache);
    cache.insert(
        key,
        CachedDriveList {
            created_at: Instant::now(),
            items,
        },
    );
}

/// 缓存条目硬上限：20 秒 TTL 本身已能控制内存，这个上限只兜底极端浏览
/// 行为（同一窗口内快速翻几百个目录），避免长期运行无界增长。
const DRIVE_CACHE_MAX_ENTRIES: usize = 512;

/// 写入前清理过期条目；条目数超过硬上限时整体清空（目录列表缓存只有 20 秒
/// 有效期，清空代价极小）。
fn prune_drive_cache(cache: &mut HashMap<String, CachedDriveList>) {
    cache.retain(|_, cached| cached.created_at.elapsed() <= DRIVE_CACHE_TTL);
    if cache.len() >= DRIVE_CACHE_MAX_ENTRIES {
        cache.clear();
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_drive_cache_removes_expired_entries_and_keeps_fresh_ones() {
        let mut cache = HashMap::new();
        cache.insert(
            "old".to_string(),
            CachedDriveList {
                created_at: Instant::now() - Duration::from_secs(30),
                items: vec![],
            },
        );
        cache.insert(
            "fresh".to_string(),
            CachedDriveList {
                created_at: Instant::now(),
                items: vec![],
            },
        );

        prune_drive_cache(&mut cache);

        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key("fresh"));
        assert!(!cache.contains_key("old"));
    }

    #[test]
    fn prune_drive_cache_clears_when_entries_exceed_hard_cap() {
        let mut cache = HashMap::new();
        for index in 0..(DRIVE_CACHE_MAX_ENTRIES + 10) {
            cache.insert(
                format!("key-{index}"),
                CachedDriveList {
                    created_at: Instant::now(),
                    items: vec![],
                },
            );
        }

        prune_drive_cache(&mut cache);

        assert!(cache.is_empty());
    }
}
