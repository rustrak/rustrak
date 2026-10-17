use actix_web::{web, HttpResponse};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::auth::ApiActor;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::scheduler::schedule::{parse_timezone, Schedule};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

/// Body of `POST /api/schedules/preview`.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PreviewSchedule {
    pub schedule: Schedule,
    /// IANA zone the schedule is read in. Defaults to UTC.
    pub timezone: Option<String>,
}

/// When a schedule runs next, computed by the code that runs it: monitors
/// expect their check-ins and scheduled jobs fire at exactly these instants.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SchedulePreview {
    pub next_runs: Vec<chrono::DateTime<chrono::Utc>>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/schedules/preview",
    tag = "Schedules",
    request_body = PreviewSchedule,
    responses(
        (status = 200, description = "The next five runs", body = SchedulePreview),
        (status = 400, description = "Invalid schedule or timezone", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/schedules/preview
///
/// Validates a schedule and lists its next five runs from now, so no client
/// ever parses a crontab itself.
pub async fn preview_schedule(
    body: web::Json<PreviewSchedule>,
    _actor: ApiActor,
) -> AppResult<HttpResponse> {
    let PreviewSchedule { schedule, timezone } = body.into_inner();
    schedule.validate().map_err(|message| {
        AppError::Validation(format!("Invalid schedule: {message}"))
            .with_field("schedule", FieldErrorCode::Invalid)
    })?;
    let tz = match timezone {
        Some(name) => parse_timezone(&name).map_err(|message| {
            AppError::Validation(message).with_field("timezone", FieldErrorCode::Invalid)
        })?,
        None => Tz::UTC,
    };
    Ok(HttpResponse::Ok().json(SchedulePreview {
        next_runs: schedule.upcoming(chrono::Utc::now(), tz, 5),
    }))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(paths(preview_schedule))]
pub struct SchedulesApi;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/schedules/preview", web::post().to(preview_schedule));
}
