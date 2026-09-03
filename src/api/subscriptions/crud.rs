use super::*;

/// 列出所有订阅（附带展示字段，避免前端重复推断）
pub(super) async fn list_subscriptions(
    State(state): State<Arc<SubscriptionState>>,
    Query(query): Query<ListSubscriptionsQuery>,
) -> Result<Json<Response<Vec<SubscriptionListItem>>>> {
    let subscriptions = match query.limit {
        Some(limit) => {
            state
                .store
                .list_paginated(query.offset.unwrap_or(0), limit)
                .await
        }
        None => state.store.list().await,
    };
    let items = subscriptions
        .into_iter()
        .map(SubscriptionListItem::from)
        .collect();
    Ok(Json(Response::ok(items)))
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
    let settings = state.settings_store.get().await;
    let rules = create_rules(&req, &settings);
    let rule_preset_id = req.rule_preset_id.trim().to_string();
    let rule_summary = summarize_rules(Some(&rules));
    let id = format!("{:x}", md5::compute(format!("{}:{}", req.url, req.title)));
    let id = &id[..12];

    let now = unix_now();

    // 季度输入统一解析为规范化列表：连续集合折叠为区间语义，
    // 跳季集合（如 1,3）保留 season_list。
    let (season, season_end, season_list) = if !req.season_spec.trim().is_empty() {
        let list = crate::models::subscription::normalize_season_list(
            crate::models::subscription::parse_season_spec_list(&req.season_spec),
        );
        (list.0, list.1, list.2)
    } else if let Some(list) = req.season_list.clone() {
        let list = crate::models::subscription::normalize_season_list(list);
        (list.0, list.1, list.2)
    } else {
        let (season, season_end) =
            crate::models::subscription::normalize_season_bounds(req.season, req.season_end);
        (season, season_end, None)
    };
    let media_type = if req.media_type.is_empty() {
        "series".to_string()
    } else {
        req.media_type
    };
    let start_episode_number =
        normalize_start_episode_number(req.start_episode_number, &media_type);
    let total_episode_number = if season_end.is_some() {
        // 多季订阅不把单季总集数当作完结目标，避免过早完结
        rules.finish_after_episode
    } else {
        episode_count_for_season(req.metadata.as_ref(), season).or(rules.finish_after_episode)
    };
    let cloud_type = validate_cloud_type(&req.cloud_type)?;
    let raw_title = req.title.trim().to_string();
    let cleaned_title = crate::services::metadata::clean_media_title(&raw_title);
    let title = if cleaned_title.is_empty() {
        raw_title.clone()
    } else {
        cleaned_title
    };

    let subscription = Subscription {
        id: id.to_string(),
        title,
        source_title: if raw_title.is_empty() {
            String::new()
        } else {
            raw_title
        },
        media_type,
        season,
        season_end,
        season_list,
        start_episode_number,
        current_episode_number: 0,
        total_episode_number,
        source_group: String::new(),
        tags: normalize_tags(req.tags),
        metadata: req.metadata,
        cloud_type,
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
        sync_downloads: vec![],
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
    let requested_cloud_type = req
        .cloud_type
        .as_deref()
        .map(validate_cloud_type)
        .transpose()?;
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
            let mut media_type_changed = false;
            if let Some(media_type) = req.media_type {
                media_type_changed = media_type != sub.media_type;
                sub.media_type = media_type;
            }
            let mut season_fields_changed = false;
            let season_range_before = (sub.season, sub.season_end, sub.season_list.clone());
            if let Some(spec) = req
                .season_spec
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let (season, season_end, season_list) =
                    crate::models::subscription::normalize_season_list(
                        crate::models::subscription::parse_season_spec_list(spec),
                    );
                sub.season = season;
                sub.season_end = season_end;
                sub.season_list = season_list;
                season_fields_changed = true;
            } else {
                if let Some(season) = req.season {
                    let season = season.max(1);
                    sub.season = season;
                    season_fields_changed = true;
                }
                if let Some(season_end) = req.season_end {
                    sub.season_end = season_end;
                    season_fields_changed = true;
                }
                // 季度集合：字段存在即生效；空数组清除集合回到区间语义。
                if let Some(list) = req.season_list.clone() {
                    if list.is_empty() {
                        sub.season_list = None;
                    } else {
                        let (season, season_end, season_list) =
                            crate::models::subscription::normalize_season_list(list);
                        sub.season = season;
                        sub.season_end = season_end;
                        sub.season_list = season_list;
                    }
                    season_fields_changed = true;
                }
            }
            if season_fields_changed {
                sub.normalize_season_range();
                let season_range_changed =
                    season_range_before != (sub.season, sub.season_end, sub.season_list.clone());
                if season_range_changed {
                    // 季度范围调整是「暂不转存 → 随时可扩季」的语义：
                    // 已转存/已知证据按「季+集」解析，扩季（如 [1,3] 加上 S2）
                    // 后旧进度仍有效，新季文件会在下次检查中被发现并转存，
                    // 已转存的季不会重复。只重置单季完结目标与完结状态：
                    // 旧 total 属于原范围，多季订阅由证据（而非集数目标）协调完结。
                    sub.total_episode_number = None;
                    sub.completed = false;
                    sub.status = "active".to_string();
                    sub.invalid_since = None;
                    sub.last_error.clear();
                }
            }
            if media_type_changed {
                // 媒体类型变化（剧集 ↔ 电影）语义完全不同，保留历史重置行为。
                reset_progress_for_content_change(sub);
            }
            if let Some(start_episode_number) = req.start_episode_number {
                sub.start_episode_number =
                    normalize_start_episode_number(Some(start_episode_number), &sub.media_type);
            }
            if sub.media_type == "movie" {
                sub.start_episode_number = None;
            }
            if let Some(cloud_type) = requested_cloud_type {
                sub.cloud_type = cloud_type;
            }
            if let Some(tags) = req.tags {
                sub.tags = normalize_tags(tags);
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
            if let Some(total_episode_number) = req.total_episode_number {
                sub.total_episode_number = total_episode_number;
            }
            if let Some(metadata) = req.metadata {
                sub.metadata = metadata
                    .map(|refreshed| merge_refreshed_metadata(sub.metadata.as_ref(), refreshed));
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
                // 多季/跳季订阅没有单一总集数：把 min 季的单季集数写成完结
                // 目标会在其他季集数超过该值时误判完结并封顶转存。
                if sub.is_multi_season() {
                    if sub.total_episode_number.is_none() {
                        sub.total_episode_number = sub.rules.finish_after_episode;
                    }
                } else if let Some(count) =
                    episode_count_for_season(sub.metadata.as_ref(), sub.season)
                {
                    sub.total_episode_number = Some(count);
                } else if sub.total_episode_number.is_none() {
                    sub.total_episode_number = sub.rules.finish_after_episode;
                }
            }
            // 手动标记优先于自动推导：否则「重新追更」会被证据立刻改回已完结。
            match req.completed {
                Some(completed) => apply_manual_completion(sub, completed),
                None => reconcile_completion_status(sub),
            }
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
#[derive(Debug, Deserialize)]
pub(super) struct DeleteSubscriptionQuery {
    #[serde(default)]
    confirm: String,
}

pub(super) async fn delete_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
    Query(query): Query<DeleteSubscriptionQuery>,
) -> Result<impl IntoResponse> {
    if query.confirm != id {
        return Err(AppError::Validation(
            "删除确认参数必须与订阅 ID 一致".to_string(),
        ));
    }
    let deleted = state.store.delete(&id).await?;
    if deleted {
        Ok((StatusCode::NO_CONTENT, ()))
    } else {
        Err(AppError::NotFound("订阅不存在".to_string()))
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for tag in tags {
        let tag: String = tag.trim().chars().take(32).collect();
        if !tag.is_empty() && !result.contains(&tag) {
            result.push(tag);
        }
        if result.len() >= 20 {
            break;
        }
    }
    result
}
