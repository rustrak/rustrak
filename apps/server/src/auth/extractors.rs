use actix_web::{dev::Payload, web, FromRequest, HttpRequest};
use std::future::Future;
use std::pin::Pin;

use crate::auth::sentry_auth::parse_sentry_auth_header;
use crate::auth::session::AuthenticatedUser;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{self, Access, Assignment, Grants, ProjectScope, Resource};
use crate::models::{AuthToken, Project, User};
use crate::services::{AuthTokenService, ProjectService, RoleService, UsersService};

/// The database pool registered on the app.
pub(crate) fn pool(req: &HttpRequest) -> AppResult<web::Data<DbPool>> {
    req.app_data::<web::Data<DbPool>>()
        .cloned()
        .ok_or_else(|| AppError::Internal("Database pool not configured".to_string()))
}

/// The API token of an `Authorization: Bearer` header: well-formed, known
/// and unexpired. Every failure is a 401 except a database one.
pub(crate) async fn api_token(req: &HttpRequest, pool: &DbPool) -> AppResult<AuthToken> {
    let header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;
    let token = header.strip_prefix("Bearer ").ok_or_else(|| {
        AppError::Unauthorized(
            "Invalid Authorization header format, expected 'Bearer <token>'".to_string(),
        )
    })?;
    let token = token.trim();
    if !crate::models::token::is_valid_token_format(token) {
        return Err(AppError::Unauthorized(
            "Malformed Bearer token, must be 40 lowercase hex chars".to_string(),
        ));
    }
    let token = AuthTokenService::get_by_token(pool, token)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid Bearer token".to_string()))?;
    if token.is_expired() {
        return Err(AppError::Unauthorized("Bearer token expired".to_string()));
    }

    // Off the request path: the answer does not wait on this write.
    let pool = pool.clone();
    let token_id = token.id;
    tokio::spawn(async move {
        if let Err(e) = AuthTokenService::update_last_used(&pool, token_id).await {
            log::warn!("Could not record the last use of API token {token_id}: {e}");
        }
    });

    Ok(token)
}

/// Sentry SDK authentication (ingest endpoints): the project of the URL path,
/// with the DSN's key from the path, the query, `X-Sentry-Auth` or Basic auth.
pub struct SentryAuth {
    pub project: Project,
}

impl FromRequest for SentryAuth {
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let pool = pool(req);

        let project_id: Option<i32> = req
            .match_info()
            .get("project_id")
            .and_then(|s| s.parse().ok());

        // The HTTP check-in endpoint carries the key in its path.
        let path_sentry_key = req.match_info().get("sentry_key").map(str::to_string);

        let query_sentry_key = req.query_string().split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            if key == "sentry_key" {
                Some(value.to_string())
            } else {
                None
            }
        });

        let header_sentry_key = req
            .headers()
            .get("X-Sentry-Auth")
            .and_then(|h| h.to_str().ok())
            .map(parse_sentry_auth_header)
            .and_then(|map| map.get("sentry_key").cloned());

        // `Authorization: Basic base64("<key>:")`, which Relay accepts on the
        // HTTP check-in endpoint.
        let basic_sentry_key = req
            .headers()
            .get(actix_web::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Basic "))
            .and_then(|b| {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD
                    .decode(b.trim())
                    .ok()
            })
            .and_then(|raw| String::from_utf8(raw).ok())
            .and_then(|creds| creds.split(':').next().map(str::to_string))
            .filter(|key| !key.is_empty());

        Box::pin(async move {
            let pool = pool?;
            let project_id = project_id.ok_or_else(|| {
                AppError::Validation("Missing or invalid project_id in URL".to_string())
            })?;

            // Path (HTTP check-ins), query param, header, then Basic auth
            let sentry_key_str = path_sentry_key
                .or(query_sentry_key)
                .or(header_sentry_key)
                .or(basic_sentry_key)
                .ok_or_else(|| {
                    AppError::Unauthorized(
                        "Missing sentry_key in query param or X-Sentry-Auth header".to_string(),
                    )
                })?;

            let sentry_key: uuid::Uuid = sentry_key_str
                .parse()
                .map_err(|_| AppError::Unauthorized("Invalid sentry_key format".to_string()))?;

            let project = ProjectService::get_by_id(pool.get_ref(), project_id).await?;

            if project.sentry_key != sentry_key {
                return Err(AppError::Unauthorized(
                    "Invalid sentry_key for project".to_string(),
                ));
            }

            Ok(SentryAuth { project })
        })
    }
}

/// Whether a failed Bearer attempt should fall through to session auth.
///
/// Reads [`AppError::kind`] rather than matching the error itself. A browser
/// carrying a stale `Authorization` header alongside a valid session cookie is
/// the ordinary case this exists for, and the day any Bearer path annotates
/// its `Unauthorized` with a field (a login form error is the obvious next
/// one), a bare `matches!(err, AppError::Unauthorized(_))` would stop matching
/// and turn that request into a 401 that looks exactly like a session bug.
/// Only a genuine `Internal`/`Database` failure propagates.
fn falls_through_to_session(error: &AppError) -> bool {
    matches!(error.kind(), AppError::Unauthorized(_))
}

/// Authenticated principal for management API endpoints: the acting user and
/// what they may do.
///
/// Resolves from a Bearer token (its owner) or a session cookie. A legacy
/// token with no owning user (`user_id = NULL`) resolves to `user: None` with
/// every permission.
pub struct ApiActor {
    pub user: Option<User>,
    pub access: Access,
    /// Signed in with a session, rather than holding a token.
    pub via_session: bool,
}

