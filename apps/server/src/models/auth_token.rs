use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::models::rbac::{Grants, TokenScope};
use crate::models::Permission;

/// An API token. `token` holds the SHA-256 hash, except on a token just
/// created, where it holds the raw value that is shown once.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AuthToken {
    pub id: i32,
    #[serde(skip)]
    pub token: String,
    pub token_prefix: String,
    pub description: Option<String>,
    /// Owning user. `None` for legacy/instance-level tokens (full access).
    pub user_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub all_permissions: bool,
    pub all_projects: bool,
    #[sqlx(skip)]
    pub permissions: Vec<Permission>,
    #[sqlx(skip)]
    pub project_ids: Vec<i32>,
}

/// DTO for creating a new token
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateAuthToken {
    #[serde(default)]
    pub description: Option<String>,
    /// What the token may do, within its owner's permissions. Omitted or
    /// `null`: everything its owner may do.
    #[serde(default)]
    pub permissions: Option<Vec<Permission>>,
    /// The projects it may reach. Omitted or `null`: every project its owner reaches.
    #[serde(default)]
    pub project_ids: Option<Vec<i32>>,
    /// When it stops working. Omitted or `null`: never.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Response that includes the full token (only on creation)
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AuthTokenCreatedResponse {
    pub id: i32,
    pub token: String, // Only shown once!
    #[cfg_attr(feature = "openapi", schema(required))]
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    /// `null`: everything its owner may do.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub permissions: Option<Vec<Permission>>,
    /// `null`: every project its owner reaches.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub project_ids: Option<Vec<i32>>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Response for listing (token is masked)
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AuthTokenResponse {
    pub id: i32,
    pub token_prefix: String, // First 8 chars only
    #[cfg_attr(feature = "openapi", schema(required))]
    pub description: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub user_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub last_used_at: Option<DateTime<Utc>>,
    /// `null`: everything its owner may do.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub permissions: Option<Vec<Permission>>,
    /// `null`: every project its owner reaches.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub project_ids: Option<Vec<i32>>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub expires_at: Option<DateTime<Utc>>,
}

impl AuthToken {
    fn permissions_or_all(&self) -> Option<Vec<Permission>> {
        (!self.all_permissions).then(|| self.permissions.clone())
    }

    fn projects_or_all(&self) -> Option<Vec<i32>> {
        (!self.all_projects).then(|| self.project_ids.clone())
    }

    /// What the token was limited to, to narrow its owner's access.
    pub fn scope(&self) -> TokenScope {
        TokenScope {
            grants: if self.all_permissions {
                Grants::all()
            } else {
                crate::models::Permission::grants(&self.permissions)
            },
            projects: self.projects_or_all(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|at| at <= Utc::now())
    }

    /// Masked, for listing.
    pub fn to_response(&self) -> AuthTokenResponse {
        AuthTokenResponse {
            id: self.id,
            token_prefix: format!("{}...", self.token_prefix),
            description: self.description.clone(),
            user_id: self.user_id,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
            permissions: self.permissions_or_all(),
            project_ids: self.projects_or_all(),
            expires_at: self.expires_at,
        }
    }

    /// Full response with the raw token, only right after creation.
    pub fn to_created_response(&self) -> AuthTokenCreatedResponse {
        AuthTokenCreatedResponse {
            id: self.id,
            token: self.token.clone(),
            description: self.description.clone(),
            created_at: self.created_at,
            permissions: self.permissions_or_all(),
            project_ids: self.projects_or_all(),
            expires_at: self.expires_at,
        }
    }
}
