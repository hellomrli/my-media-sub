use axum::{routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use super::response::ApiResponse as Response;
use crate::error::Result;
use crate::models::subscription::{normalize_season_bounds, parse_season_spec_list};
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
    /// 跳季集合输入（优先于 season_spec / season..season_end）
    #[serde(default)]
    pub season_list: Option<Vec<i32>>,
}

#[derive(Debug, Serialize)]
pub struct ParseSeasonResponse {
    pub season: i32,
    pub season_end: Option<i32>,
    /// 规范化后的季度集合；区间语义时为 None
    pub season_list: Option<Vec<i32>>,
    /// 覆盖的全部季号（升序），供前端渲染勾选
    pub seasons: Vec<i32>,
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
    let (season, season_end, season_list) = if !req.season_spec.trim().is_empty() {
        crate::models::subscription::normalize_season_list(parse_season_spec_list(&req.season_spec))
    } else if let Some(list) = req.season_list.clone().filter(|list| !list.is_empty()) {
        crate::models::subscription::normalize_season_list(list)
    } else {
        let (season, season_end) = normalize_season_bounds(req.season.unwrap_or(1), req.season_end);
        (season, season_end, None)
    };
    let seasons = match &season_list {
        Some(list) => list.clone(),
        None => (season..=season_end.unwrap_or(season)).collect(),
    };
    let multi_season = seasons.len() > 1;
    let label = match &season_list {
        Some(list) => {
            let numbers = list
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("第 {numbers} 季")
        }
        None => {
            if multi_season {
                format!("第 {season}-{} 季", season_end.unwrap_or(season))
            } else {
                format!("第 {season} 季")
            }
        }
    };
    let season_spec = match &season_list {
        Some(list) => list
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
        None => {
            if multi_season {
                format!("{season}-{}", season_end.unwrap_or(season))
            } else {
                season.to_string()
            }
        }
    };
    Ok(Json(Response::ok(ParseSeasonResponse {
        season,
        season_end,
        season_list,
        seasons,
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
