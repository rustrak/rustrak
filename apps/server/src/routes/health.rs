use actix_web::{http::StatusCode, web, HttpResponse};
use serde::Serialize;

use crate::auth::ApiActor;
use crate::db::{self, DbPool};

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LivenessResponse {
    status: &'static str,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadinessResponse {
    status: &'static str,
    checks: ReadinessChecks,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReadinessChecks {
    database: &'static str,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VersionResponse {
    version: &'static str,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/health",
    operation_id = "health.liveness",
    tag = "Health",
    responses(
        (status = 200, description = "Server is alive", body = LivenessResponse),
    ),
))]
/// Liveness check - is the process running?
/// Returns 200 if the server is alive.
pub async fn liveness() -> HttpResponse {
    HttpResponse::Ok().json(LivenessResponse { status: "ok" })
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/health/version",
    operation_id = "health.version",
    tag = "Health",
    responses(
        (status = 200, description = "Server version info", body = VersionResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = []), ("session_cookie" = [])),
))]
/// Version check - returns the server version compiled from Cargo.toml.
///
/// Requires a session cookie or a Bearer token, unlike its neighbours in this
/// scope. `/health` and `/health/ready` answer "is this process up", which a
/// probe has to be able to ask without credentials; this one answers "which
/// build is this", which is the first thing worth knowing about a host you
/// intend to attack.
pub async fn version(_actor: ApiActor) -> HttpResponse {
    HttpResponse::Ok().json(VersionResponse {
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/health")
            .route("", web::get().to(liveness))
            .route("/version", web::get().to(version))
            .route("/ready", web::get().to(readiness)),
    );
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/health/ready",
    operation_id = "health.readiness",
    tag = "Health",
    responses(
        (status = 200, description = "Service is ready", body = ReadinessResponse),
        (status = 503, description = "Service unavailable", body = ReadinessResponse),
    ),
))]
/// Readiness check - is the service ready to handle requests?
/// Returns 200 if all dependencies are available, 503 otherwise.
pub async fn readiness(pool: web::Data<DbPool>) -> HttpResponse {
    let db_healthy = db::health_check(pool.get_ref()).await;

    let (status, db_status, http_status) = if db_healthy {
        ("ready", "ok", StatusCode::OK)
    } else {
        ("not_ready", "error", StatusCode::SERVICE_UNAVAILABLE)
    };

    let response = ReadinessResponse {
        status,
        checks: ReadinessChecks {
            database: db_status,
        },
    };

    HttpResponse::build(http_status).json(response)
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(liveness, version, readiness),
    components(schemas(LivenessResponse, VersionResponse, ReadinessResponse, ReadinessChecks))
)]
pub struct HealthApi;
