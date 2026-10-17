//! Project-scoped exact lookup across issues by user or request identity.
use actix_web::{web, HttpResponse};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::pagination::{EventCursor, PaginatedResponse};
use crate::services::access::{self, Action};
use crate::services::event_lookup::{self, LookupKind};
use crate::services::ProjectService;

/// Exactly one selector is required; cursors belong to that project and selector.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct LookupQuery {
    /// Exact string from the event's user.id (1..200 UTF-8 bytes).
    pub user_id: Option<String>,
    /// Exact request.id tag in object or array-form event tags (1..200 UTF-8 bytes).
    pub request_id: Option<String>,
    /// Opaque continuation returned by the preceding lookup page.
    pub cursor: Option<String>,
}

impl LookupQuery {
    fn selector(&self) -> AppResult<(LookupKind, &str)> {
        let (kind, value) = match (self.user_id.as_deref(), self.request_id.as_deref()) {
            (Some(value), None) => (LookupKind::User, value),
            (None, Some(value)) => (LookupKind::Request, value),
            _ => {
                return Err(AppError::Validation(
                    "Supply exactly one user_id or request_id".into(),
                ))
            }
        };
        if value.is_empty()
            || value.len() > 200
            || value.chars().any(char::is_control)
            || value.trim() != value
        {
            return Err(AppError::Validation(
                "Lookup identifier must contain 1..200 UTF-8 bytes without control or surrounding whitespace".into(),
            ));
        }
        Ok((kind, value))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LookupCursor {
    project_id: i32,
    kind: LookupKind,
    value: String,
    event: EventCursor,
}

impl LookupCursor {
    fn decode(encoded: &str, project_id: i32, kind: LookupKind, value: &str) -> AppResult<Self> {
        if encoded.len() > 2048 {
            return Err(AppError::Validation(
                "Lookup cursor exceeds 2048 bytes".into(),
            ));
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| AppError::Validation("Invalid lookup cursor encoding".into()))?;
        let cursor: Self = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::Validation("Invalid lookup cursor format".into()))?;
        // Reject cross-search reuse instead of silently skipping unrelated rows.
        if cursor.project_id != project_id
            || cursor.kind != kind
            || cursor.value != value
            || cursor.event.order != "desc"
        {
            return Err(AppError::Validation(
                "Lookup cursor belongs to a different project, selector or order".into(),
            ));
        }
        Ok(cursor)
    }

    fn encode(&self) -> AppResult<String> {
        let bytes = serde_json::to_vec(self).map_err(|error| {
            AppError::Internal(format!("Lookup cursor serialization failed: {error}"))
        })?;
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/events/lookup",
    tag = "Events",
    params(("project_id" = i32, Path, description = "Project ID"), LookupQuery),
    responses(
        (status = 200, description = "Exact identity matches, newest first; at most 20 summaries", body = inline(crate::pagination::PaginatedResponse<crate::models::EventResponse>)),
        (status = 400, description = "Invalid selector or cursor", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Accessible project not found", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = [])),
))]
/// Find exact user/request matches across issues with project Viewer permission.
pub async fn lookup(
    pool: web::Data<DbPool>,
    project_id: web::Path<i32>,
    query: web::Query<LookupQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = project_id.into_inner();
    if project_id <= 0 {
        return Err(AppError::Validation("project_id must be positive".into()));
    }
    access::require(
        pool.get_ref(),
        actor.is_admin(),
        actor.user_id(),
        project_id,
        Action::ViewProject,
    )
    .await?;
    // Admin and legacy tokens bypass membership; nonexistent projects still fail.
    ProjectService::get_by_id(pool.get_ref(), project_id).await?;
    let (kind, value) = query.selector()?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|encoded| LookupCursor::decode(encoded, project_id, kind, value))
        .transpose()?;
    let (events, has_more) = event_lookup::list(
        pool.get_ref(),
        project_id,
        kind,
        value,
        cursor.as_ref().map(|cursor| &cursor.event),
    )
    .await?;
    let next_cursor = if has_more {
        events
            .last()
            .map(|last| {
                LookupCursor {
                    project_id,
                    kind,
                    value: value.to_owned(),
                    event: EventCursor::new("desc", last.timestamp, last.id),
                }
                .encode()
            })
            .transpose()?
    } else {
        None
    };
    let responses = events.iter().map(|event| event.to_response()).collect();
    Ok(HttpResponse::Ok().json(PaginatedResponse::new(responses, next_cursor, has_more)))
}
