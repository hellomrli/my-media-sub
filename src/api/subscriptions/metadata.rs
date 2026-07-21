use super::*;

/// 后台刮削单个订阅元数据
pub(super) async fn scrape_subscription_metadata(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
    Json(req): Json<ScrapeMetadataRequest>,
) -> Result<impl IntoResponse> {
    if state.store.get(&id).await.is_none() {
        return Err(AppError::NotFound("订阅不存在".to_string()));
    }

    let job = state
        .job_queue
        .submit_metadata_scrape(MetadataScrapePayload {
            subscription_id: Some(id),
            overwrite: req.overwrite,
        })
        .await?;

    Ok((StatusCode::ACCEPTED, Json(Response::ok(job))))
}

/// 预览订阅规则产生的重命名和转存计划
pub(super) async fn preview_subscription_rename(
    State(state): State<Arc<SubscriptionState>>,
    Json(req): Json<RenamePreviewRequest>,
) -> Result<impl IntoResponse> {
    let base = if let Some(id) = req.subscription_id.as_deref() {
        Some(
            state
                .store
                .get(id)
                .await
                .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?,
        )
    } else {
        None
    };
    let base_ref = base.as_ref();
    let mut sub = preview_subscription(&req, base_ref);
    let settings = state.settings_store.get().await;
    sub.rules = effective_rules(
        &sub.rules,
        &sub.media_type,
        &settings.default_rename_template,
    );
    let mut files = preview_files(&req, &sub);
    let mut source_probed = false;
    let mut probe_warning = String::new();
    if req.probe_source && !sub.url.trim().is_empty() {
        if settings.quark_cookie.trim().is_empty() {
            probe_warning = "未配置夸克 Cookie，已使用样例文件预览".to_string();
        } else {
            let probe = crate::clients::QuarkShareProbe::new(settings.quark_cookie.clone())
                .probe(&sub.url, &sub.password, 300)
                .await;
            if probe.ok && !probe.files.is_empty() {
                files = probe
                    .files
                    .into_iter()
                    .map(|file| RuleProbeFile {
                        name: file.name,
                        fid: file.fid,
                        is_dir: file.is_dir,
                        size: file.size,
                        parent_path: file.parent_path,
                        updated_at: file.updated_at,
                    })
                    .collect();
                source_probed = true;
            } else {
                let detail = probe.message.trim();
                probe_warning = if detail.is_empty() {
                    "分享探测失败或无文件，已使用样例文件预览".to_string()
                } else {
                    format!("分享探测失败：{detail}；已使用样例文件预览")
                };
            }
        }
    }
    let plan = build_transfer_plan(&sub, Some(&files), None, None, None);
    let items: Vec<RenamePreviewItem> = plan
        .items
        .into_iter()
        .map(|item| RenamePreviewItem {
            source_name: item.source_name,
            source_parent_path: item.source_parent_path,
            target_name: item.target_name,
            action: item.action,
            skip_reason: item.skip_reason,
            episode: item.episode,
            episodes: item.episodes,
            season: item.season,
            target_dir: item.target_dir,
        })
        .collect();
    let groups = group_rename_preview_items(&items);
    let show_root = resolve_show_root(&sub, &plan.target_dir);

    Ok(Json(Response::ok(RenamePreviewResponse {
        summary: plan.summary,
        target_dir: if sub.is_multi_season() {
            show_root.clone()
        } else {
            plan.target_dir
        },
        show_root,
        multi_season: sub.is_multi_season(),
        transfer_count: plan.transfer_count,
        skip_count: plan.skip_count,
        matched_count: plan.matched_count,
        current_episode_number: plan.current_episode_number,
        episodes: plan.episodes,
        missing_episodes: plan.missing_episodes,
        duplicate_episodes: plan.duplicate_episodes,
        probe_warning,
        source_probed,
        groups,
        items,
    })))
}

/// 后台批量刮削订阅元数据
pub(super) async fn scrape_all_subscription_metadata(
    State(state): State<Arc<SubscriptionState>>,
    Json(req): Json<ScrapeMetadataRequest>,
) -> Result<impl IntoResponse> {
    let job = state
        .job_queue
        .submit_metadata_scrape(MetadataScrapePayload {
            subscription_id: None,
            overwrite: req.overwrite,
        })
        .await?;

    Ok((StatusCode::ACCEPTED, Json(Response::ok(job))))
}
