use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::app::AppContext;
use crate::clients::quark::QuarkShareProbe;
use crate::error::{AppError, Result};
use crate::models::subscription::{ProbeResult, SourceCandidate};
use crate::services::notification::add_notification;
use crate::services::subscription_source_switch::SubscriptionSourceSwitchService;

/// 获取订阅的换源候选列表
pub async fn get_source_candidates(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
) -> Result<Json<Vec<SourceCandidate>>> {
    let sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

    Ok(Json(sub.source_candidates))
}

/// 探测候选项详情
#[derive(Debug, Deserialize)]
pub struct ProbeCandidateRequest {
    candidate_id: String,
}

pub async fn probe_candidate(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
    Json(req): Json<ProbeCandidateRequest>,
) -> Result<Json<ProbeResult>> {
    let settings = ctx.settings_store.get().await;
    let cookie = &settings.quark_cookie;

    let sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

    let candidate = sub
        .source_candidates
        .iter()
        .find(|c| c.id == req.candidate_id)
        .ok_or_else(|| AppError::NotFound("候选项不存在".to_string()))?;

    let quark_probe = Arc::new(QuarkShareProbe::new(cookie.clone()));
    let service = SubscriptionSourceSwitchService::new(quark_probe);
    let probe_result = service.probe_candidate(candidate, cookie).await?;

    Ok(Json(probe_result))
}

/// 应用换源
#[derive(Debug, Deserialize)]
pub struct ApplySourceSwitchRequest {
    candidate_id: String,
}

#[derive(Debug, Serialize)]
pub struct ApplySourceSwitchResponse {
    success: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_error: Option<String>,
}

pub async fn apply_source_switch(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
    Json(req): Json<ApplySourceSwitchRequest>,
) -> Result<Json<ApplySourceSwitchResponse>> {
    let settings = ctx.settings_store.get().await;
    let cookie = &settings.quark_cookie;

    let mut sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

    let quark_probe = Arc::new(QuarkShareProbe::new(cookie.clone()));
    let service = SubscriptionSourceSwitchService::new(quark_probe);
    service.apply_source_switch(&mut sub, &req.candidate_id)?;

    sub.updated_at = crate::utils::unix_now();

    // 使用更新方法
    let sub_clone = sub.clone();
    ctx.subscription_store
        .update(&sub.id, |s| {
            *s = sub_clone;
        })
        .await?;

    // 记录通知
    let message = format!("订阅「{}」已成功换源", sub.title);
    let mut meta: HashMap<String, Value> = HashMap::new();
    meta.insert("subscription_id".to_string(), Value::String(sub.id.clone()));

    add_notification(
        &ctx.notification_store,
        "success",
        "subscription_source_switched",
        "换源成功".to_string(),
        message.clone(),
        meta,
    )
    .await?;

    let (check_summary, check_error) = if cookie.trim().is_empty() {
        (
            None,
            Some("未配置夸克 Cookie，已跳过换源后的立即检查".to_string()),
        )
    } else {
        match ctx
            .check_service
            .check_subscription_with_options(&sub.id, cookie, false)
            .await
        {
            Ok(result) => (Some(result.summary), None),
            Err(error) => (None, Some(error.to_string())),
        }
    };

    Ok(Json(ApplySourceSwitchResponse {
        success: true,
        message: if check_error.is_none() {
            "换源成功，已立即检查订阅".to_string()
        } else {
            "换源成功，立即检查未完成".to_string()
        },
        check_summary,
        check_error,
    }))
}

/// 手动触发换源搜索
pub async fn trigger_source_search(
    State(ctx): State<Arc<AppContext>>,
    Path(subscription_id): Path<String>,
) -> Result<Json<Vec<SourceCandidate>>> {
    let settings = ctx.settings_store.get().await;
    let cookie = &settings.quark_cookie;
    let pansou_api_url = pansou_api_url_option(&settings.pansou_api_url);

    let sub = ctx
        .subscription_store
        .get(&subscription_id)
        .await
        .ok_or_else(|| AppError::NotFound("订阅不存在".to_string()))?;

    let quark_probe = Arc::new(QuarkShareProbe::new(cookie.clone()));
    let service = SubscriptionSourceSwitchService::with_pansou_api_url(quark_probe, pansou_api_url);
    let candidates = service.search_source_candidates(&sub).await?;

    // 更新订阅
    let updated_candidates = candidates.clone();
    ctx.subscription_store
        .update(&sub.id, |s| {
            s.source_candidates = updated_candidates;
            s.last_source_search_time = Some(crate::utils::unix_now());
        })
        .await?;

    Ok(Json(candidates))
}

/// 注册路由
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
            "/api/subscriptions/{id}/source-candidates/apply",
            post(apply_source_switch),
        )
        .route(
            "/api/subscriptions/{id}/source-candidates/search",
            post(trigger_source_search),
        )
        .with_state(ctx)
}

fn pansou_api_url_option(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
