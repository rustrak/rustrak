//! The MCP server at `/mcp`, and the instance switch that opens it.
//!
//! Stateless Streamable HTTP (protocol `2026-07-28`): every call is one `POST`
//! with nothing kept between them, so the endpoint costs nothing while idle
//! and needs no session store. `rmcp` speaks the protocol; this module adapts
//! Actix to its `http` 1.x types and gives each request a handler that already
//! knows who is calling. The tools themselves live in `crate::mcp`.

use std::convert::Infallible;
use std::sync::Arc;

use actix_web::http::header::{HeaderName, HeaderValue, AUTHORIZATION, HOST, WWW_AUTHENTICATE};
use actix_web::http::StatusCode;
use actix_web::{web, HttpRequest, HttpResponse};
use futures_util::StreamExt;
use http_body_util::{BodyStream, Full};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde::{Deserialize, Serialize};

use crate::auth::extractors::api_token;
use crate::auth::ApiActor;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::mcp::RustrakMcp;
use crate::models::rbac::{Action, Resource};
use crate::routes::oauth;
use crate::services::oauth::{OAuthService, ACCESS_PREFIX};

/// Turns `/mcp` on or off: the body of `PUT /api/settings/mcp`.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct McpSettings {
    pub enabled: bool,
}

/// Whether `/mcp` answers, and the URL to give an assistant.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct McpStatus {
    pub enabled: bool,
    /// The configured endpoint (`PUBLIC_URL` + `/mcp`), which OAuth checks
    /// the resource against; the browser's own origin may differ. `null`
    /// without `PUBLIC_URL`, when signing in is off.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub url: Option<String>,
}

impl McpStatus {
    fn new(enabled: bool, config: &Config) -> Self {
        Self {
            enabled,
            url: oauth::issuer(config).map(oauth::mcp_resource),
        }
    }
}

async fn is_enabled(pool: &DbPool) -> AppResult<bool> {
    Ok(
        sqlx::query_scalar("SELECT mcp_enabled FROM installation WHERE id = 1")
            .fetch_one(pool)
            .await?,
    )
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/settings/mcp",
    operation_id = "settings.getMcp",
    tag = "Settings",
    responses(
        (status = 200, description = "Whether the MCP endpoint is on, and its URL", body = McpStatus),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Whether the MCP endpoint at `/mcp` is on. Open to every member: it is what
/// they point their own assistant at.
pub async fn get_settings(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    _actor: ApiActor,
) -> AppResult<HttpResponse> {
    let enabled = is_enabled(pool.get_ref()).await?;
    Ok(HttpResponse::Ok().json(McpStatus::new(enabled, &config)))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/api/settings/mcp",
    operation_id = "settings.updateMcp",
    tag = "Settings",
    request_body = McpSettings,
    responses(
        (status = 200, description = "The MCP endpoint's new state, and its URL", body = McpStatus),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Missing mcp:update", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Turns the MCP endpoint at `/mcp` on or off (needs `mcp:update`).
pub async fn update_settings(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    actor: ApiActor,
    body: web::Json<McpSettings>,
) -> AppResult<HttpResponse> {
    actor.require(Resource::Mcp, Action::Update, None)?;
    sqlx::query("UPDATE installation SET mcp_enabled = $1 WHERE id = 1")
        .bind(body.enabled)
        .execute(pool.get_ref())
        .await?;
    Ok(HttpResponse::Ok().json(McpStatus::new(body.enabled, &config)))
}

/// Who is calling `/mcp`: an OAuth access token issued for it, or an API
/// token. `None` when neither checks out. Bearer only: an MCP client has no
/// dashboard cookie, and refusing one keeps a browser tab from calling tools
/// on its user's behalf.
async fn caller(
    req: &HttpRequest,
    pool: &DbPool,
    resource: Option<&str>,
) -> AppResult<Option<ApiActor>> {
    let Some(token) = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    else {
        return Ok(None);
    };
    // An OAuth grant acts as its user; an API token is narrowed to the scope
    // it was created with.
    let actor = if token.starts_with(ACCESS_PREFIX) {
        // Without an issuer no OAuth token was ever valid here.
        let Some(resource) = resource else {
            return Ok(None);
        };
        match OAuthService::authenticate(pool, token.trim(), resource).await? {
            Some(user_id) => ApiActor::for_token(pool, Some(user_id)).await,
            None => return Ok(None),
        }
    } else {
        match api_token(req, pool).await {
            Ok(token) => ApiActor::for_api_token(pool, &token).await,
            Err(e) => Err(e),
        }
    };
    match actor {
        Ok(actor) => Ok(Some(actor)),
        Err(e) if matches!(e.kind(), AppError::Unauthorized(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// `/mcp`: authenticate, then hand the request to `rmcp`.
pub async fn serve(
    req: HttpRequest,
    body: web::Bytes,
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
) -> AppResult<HttpResponse> {
    if !is_enabled(pool.get_ref()).await? {
        return Err(AppError::NotFound("Not found".to_string()));
    }
    let base = oauth::issuer(&config);
    let resource = base.map(oauth::mcp_resource);
    let Some(actor) = caller(&req, pool.get_ref(), resource.as_deref()).await? else {
        // RFC 9728: tells an OAuth client where to start authorizing, when
        // there is an issuer to send it to.
        let challenge = match base {
            Some(base) => format!(
                "Bearer resource_metadata=\"{}\"",
                oauth::resource_metadata_url(base)
            ),
            None => "Bearer".to_string(),
        };
        return Ok(HttpResponse::Unauthorized()
            .insert_header((WWW_AUTHENTICATE, challenge))
            .json(serde_json::json!({
                "error": { "type": "Unauthorized", "message": "A valid bearer token is required" }
            })));
    };
    let handler = RustrakMcp::for_request(&req, actor)?;

    let mut request = http::Request::builder()
        .method(req.method().as_str())
        .uri(req.uri().to_string());
    for (name, value) in req.headers() {
        request = request.header(name.as_str(), value.as_bytes());
    }
    // rmcp rejects a request without `Host` (an HTTP/1.0 client may send
    // none). It reads the value only for the DNS-rebinding check, which is
    // disabled below, so any name satisfies it. A fixed one, because the
    // request's own idea of its host (`X-Forwarded-Host`) is the caller's to
    // choose and would reach rmcp's logs.
    if !req.headers().contains_key(HOST) {
        request = request.header("host", "localhost");
    }
    let request = request
        .body(Full::new(body))
        .map_err(|e| AppError::Internal(format!("MCP request: {e}")))?;

    // ponytail: one service per request so the handler owns its caller; a
    // shared service would need the actor smuggled through request extensions.
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(NeverSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            // Host checks guard unauthenticated local servers from DNS
            // rebinding. Every call here carries a token a rebinding page
            // cannot read, and the public hostname is the operator's.
            .disable_allowed_hosts(),
    );
    let (parts, body) = service.handle(request).await.into_parts();

    let mut response = HttpResponse::build(
        StatusCode::from_u16(parts.status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
    );
    for (name, value) in &parts.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_str().as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            response.insert_header((name, value));
        }
    }
    let body = BodyStream::new(body)
        .filter_map(|frame| async move { frame.ok()?.into_data().ok().map(Ok::<_, Infallible>) });
    Ok(response.streaming(body))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    // Every method: rmcp answers what it does not serve (a legacy client's
    // `GET` stream, `DELETE`) with the 405 the transport spec asks for.
    cfg.route("/mcp", web::route().to(serve)).service(
        web::resource("/api/settings/mcp")
            .route(web::get().to(get_settings))
            .route(web::put().to(update_settings)),
    );
}
