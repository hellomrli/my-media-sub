use super::*;

/// 获取单个订阅的剧集、流水线与活动聚合状态
pub(super) async fn get_subscription_status(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<Json<Response<SubscriptionDetail>>> {
    let subscription = state
        .store
        .get(&id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
    let settings = state.settings_store.get().await;
    let jobs = state.job_store.list().await;
    let notifications = state.notification_store.list(true).await;
    let events = state
        .automation_event_store
        .list_by_subscription(&id, 1_000)
        .await;
    let detail = build_subscription_detail(subscription, &settings, &jobs, &notifications, &events);
    Ok(Json(Response::ok(detail)))
}
