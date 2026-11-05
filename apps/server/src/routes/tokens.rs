use actix_web::{web, HttpResponse};

use crate::auth::rbac::{Action, Resource};
use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::CreateAuthToken;
#[cfg(feature = "openapi")]
use crate::models::{AuthTokenCreatedResponse, AuthTokenResponse};
use crate::services::AuthTokenService;

#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/tokens",
    operation_id = "tokens.list",
    tag = "Tokens",
    responses(
        (status = 200, description = "List of tokens (masked)", body = Vec<AuthTokenResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/tokens - List tokens (masked)
///
/// Everyone sees their own tokens; `api-tokens:read` sees every token.
pub async fn list_tokens(pool: web::Data<DbPool>, actor: ApiActor) -> AppResult<HttpResponse> {
    let tokens = if actor.access.can(Resource::ApiTokens, Action::Read, None) {
        AuthTokenService::list(pool.get_ref()).await?
    } else {
        let uid = actor
            .user_id()
            .ok_or_else(|| AppError::Unauthorized("Not authenticated".to_string()))?;
        AuthTokenService::list_for_user(pool.get_ref(), uid).await?
    };
    let responses: Vec<_> = tokens.iter().map(|t| t.to_response()).collect();

    Ok(HttpResponse::Ok().json(responses))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/tokens",
    operation_id = "tokens.create",
    tag = "Tokens",
    request_body = CreateAuthToken,
    responses(
        (status = 201, description = "Token created (full token shown once)", body = AuthTokenCreatedResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Called with a token, or a permission the caller lacks", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// POST /api/tokens - Create a new token (from a signed-in session only)
///
/// A token cannot create tokens: a limited or expiring one could otherwise
/// mint itself a way out of its limits.
pub async fn create_token(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    body: web::Json<CreateAuthToken>,
) -> AppResult<HttpResponse> {
    if !actor.via_session {
        return Err(AppError::Forbidden(
            "API tokens can only be created from a signed-in session".to_string(),
        ));
    }
    let lacking = body
        .permissions
        .iter()
        .flatten()
        .any(|p| !actor.access.holds_somewhere(p.resource, p.action));
    if lacking {
        return Err(AppError::Forbidden(
            "You cannot grant permissions you do not hold".to_string(),
        ));
    }
    let token =
        AuthTokenService::create_for_user(pool.get_ref(), body.into_inner(), actor.user_id())
            .await?;

    // Return full token (only time it's visible!)
    Ok(HttpResponse::Created().json(token.to_created_response()))
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/api/tokens/{id}",
    operation_id = "tokens.delete",
    tag = "Tokens",
    params(("id" = i32, Path, description = "Token ID")),
    responses(
        (status = 204, description = "Token revoked"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// DELETE /api/tokens/{id} - Revoke a token
///
/// Admins may revoke any token; non-admins may only revoke their own.
pub async fn delete_token(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();

    if !actor.access.can(Resource::ApiTokens, Action::Delete, None) {
        let token = AuthTokenService::get_by_id(pool.get_ref(), id).await?;
        if token.user_id != actor.user_id() {
            return Err(AppError::Forbidden(
                "You can only revoke your own tokens".to_string(),
            ));
        }
    }

    AuthTokenService::delete(pool.get_ref(), id).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/tokens/{id}",
    operation_id = "tokens.get",
    tag = "Tokens",
    params(("id" = i32, Path, description = "Token ID")),
    responses(
        (status = 200, description = "Token details (masked)", body = AuthTokenResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// GET /api/tokens/{id} - Token details, masked
///
/// Tokens are stored hashed, so the full value is only ever shown on
/// creation. Everyone may view their own; `api-tokens:read` views any.
pub async fn get_token(
    pool: web::Data<DbPool>,
    actor: ApiActor,
    path: web::Path<i32>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    let token = AuthTokenService::get_by_id(pool.get_ref(), id).await?;

    if !actor.access.can(Resource::ApiTokens, Action::Read, None)
        && token.user_id != actor.user_id()
    {
        return Err(AppError::Forbidden(
            "You can only view your own tokens".to_string(),
        ));
    }

    Ok(HttpResponse::Ok().json(token.to_response()))
}

#[cfg(feature = "openapi")]
#[derive(OpenApi)]
#[openapi(
    paths(list_tokens, create_token, get_token, delete_token),
    components(schemas(
        crate::models::AuthTokenResponse,
        crate::models::AuthTokenCreatedResponse,
        crate::models::CreateAuthToken,
    ))
)]
pub struct TokensApi;

/// Configure token routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/tokens")
            .route("", web::get().to(list_tokens))
            .route("", web::post().to(create_token))
            .route("/{id}", web::get().to(get_token))
            .route("/{id}", web::delete().to(delete_token)),
    );
}
