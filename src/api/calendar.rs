use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use chrono::{NaiveDate, Utc};
use serde::Deserialize;

use super::response::ApiResponse as Response;
use crate::error::{AppError, Result};
use crate::jobs::JobStore;
use crate::models::{CalendarStatus, MediaCalendar};
use crate::services::media_calendar::{
    build_media_calendar, natural_week, shanghai_offset, MediaCalendarQuery,
    MAX_CALENDAR_RANGE_DAYS,
};
use crate::store::{AutomationEventStore, NotificationStore, SettingsStore, SubscriptionStore};

pub struct CalendarState {
    pub subscription_store: Arc<SubscriptionStore>,
    pub settings_store: Arc<SettingsStore>,
    pub job_store: Arc<JobStore>,
    pub notification_store: Arc<NotificationStore>,
    pub automation_event_store: Arc<AutomationEventStore>,
}

#[derive(Debug, Default, Deserialize)]
struct CalendarQueryParams {
    from: Option<String>,
    to: Option<String>,
    status: Option<String>,
    media_type: Option<String>,
    subscription: Option<String>,
}

async fn get_calendar(
    State(state): State<Arc<CalendarState>>,
    Query(params): Query<CalendarQueryParams>,
) -> Result<Json<Response<MediaCalendar>>> {
    let today = Utc::now().with_timezone(&shanghai_offset()).date_naive();
    let query = parse_query(params, today)?;
    let (subscriptions, settings, jobs, notifications, events) = tokio::join!(
        state.subscription_store.list(),
        state.settings_store.get(),
        state.job_store.list(),
        state.notification_store.list(true),
        state.automation_event_store.list(5_000),
    );
    let calendar = build_media_calendar(
        subscriptions,
        &settings,
        &jobs,
        &notifications,
        &events,
        &query,
    );
    Ok(Json(Response::ok(calendar)))
}

fn parse_query(params: CalendarQueryParams, today: NaiveDate) -> Result<MediaCalendarQuery> {
    let (default_from, default_to) = natural_week(today);
    let from = parse_optional_date(params.from.as_deref(), "from")?.unwrap_or(default_from);
    let to = parse_optional_date(params.to.as_deref(), "to")?.unwrap_or(default_to);
    if from > to {
        return Err(AppError::Validation(
            "日历查询的 from 不能晚于 to".to_string(),
        ));
    }
    if (to - from).num_days() > MAX_CALENDAR_RANGE_DAYS {
        return Err(AppError::Validation(format!(
            "日历查询范围不能超过 {} 天",
            MAX_CALENDAR_RANGE_DAYS + 1
        )));
    }

    let status = params.status.as_deref().map(parse_status).transpose()?;
    let media_type = normalize_filter(params.media_type);
    let subscription_id = normalize_filter(params.subscription);

    Ok(MediaCalendarQuery {
        from,
        to,
        today,
        status,
        media_type,
        subscription_id,
    })
}

fn parse_optional_date(value: Option<&str>, field: &str) -> Result<Option<NaiveDate>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(Some)
        .map_err(|_| AppError::Validation(format!("{} 必须使用 YYYY-MM-DD 格式", field)))
}

fn parse_status(value: &str) -> Result<CalendarStatus> {
    match value.trim().to_ascii_lowercase().as_str() {
        "today" => Ok(CalendarStatus::Today),
        "this_week" => Ok(CalendarStatus::ThisWeek),
        "aired_undiscovered" => Ok(CalendarStatus::AiredUndiscovered),
        "discovered_pending_transfer" => Ok(CalendarStatus::DiscoveredPendingTransfer),
        "transferred_pending_download" => Ok(CalendarStatus::TransferredPendingDownload),
        "completed_missing" => Ok(CalendarStatus::CompletedMissing),
        "ready" => Ok(CalendarStatus::Ready),
        "scheduled" => Ok(CalendarStatus::Scheduled),
        "unknown_schedule" => Ok(CalendarStatus::UnknownSchedule),
        _ => Err(AppError::Validation(
            "status 必须是 today、this_week、aired_undiscovered、discovered_pending_transfer、transferred_pending_download、completed_missing、ready、scheduled 或 unknown_schedule"
                .to_string(),
        )),
    }
}

fn normalize_filter(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn routes(
    subscription_store: Arc<SubscriptionStore>,
    settings_store: Arc<SettingsStore>,
    job_store: Arc<JobStore>,
    notification_store: Arc<NotificationStore>,
    automation_event_store: Arc<AutomationEventStore>,
) -> Router {
    let state = Arc::new(CalendarState {
        subscription_store,
        settings_store,
        job_store,
        notification_store,
        automation_event_store,
    });
    Router::new()
        .route("/api/calendar", get(get_calendar))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn defaults_to_current_natural_week() {
        let query = parse_query(CalendarQueryParams::default(), date("2027-01-01")).unwrap();
        assert_eq!(query.from.to_string(), "2026-12-28");
        assert_eq!(query.to.to_string(), "2027-01-03");
    }

    #[test]
    fn rejects_invalid_ranges_and_statuses() {
        let reversed = CalendarQueryParams {
            from: Some("2026-07-10".to_string()),
            to: Some("2026-07-01".to_string()),
            ..Default::default()
        };
        assert!(parse_query(reversed, date("2026-07-10")).is_err());

        let invalid_status = CalendarQueryParams {
            status: Some("missing-ish".to_string()),
            ..Default::default()
        };
        assert!(parse_query(invalid_status, date("2026-07-10")).is_err());
    }
}
