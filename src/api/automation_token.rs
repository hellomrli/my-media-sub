use super::response::ApiResponse;
use crate::{
    error::Result,
    store::{automation_token::TOKEN_SCOPES, AutomationTokenStore},
};
use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
struct RotateRequest {
    scopes: Vec<String>,
    expires_days: Option<u64>,
}
#[derive(Serialize)]
struct RotateResponse {
    token: String,
    status: crate::store::automation_token::AutomationTokenStatus,
}
async fn status(
    State(store): State<Arc<AutomationTokenStore>>,
) -> Json<ApiResponse<crate::store::automation_token::AutomationTokenStatus>> {
    Json(ApiResponse::ok(store.status().await))
}
async fn scopes() -> Json<ApiResponse<Vec<&'static str>>> {
    Json(ApiResponse::ok(TOKEN_SCOPES.to_vec()))
}
async fn rotate(
    State(store): State<Arc<AutomationTokenStore>>,
    Json(req): Json<RotateRequest>,
) -> Result<Json<ApiResponse<RotateResponse>>> {
    let (token, status) = store.rotate(req.scopes, req.expires_days).await?;
    Ok(Json(ApiResponse::ok(RotateResponse { token, status })))
}
async fn revoke(
    State(store): State<Arc<AutomationTokenStore>>,
) -> Result<Json<ApiResponse<crate::store::automation_token::AutomationTokenStatus>>> {
    Ok(Json(ApiResponse::ok(store.revoke().await?)))
}
pub fn routes(store: Arc<AutomationTokenStore>) -> Router {
    Router::new()
        .route(
            "/api/automation-token",
            get(status).post(rotate).delete(revoke),
        )
        .route("/api/automation-token/scopes", get(scopes))
        .with_state(store)
}
