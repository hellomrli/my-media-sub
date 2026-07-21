use axum::{routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use super::response::ApiResponse as Response;
use crate::error::Result;
use crate::models::subscription::{normalize_season_bounds, parse_season_spec};
use crate::services::title_normalize::normalize_title_detailed;

#[derive(Debug, Deserialize)]
pub struct NormalizeTitleRequest {
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct NormalizeTitleResponse {
    pub original: String,
    pub normalized: String,
    pub changed: bool,
}

#[derive(Debug, Deserialize)]
pub struct ParseSeasonRequest {
    #[serde(default)]
    pub season_spec: String,
    #[serde(default)]
    pub season: Option<i32>,
    #[serde(default)]
    pub season_end: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct ParseSeasonResponse {
    pub season: i32,
    pub season_end: Option<i32>,
    pub multi_season: bool,
    pub label: String,
    pub season_spec: String,
}

async fn normalize_title(
    Json(req): Json<NormalizeTitleRequest>,
) -> Result<Json<Response<NormalizeTitleResponse>>> {
    let detailed = normalize_title_detailed(&req.title);
    Ok(Json(Response::ok(NormalizeTitleResponse {
        changed: detailed.normalized != detailed.original,
        original: detailed.original,
        normalized: detailed.normalized,
    })))
}

async fn parse_season(
    Json(req): Json<ParseSeasonRequest>,
) -> Result<Json<Response<ParseSeasonResponse>>> {
    let (season, season_end) = if !req.season_spec.trim().is_empty() {
        parse_season_spec(&req.season_spec)
    } else {
        normalize_season_bounds(req.season.unwrap_or(1), req.season_end)
    };
    let multi_season = season_end.is_some_and(|end| end > season);
    let label = if multi_season {
        format!("第 {season}-{} 季", season_end.unwrap_or(season))
    } else {
        format!("第 {season} 季")
    };
    let season_spec = if multi_season {
        format!("{season}-{}", season_end.unwrap_or(season))
    } else {
        season.to_string()
    };
    Ok(Json(Response::ok(ParseSeasonResponse {
        season,
        season_end,
        multi_season,
        label,
        season_spec,
    })))
}

pub fn routes() -> Router {
    Router::new()
        .route("/api/utils/normalize-title", post(normalize_title))
        .route("/api/utils/parse-season", post(parse_season))
}
