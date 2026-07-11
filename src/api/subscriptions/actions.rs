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

/// STRM 生成响应
#[derive(Serialize)]
struct GenerateStrmResponse {
    subscription_id: String,
    generated_count: usize,
    skipped_count: usize,
    output_dir: String,
    files: Vec<GenerateStrmFile>,
}

#[derive(Serialize)]
struct GenerateStrmFile {
    fid: String,
    file_name: String,
    strm_path: String,
    url: String,
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

/// 按订阅目标目录中的已有视频补齐 STRM 文件
pub(super) async fn audit_existing_strm_files(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    Ok(Json(Response::ok(
        state
            .transfer_service
            .audit_existing_strm_files(&id)
            .await?,
    )))
}

pub(super) async fn generate_existing_strm_files(
    State(state): State<Arc<SubscriptionState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse> {
    let result = state
        .transfer_service
        .generate_existing_strm_files(&id)
        .await?;

    Ok(Json(Response::ok(GenerateStrmResponse {
        subscription_id: id,
        generated_count: result.generated_count,
        skipped_count: result.skipped_count,
        output_dir: result.output_dir.display().to_string(),
        files: result
            .files
            .into_iter()
            .map(|file| GenerateStrmFile {
                fid: file.fid,
                file_name: file.file_name,
                strm_path: file.strm_path.display().to_string(),
                url: file.url,
            })
            .collect(),
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
