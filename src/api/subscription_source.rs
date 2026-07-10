//! 订阅换源 API：候选搜索、探测评分、预览、应用、历史与回滚。

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use super::response::ApiResponse as Response;
use crate::app::AppContext;
use crate::clients::quark::QuarkShareProbe;
use crate::error::{AppError, Result};
use crate::models::subscription::{ProbeResult, SourceCandidate, SourceSwitchHistoryItem};
use crate::services::notification::add_notification;
use crate::services::subscription_source_switch::{
    SourceSwitchPreview, SourceSwitchRollbackResult, SubscriptionSourceSwitchService,
};

/// 获取订阅的换源候选列表
pub async fn get_source_candidates(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
) -> Result<Json<Response<Vec<SourceCandidate>>>> {
    let sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
    Ok(Json(Response::ok(sub.source_candidates)))
}

#[derive(Debug, Deserialize)]
pub struct CandidateRequest {
    candidate_id: String,
}

/// 探测候选项并持久化权威评分。为兼容旧调用，响应仍返回 ProbeResult。
pub async fn probe_candidate(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
    Json(req): Json<CandidateRequest>,
) -> Result<Json<Response<ProbeResult>>> {
    let settings = ctx.settings_store.get().await;
    let sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
    let candidate = sub
        .source_candidates
        .iter()
        .find(|candidate| candidate.id == req.candidate_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound("候选项不存在".to_string()))?;
    let service = source_switch_service(&settings.pansou_api_url, &settings.quark_cookie);
    let scored = service
        .probe_and_score_candidate(
            &candidate,
            &settings.quark_cookie,
            chrono::Utc::now().timestamp_millis(),
        )
        .await?;
    let probe = scored.probe_info.clone().unwrap_or(ProbeResult {
        ok: false,
        state: "failed".to_string(),
        message: "候选探测没有返回结果".to_string(),
        files: vec![],
    });
    persist_scored_candidate(&ctx, &subscription_id, scored).await?;
    Ok(Json(Response::ok(probe)))
}

/// 探测候选并返回应用前差异、安全条件和自动应用判定。
pub async fn preview_source_switch(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
    Json(req): Json<CandidateRequest>,
) -> Result<Json<Response<SourceSwitchPreview>>> {
    let settings = ctx.settings_store.get().await;
    let sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
    let candidate = sub
        .source_candidates
        .iter()
        .find(|candidate| candidate.id == req.candidate_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound("候选项不存在".to_string()))?;
    let service = source_switch_service(&settings.pansou_api_url, &settings.quark_cookie);
    let scored = service
        .probe_and_score_candidate(
            &candidate,
            &settings.quark_cookie,
            chrono::Utc::now().timestamp_millis(),
        )
        .await?;
    persist_scored_candidate(&ctx, &subscription_id, scored.clone()).await?;
    let latest = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
    let preview = service.preview_candidate(&latest, scored, &settings, crate::utils::unix_now());
    Ok(Json(Response::ok(preview)))
}

#[derive(Debug, Deserialize)]
pub struct ApplySourceSwitchRequest {
    candidate_id: String,
}

#[derive(Debug, Serialize)]
pub struct ApplySourceSwitchResponse {
    success: bool,
    message: String,
    preview: SourceSwitchPreview,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_error: Option<String>,
}

/// 手动应用换源。API 会重新探测并强制季度、进度覆盖和历史失败安全条件。
pub async fn apply_source_switch(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
    Json(req): Json<ApplySourceSwitchRequest>,
) -> Result<Json<Response<ApplySourceSwitchResponse>>> {
    let settings = ctx.settings_store.get().await;
    let mut sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
    let candidate = sub
        .source_candidates
        .iter()
        .find(|candidate| candidate.id == req.candidate_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound("候选项不存在".to_string()))?;
    let service = source_switch_service(&settings.pansou_api_url, &settings.quark_cookie);
    let scored = service
        .probe_and_score_candidate(
            &candidate,
            &settings.quark_cookie,
            chrono::Utc::now().timestamp_millis(),
        )
        .await?;
    if let Some(candidate) = sub
        .source_candidates
        .iter_mut()
        .find(|candidate| candidate.id == scored.id)
    {
        *candidate = scored.clone();
    }
    let preview = service.preview_candidate(&sub, scored, &settings, crate::utils::unix_now());
    if !preview.can_apply {
        return Err(AppError::Validation(format!(
            "候选未通过安全检查：{}",
            preview.warnings.join("；")
        )));
    }

    service.apply_source_switch_with_audit(
        &mut sub,
        &req.candidate_id,
        false,
        "用户确认预览后手动应用",
    )?;
    sub.updated_at = crate::utils::unix_now();
    persist_subscription(&ctx, &sub).await?;

    let mut meta = HashMap::new();
    meta.insert("subscription_id".to_string(), Value::String(sub.id.clone()));
    meta.insert(
        "candidate_id".to_string(),
        Value::String(req.candidate_id.clone()),
    );
    meta.insert(
        "quality_score".to_string(),
        Value::from(preview.candidate.quality.score),
    );
    meta.insert("automatic".to_string(), Value::Bool(false));
    add_notification(
        &ctx.notification_store,
        "success",
        "subscription_source_switched",
        "换源成功".to_string(),
        format!("订阅「{}」已成功换源", sub.title),
        meta,
    )
    .await?;

    let (check_summary, check_error) = immediate_check(&ctx, &sub.id, &settings.quark_cookie).await;
    update_latest_history_check(&ctx, &sub.id, check_error.as_deref()).await?;
    Ok(Json(Response::ok(ApplySourceSwitchResponse {
        success: true,
        message: if check_error.is_none() {
            "换源成功，已立即检查订阅".to_string()
        } else {
            "换源成功，立即检查未完成".to_string()
        },
        preview,
        check_summary,
        check_error,
    })))
}

