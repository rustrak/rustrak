use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::rbac::{Action, Grants, Resource};

/// One `(resource, action)` grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Permission {
    pub resource: Resource,
    pub action: Action,
}

impl Permission {
    pub fn grants(permissions: &[Permission]) -> Grants {
        Grants::of(permissions.iter().map(|p| (p.resource, p.action)))
    }
}

/// One grant of a plugin's permission: `hello:hits`, as its manifest
/// declares it. Kept apart from `Permission`, whose resource is Rustrak's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginGrant {
    /// `<plugin>:<resource>`.
    pub resource: String,
    pub action: Action,
}

/// Rustrak's grants and the plugins' ones, as one.
pub fn grants_of(permissions: &[Permission], plugins: &[PluginGrant]) -> Grants {
    Permission::grants(permissions)
        .with_plugins(plugins.iter().map(|p| (p.resource.clone(), p.action)))
}

/// A named set of permissions, given to users by an assignment.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Role {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub permissions: Vec<Permission>,
    /// Plugins' permissions it gives.
    pub plugin_permissions: Vec<PluginGrant>,
    /// How many assignments use this role.
    pub assignment_count: i64,
    pub created_at: DateTime<Utc>,
}

/// A role given to a user. `project_id = null` reaches every project. A role's
/// workspace permissions apply wherever it is held.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RoleAssignment {
    pub id: i32,
    pub user_id: i32,
    pub role_id: i32,
    pub role_name: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub project_id: Option<i32>,
    /// The user's own permissions rather than a role of the list.
    pub is_personal: bool,
    pub created_at: DateTime<Utc>,
}

/// Permissions given to one person directly, without a role of the list.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PersonalAccess {
    pub permissions: Vec<Permission>,
    /// Omitted or `null`: the ones held stay.
    #[serde(default)]
    pub plugin_permissions: Option<Vec<PluginGrant>>,
    /// Where its project permissions apply. Omitted or `null`: every project.
    #[serde(default)]
    pub project_ids: Option<Vec<i32>>,
}
