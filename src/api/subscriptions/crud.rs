use super::*;

/// 列出所有订阅
pub(super) async fn list_subscriptions(
    State(state): State<Arc<SubscriptionState>>,
    Query(query): Query<ListSubscriptionsQuery>,
) -> Result<Json<Response<Vec<Subscription>>>> {
    let subscriptions = match query.limit {
        Some(limit) => {
            state
                .store
                .list_paginated(query.offset.unwrap_or(0), limit)
                .await
        }
        None => state.store.list().await,
    };
    Ok(Json(Response::ok(subscriptions)))
}

/// 获取单个订阅
pub(super) async fn get_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    match state.store.get(&id).await {
        Some(sub) => Ok(Json(Response::ok(sub))),
        None => Err(AppError::NotFound("订阅不存在".to_string())),
    }
}

/// 获取单个订阅的剧集、流水线与活动聚合状态
/// 创建订阅
pub(super) async fn create_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Json(req): Json<CreateSubscriptionRequest>,
) -> Result<impl IntoResponse> {
    if let Some(schedule) = req.manual_schedule.as_ref() {
        validate_manual_schedule(schedule).map_err(AppError::Validation)?;
    }
    let settings = state.settings_store.get().await;
    let rules = create_rules(&req, &settings);
    let rule_preset_id = req.rule_preset_id.trim().to_string();
    let rule_summary = summarize_rules(Some(&rules));
    let id = format!("{:x}", md5::compute(format!("{}:{}", req.url, req.title)));
    let id = &id[..12];

    let now = unix_now();

    let season = req.season.max(1);
    let media_type = if req.media_type.is_empty() {
        "series".to_string()
    } else {
        req.media_type
    };
    let start_episode_number =
        normalize_start_episode_number(req.start_episode_number, &media_type);
    let total_episode_number =
        episode_count_for_season(req.metadata.as_ref(), season).or(rules.finish_after_episode);

    let subscription = Subscription {
        id: id.to_string(),
        title: req.title,
        source_title: String::new(),
        media_type,
        season,
        start_episode_number,
        current_episode_number: 0,
        total_episode_number,
        source_group: String::new(),
        metadata: req.metadata,
        manual_schedule: req.manual_schedule,
        cloud_type: if req.cloud_type.is_empty() {
            "quark".to_string()
        } else {
            req.cloud_type
        },
        url: req.url,
        password: req.password,
        known_files: vec![],
        known_file_keys: vec![],
        known_episodes: vec![],
        transferred_files: vec![],
        transferred_file_keys: vec![],
        last_probe: None,
        last_plan_summary: String::new(),
        notify_only: req.notify_only,
        sync_download_enabled: req.sync_download_enabled,
        sync_download_dir: req.sync_download_dir,
        strm_enabled: req.strm_enabled,
        enabled: true,
        completed: false,
        rules,
        rule_preset_id,
        created_at: now,
        updated_at: now,
        last_checked_at: now,
        last_new_files: vec![],
        last_new_episodes: vec![],
        last_check_summary: String::new(),
        check_history: vec![],
        status: "active".to_string(),
        invalid_since: None,
        last_error: String::new(),
        rule_summary,
        source_candidates: vec![],
        last_source_search_time: None,
        previous_share_links: vec![],
        source_failure_count: 0,
        last_source_switch_at: None,
        source_switch_history: vec![],
    };

    let created = state.store.create(subscription).await?;
    Ok((StatusCode::CREATED, Json(Response::ok(created))))
}

/// 更新订阅
pub(super) async fn update_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSubscriptionRequest>,
) -> Result<impl IntoResponse> {
    if let Some(Some(schedule)) = req.manual_schedule.as_ref() {
        validate_manual_schedule(schedule).map_err(AppError::Validation)?;
    }
    let has_explicit_total_episode_number = req.total_episode_number.is_some();
    let keep_progress_on_source_change = req.keep_progress_on_source_change.unwrap_or(true);
    let continue_from_current_episode =
        continue_from_current_episode_default(req.continue_from_current_episode);
    let settings = state.settings_store.get().await;
    let requested_rule_preset_id = req
        .rule_preset_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToString::to_string);
    let requested_preset_rules = requested_rule_preset_id
        .as_deref()
        .and_then(|id| preset_rules(&settings, id));
    let updated = state
        .store
        .update(&id, |sub| {
            let mut source_changed = false;
            if let Some(title) = req.title {
                sub.title = title;
            }
            if let Some(url) = req.url {
                source_changed |= url != sub.url;
                sub.url = url;
            }
            if let Some(password) = req.password {
                source_changed |= password != sub.password;
                sub.password = password;
            }
            if let Some(media_type) = req.media_type {
                sub.media_type = media_type;
            }
            if let Some(season) = req.season {
                sub.season = season.max(1);
            }
            if let Some(start_episode_number) = req.start_episode_number {
                sub.start_episode_number =
                    normalize_start_episode_number(Some(start_episode_number), &sub.media_type);
            }
            if sub.media_type == "movie" {
                sub.start_episode_number = None;
            }
            if let Some(cloud_type) = req.cloud_type {
                sub.cloud_type = cloud_type;
            }
            if let Some(enabled) = req.enabled {
                sub.enabled = enabled;
            }
            if let Some(notify_only) = req.notify_only {
                sub.notify_only = notify_only;
            }
            if let Some(sync_download_enabled) = req.sync_download_enabled {
                sub.sync_download_enabled = sync_download_enabled;
            }
            if let Some(sync_download_dir) = req.sync_download_dir {
                sub.sync_download_dir = sync_download_dir;
            }
            if let Some(strm_enabled) = req.strm_enabled {
                sub.strm_enabled = strm_enabled;
            }
            if let Some(total_episode_number) = req.total_episode_number {
                sub.total_episode_number = total_episode_number;
            }
            if let Some(metadata) = req.metadata {
                sub.metadata = metadata;
            }
            if let Some(manual_schedule) = req.manual_schedule {
                sub.manual_schedule = manual_schedule;
            }
            if let Some(rules) = req.rules {
                sub.rules = rules;
            } else if let Some(rules) = requested_preset_rules {
                sub.rules = rules;
            }
            if let Some(rule_preset_id) = requested_rule_preset_id {
                sub.rule_preset_id = rule_preset_id;
            }
            if let Some(target_dir) = req.target_dir {
                sub.rules.target_dir = target_dir;
            }
            if let Some(rename_template) = req.rename_template {
                sub.rules.rename_template = rename_template;
            }
            apply_source_change_options(
                sub,
                source_changed,
                keep_progress_on_source_change,
                continue_from_current_episode,
            );
            if !has_explicit_total_episode_number {
                if let Some(count) = episode_count_for_season(sub.metadata.as_ref(), sub.season) {
                    sub.total_episode_number = Some(count);
                } else if sub.total_episode_number.is_none() {
                    sub.total_episode_number = sub.rules.finish_after_episode;
                }
            }
            reconcile_completion_status(sub);
            sub.rule_summary = summarize_rules(Some(&sub.rules));
            sub.updated_at = unix_now();
        })
        .await?;

    match updated {
        Some(sub) => Ok(Json(Response::ok(sub))),
        None => Err(AppError::NotFound("订阅不存在".to_string())),
    }
}

/// 删除订阅
pub(super) async fn delete_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let deleted = state.store.delete(&id).await?;
    if deleted {
        Ok((StatusCode::NO_CONTENT, ()))
    } else {
        Err(AppError::NotFound("订阅不存在".to_string()))
    }
}