/// 手动触发换源搜索
pub async fn trigger_source_search(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
) -> Result<Json<Response<Vec<SourceCandidate>>>> {
    let settings = ctx.settings_store.get().await;
    let sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
    let service = source_switch_service(&settings.pansou_api_url, &settings.quark_cookie);
    let candidates = service.search_source_candidates(&sub).await?;
    let updated_candidates = candidates.clone();
    ctx.subscription_store
        .update(&sub.id, |subscription| {
            subscription.source_candidates = updated_candidates;
            subscription.last_source_search_time = Some(crate::utils::unix_now());
        })
        .await?;
    Ok(Json(Response::ok(candidates)))
}

pub async fn get_source_switch_history(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
) -> Result<Json<Response<Vec<SourceSwitchHistoryItem>>>> {
    let sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
    Ok(Json(Response::ok(sub.source_switch_history)))
}

pub async fn rollback_source_switch(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
) -> Result<Json<Response<SourceSwitchRollbackResult>>> {
    let settings = ctx.settings_store.get().await;
    let mut sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;
    let service = source_switch_service(&settings.pansou_api_url, &settings.quark_cookie);
    let result = service.rollback_last_source(&mut sub)?;
    sub.updated_at = crate::utils::unix_now();
    persist_subscription(&ctx, &sub).await?;

    let mut meta = HashMap::new();
    meta.insert("subscription_id".to_string(), Value::String(sub.id.clone()));
    meta.insert(
        "restored_url".to_string(),
        Value::String(result.restored_url.clone()),
    );
    add_notification(
        &ctx.notification_store,
        "warning",
        "subscription_source_rolled_back",
        "来源已回滚".to_string(),
        format!("订阅「{}」已回滚到上一来源", sub.title),
        meta,
    )
    .await?;
    let _ = immediate_check(&ctx, &sub.id, &settings.quark_cookie).await;
    Ok(Json(Response::ok(result)))
}

pub fn routes(ctx: Arc<AppContext>) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route(
            "/api/subscriptions/{id}/source-candidates",
            get(get_source_candidates),
        )
        .route(
            "/api/subscriptions/{id}/source-candidates/probe",
            post(probe_candidate),
        )
        .route(
            "/api/subscriptions/{id}/source-candidates/preview",
            post(preview_source_switch),
        )
        .route(
            "/api/subscriptions/{id}/source-candidates/apply",
            post(apply_source_switch),
        )
        .route(
            "/api/subscriptions/{id}/source-candidates/search",
            post(trigger_source_search),
        )
        .route(
            "/api/subscriptions/{id}/source-history",
            get(get_source_switch_history),
        )
        .route(
            "/api/subscriptions/{id}/source-history/rollback",
            post(rollback_source_switch),
        )
        .with_state(ctx)
}

fn source_switch_service(pansou_url: &str, cookie: &str) -> SubscriptionSourceSwitchService {
    SubscriptionSourceSwitchService::with_pansou_api_url(
        Arc::new(QuarkShareProbe::new(cookie)),
        pansou_api_url_option(pansou_url),
    )
}

async fn persist_scored_candidate(
    ctx: &Arc<AppContext>,
    subscription_id: &str,
    scored: SourceCandidate,
) -> Result<()> {
    ctx.subscription_store
        .update(subscription_id, |subscription| {
            if let Some(candidate) = subscription
                .source_candidates
                .iter_mut()
                .find(|candidate| candidate.id == scored.id)
            {
                *candidate = scored;
            }
        })
        .await?;
    Ok(())
}

async fn persist_subscription(
    ctx: &Arc<AppContext>,
    subscription: &crate::models::Subscription,
) -> Result<()> {
    let snapshot = subscription.clone();
    ctx.subscription_store
        .update(&subscription.id, |current| *current = snapshot)
        .await?;
    Ok(())
}

async fn immediate_check(
    ctx: &Arc<AppContext>,
    subscription_id: &str,
    cookie: &str,
) -> (Option<String>, Option<String>) {
    if cookie.trim().is_empty() {
        return (
            None,
            Some("未配置夸克 Cookie，已跳过换源后的立即检查".to_string()),
        );
    }
    match ctx
        .check_service
        .check_subscription_with_options(subscription_id, cookie, false)
        .await
    {
        Ok(result) => (Some(result.summary), None),
        Err(error) => (None, Some(error.to_string())),
    }
}

async fn update_latest_history_check(
    ctx: &Arc<AppContext>,
    subscription_id: &str,
    error: Option<&str>,
) -> Result<()> {
    ctx.subscription_store
        .update(subscription_id, |subscription| {
            if let Some(history) = subscription
                .source_switch_history
                .iter_mut()
                .find(|history| history.status == "succeeded")
            {
                history.error = error.unwrap_or_default().to_string();
            }
        })
        .await?;
    Ok(())
}

fn pansou_api_url_option(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
