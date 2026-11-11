//! What the anonymous telemetry would send, for the operator to see.

use std::sync::Arc;

use actix_web::{web, HttpResponse};
use serde::Serialize;

use crate::auth::ApiActor;
use crate::error::AppResult;
use crate::models::rbac::{Action, Resource};
use crate::telemetry::{Report, Reporter, TelemetryStatus};

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TelemetryPreview {
    pub enabled: bool,
    /// Why nothing is sent, when `enabled` is false.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub reason: Option<String>,
    /// Where it goes, when `enabled` is true.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub sink: Option<&'static str>,
    /// The next heartbeat, exactly as it would leave. Shown even when
    /// disabled, so the decision can be made on the real thing.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub report: PreviewReport,
}

/// The report with its instance id, which the wire format carries as
/// `distinct_id` rather than as a property.
#[derive(Serialize)]
pub struct PreviewReport {
    pub instance_id: String,
    #[serde(flatten)]
    pub report: Report,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/telemetry/preview",
    operation_id = "telemetry.preview",
    tag = "Telemetry",
    responses(
        (status = 200, description = "What the next heartbeat would carry", body = TelemetryPreview),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn preview(
    status: web::Data<TelemetryStatus>,
    reporter: web::Data<Arc<Reporter>>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Telemetry, Action::Read, None)?;
    let report = reporter.preview().await?;
    let (enabled, reason, sink) = match status.get_ref() {
        TelemetryStatus::Enabled { sink } => (true, None, Some(*sink)),
        TelemetryStatus::Disabled(why) => (false, Some(why.to_string()), None),
    };
    Ok(HttpResponse::Ok().json(TelemetryPreview {
        enabled,
        reason,
        sink,
        report: PreviewReport {
            instance_id: report.instance_id.clone(),
            report,
        },
    }))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/telemetry").route("/preview", web::get().to(preview)));
}