impl ApiActor {
    /// The actor a token acts as: its owner, or full access for a legacy
    /// token with none (`user_id = None`).
    pub async fn for_token(pool: &DbPool, user_id: Option<i32>) -> AppResult<Self> {
        let Some(uid) = user_id else {
            return Ok(ApiActor {
                via_session: false,
                user: None,
                access: Access::new(vec![Assignment {
                    project_id: None,
                    grants: Grants::all(),
                }]),
            });
        };
        let user = UsersService::get_by_id(pool, uid)
            .await?
            .ok_or_else(|| AppError::Unauthorized("Token owner not found".to_string()))?;
        user.ensure_active()?;
        Self::for_user(pool, user).await
    }

    /// The actor an API token acts as: [`ApiActor::for_token`] narrowed by
    /// the token's own scope.
    pub async fn for_api_token(pool: &DbPool, token: &AuthToken) -> AppResult<Self> {
        let actor = Self::for_token(pool, token.user_id).await?;
        Ok(ApiActor {
            access: actor.access.capped(token.scope()),
            ..actor
        })
    }

    pub async fn for_user(pool: &DbPool, user: User) -> AppResult<Self> {
        let access = RoleService::access_for(pool, user.id).await?;
        Ok(ApiActor {
            via_session: false,
            user: Some(user),
            access,
        })
    }

    /// Fails unless the actor may do `action` on `resource`. For a project
    /// resource, someone who cannot see the project gets a 404 so its
    /// existence is not leaked; someone who can see it but lacks the
    /// permission gets a 403.
    pub fn require(
        &self,
        resource: Resource,
        action: rbac::Action,
        project_id: Option<i32>,
    ) -> AppResult<()> {
        if let Some(id) = project_id {
            if !self
                .access
                .can(Resource::Project, rbac::Action::Read, Some(id))
            {
                return Err(AppError::NotFound(format!("Project {id} not found")));
            }
        }
        if self.access.can(resource, action, project_id) {
            Ok(())
        } else {
            Err(AppError::Forbidden(format!(
                "Missing permission {}:{}",
                resource.key(),
                action.key()
            )))
        }
    }

    pub fn visible_projects(&self) -> ProjectScope {
        self.access.visible_projects()
    }

    /// Holds an instance-wide role with every permission.
    pub fn is_owner(&self) -> bool {
        self.access.is_owner()
    }

    /// The acting user's id, if any (`None` for a legacy user-less token).
    pub fn user_id(&self) -> Option<i32> {
        self.user.as_ref().map(|u| u.id)
    }

    /// Returns the acting user or an error when the action requires a real account.
    pub fn require_user(&self) -> AppResult<&User> {
        self.user
            .as_ref()
            .ok_or_else(|| AppError::Forbidden("This action requires a user account".to_string()))
    }
}

/// A Bearer token first, then the session cookie. A browser carrying a stale
/// `Authorization` header beside a valid session is the ordinary case for the
/// fall-through; only a genuine `Internal`/`Database` failure propagates.
impl FromRequest for ApiActor {
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let req = req.clone();
        let session_future = AuthenticatedUser::from_request(&req, payload);

        Box::pin(async move {
            let pool = pool(&req)?;
            match api_token(&req, pool.get_ref()).await {
                Ok(token) => return ApiActor::for_api_token(pool.get_ref(), &token).await,
                Err(ref e) if falls_through_to_session(e) => {}
                Err(e) => return Err(e),
            }

            let authed = session_future.await.map_err(|e| {
                if e.as_response_error().status_code().is_server_error() {
                    AppError::Internal(format!("Session error: {e}"))
                } else {
                    AppError::Unauthorized("Not authenticated".to_string())
                }
            })?;

            let actor = ApiActor::for_user(pool.get_ref(), authed.0).await?;
            Ok(ApiActor {
                via_session: true,
                ..actor
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FieldErrorCode;

    /// The invariant the Bearer→session fallthrough rests on. No annotated
    /// site produces `Unauthorized` today, so this is the only thing standing
    /// between "someone annotates a login field error" and "a browser with a
    /// stale Authorization header plus a valid session cookie gets a 401".
    #[test]
    fn an_annotated_unauthorized_still_falls_through_to_session() {
        let annotated = AppError::Unauthorized("Invalid Bearer token".to_string())
            .with_field("token", FieldErrorCode::Invalid);

        assert!(
            falls_through_to_session(&annotated),
            "an annotated Unauthorized must fall through to session auth"
        );

        // The shape the old code matched on, spelled out so the regression is
        // named rather than implied: this is exactly what stops matching.
        assert!(
            !matches!(annotated, AppError::Unauthorized(_)),
            "an annotated Unauthorized is AppError::WithFields, not \
             AppError::Unauthorized, which is why kind() is required here"
        );
    }

    #[test]
    fn a_plain_unauthorized_falls_through_to_session() {
        assert!(falls_through_to_session(&AppError::Unauthorized(
            "Missing Authorization header".to_string()
        )));
    }

    #[test]
    fn an_infrastructure_failure_never_falls_through() {
        for error in [
            AppError::Internal("pool exhausted".to_string()),
            AppError::Internal("pool exhausted".to_string())
                .with_field("x", FieldErrorCode::Custom),
            AppError::Validation("Missing or invalid project_id in URL".to_string()),
        ] {
            assert!(
                !falls_through_to_session(&error),
                "{error} must propagate rather than silently degrade to session auth"
            );
        }
    }
}
