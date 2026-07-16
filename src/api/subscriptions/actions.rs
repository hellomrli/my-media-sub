use super::*;

/// 检查响应
#[derive(Serialize)]
struct CheckResponse {
    subscription_id: String,
    new_files: Vec<String>,
    new_episodes: Vec<i32>,
    details: CheckDetails,
    became_invalid: bool,
    became_completed: bool,
    summary: String,
}

/// 重命名修复响应
#[derive(Serialize)]
struct RenameExistingResponse {
    subscription_id: String,
    renamed_count: usize,
}

/// 检查单个订阅
pub(super) async fn check_subscription(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
    body: Option<Json<CheckSubscriptionRequest>>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie;

    if cookie.is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }

    let force_transfer = body.map(|Json(req)| req.force_transfer).unwrap_or(false);
    let result = state
        .check_service
        .check_subscription_with_options(&id, &cookie, force_transfer)
        .await?;

    Ok(Json(Response::ok(CheckResponse {
        subscription_id: result.subscription_id,
        new_files: result.new_files,
        new_episodes: result.new_episodes,
        details: result.details,
        became_invalid: result.became_invalid,
        became_completed: result.became_completed,
        summary: result.summary,
    })))
}

/// 按订阅规则重命名目标目录中的已有文件
pub(super) async fn rename_existing_files(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let renamed_count = state.transfer_service.rename_existing_files(&id).await?;

    Ok(Json(Response::ok(RenameExistingResponse {
        subscription_id: id,
        renamed_count,
    })))
}

/// 检查所有订阅
pub(super) async fn check_all_subscriptions(
    State(state): State<Arc<SubscriptionState>>,
) -> Result<impl IntoResponse> {
    let settings = state.settings_store.get().await;
    let cookie = settings.quark_cookie;

    if cookie.is_empty() {
        return Err(AppError::Validation("未配置夸克 Cookie".to_string()));
    }

    let results = state.check_service.check_all_subscriptions(&cookie).await?;

    let responses: Vec<CheckResponse> = results
        .into_iter()
        .map(|r| CheckResponse {
            subscription_id: r.subscription_id,
            new_files: r.new_files,
            new_episodes: r.new_episodes,
            details: r.details,
            became_invalid: r.became_invalid,
            became_completed: r.became_completed,
            summary: r.summary,
        })
        .collect();

    Ok(Json(Response::ok(responses)))
}
