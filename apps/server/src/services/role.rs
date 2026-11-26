use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::db::{Db, DbPool};
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::rbac::{self, Access, Action, Assignment, Grants, Resource, Scope};
use crate::models::{
    grants_of, Permission, PersonalAccess, PluginGrant, Role, RoleAssignment, User, UserRole,
};
use crate::plugins::host::PluginHost;

pub struct RoleService;

/// The refusal of handing out access the caller does not hold.
pub fn beyond_your_own() -> AppError {
    AppError::Forbidden("You cannot grant permissions you do not hold".to_string())
}

type RoleRow = (i32, String, String, DateTime<Utc>, i64);

/// A member of one project: someone holding a role on it or on every project.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectMember {
    pub user_id: i32,
    pub email: String,
    pub roles: Vec<MemberRole>,
}

/// A role someone holds on a project.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MemberRole {
    pub id: i32,
    pub name: String,
    /// Held on every project: removing them from this one leaves it in place.
    pub inherited: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RoleRef {
    pub id: i32,
    pub name: String,
}

/// Rows whose resource or action the catalog no longer knows are skipped, so
/// a stale row can never grant anything.
fn parse_pair(resource: &str, action: &str) -> Option<(Resource, Action)> {
    let (r, a) = (Resource::parse(resource)?, Action::parse(action)?);
    r.accepts(a).then_some((r, a))
}

fn name_taken(e: sqlx::Error, name: &str) -> AppError {
    match e {
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            AppError::Conflict(format!("A role named '{}' already exists", name.trim()))
        }
        other => other.into(),
    }
}

impl Role {
    pub fn grants(&self) -> Grants {
        grants_of(&self.permissions, &self.plugin_permissions)
    }
}

/// A stored row: one of Rustrak's permissions, or a plugin's.
enum Row {
    Core(Resource, Action),
    Plugin(PluginGrant),
}

fn parse_row(resource: &str, action: &str) -> Option<Row> {
    if let Some((r, a)) = parse_pair(resource, action) {
        return Some(Row::Core(r, a));
    }
    // A plugin's: kept even while the plugin is uninstalled, so reinstalling
    // brings the roles back. Nothing checks it meanwhile.
    rbac::is_plugin_key(resource).then_some(())?;
    Some(Row::Plugin(PluginGrant {
        resource: resource.to_string(),
        action: Action::parse(action)?,
    }))
}

/// Stored rows as Rustrak's permissions and the plugins'.
fn split_rows<'a>(
    rows: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> (Vec<Permission>, Vec<PluginGrant>) {
    let (mut core, mut plugins) = (Vec::new(), Vec::new());
    for (r, a) in rows {
        match parse_row(r, a) {
            Some(Row::Core(resource, action)) => core.push(Permission { resource, action }),
            Some(Row::Plugin(grant)) => plugins.push(grant),
            None => {}
        }
    }
    (core, plugins)
}

impl RoleService {
    /// Every role of the list with its permissions, oldest first.
    pub async fn list(pool: &DbPool) -> AppResult<Vec<Role>> {
        Self::fetch(pool, false).await
    }

    /// The same, plus everyone's personal roles.
    pub async fn list_all(pool: &DbPool) -> AppResult<Vec<Role>> {
        Self::fetch(pool, true).await
    }

    async fn fetch(pool: &DbPool, personal: bool) -> AppResult<Vec<Role>> {
        let rows: Vec<RoleRow> = sqlx::query_as(
            r#"
            SELECT r.id, r.name, r.description, r.created_at,
                   (SELECT COUNT(*) FROM role_assignments a WHERE a.role_id = r.id)
            FROM roles r WHERE r.user_id IS NULL OR $1 ORDER BY r.id
            "#,
        )
        .bind(personal)
        .fetch_all(pool)
        .await?;
        let perms: Vec<(i32, String, String)> =
            sqlx::query_as("SELECT role_id, resource, action FROM role_permissions")
                .fetch_all(pool)
                .await?;

        let mut rows_of: HashMap<i32, Vec<(&str, &str)>> = HashMap::new();
        for (role_id, r, a) in &perms {
            rows_of.entry(*role_id).or_default().push((r, a));
        }
        let mut by_role: HashMap<i32, (Vec<Permission>, Vec<PluginGrant>)> = rows_of
            .into_iter()
            .map(|(id, rows)| (id, split_rows(rows)))
            .collect();

        Ok(rows
            .into_iter()
            .map(|(id, name, description, created_at, count)| {
                let (permissions, plugin_permissions) = by_role.remove(&id).unwrap_or_default();
                Role {
                    permissions,
                    plugin_permissions,
                    id,
                    name,
                    description,
                    assignment_count: count,
                    created_at,
                }
            })
            .collect())
    }

    /// Refuses a change of roles beyond `access`: every role given (one of the
    /// shared list) and every role taken away (anyone's) must be covered where
    /// it applies. Pairs are `(role_id, project_id)`; `field` names the input
    /// an unknown role came from.
    pub async fn check_grantable(
        pool: &DbPool,
        access: &Access,
        give: &[(i32, Option<i32>)],
        take: &[(i32, Option<i32>)],
        field: &'static str,
    ) -> AppResult<()> {
        let shared = Self::list(pool).await?;
        let all = if take.is_empty() {
            Vec::new()
        } else {
            Self::list_all(pool).await?
        };
        let given = give.iter().map(|&(id, scope)| (&shared, id, scope));
        let taken = take.iter().map(|&(id, scope)| (&all, id, scope));
        for (roles, id, scope) in given.chain(taken) {
            let role = roles.iter().find(|r| r.id == id).ok_or_else(|| {
                AppError::Validation(format!("Unknown role {id}"))
                    .with_field(field, FieldErrorCode::Invalid)
            })?;
            if !access.covers(&role.grants(), scope) {
                return Err(beyond_your_own());
            }
        }
        Ok(())
    }

    pub async fn get(pool: &DbPool, id: i32) -> AppResult<Role> {
        Self::list(pool)
            .await?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::NotFound(format!("Role {id} not found")))
    }

    /// Creates a role. Permissions an action does not apply to are dropped.
    pub async fn create(
        pool: &DbPool,
        name: &str,
        description: &str,
        permissions: Vec<Permission>,
        plugin_permissions: Vec<PluginGrant>,
    ) -> AppResult<Role> {
        let mut tx = pool.begin().await?;
        let (id,): (i32,) =
            sqlx::query_as("INSERT INTO roles (name, description) VALUES ($1, $2) RETURNING id")
                .bind(name.trim())
                .bind(description)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| name_taken(e, name))?;
        Self::write_permissions(&mut tx, id, &grants_of(&permissions, &plugin_permissions)).await?;
        tx.commit().await?;
        Self::get(pool, id).await
    }

    /// Replaces a role's name, description and permissions.
    pub async fn update(
        pool: &DbPool,
        id: i32,
        name: &str,
        description: &str,
        permissions: Vec<Permission>,
        plugin_permissions: Vec<PluginGrant>,
    ) -> AppResult<Role> {
        Self::get(pool, id).await?;
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE roles SET name = $1, description = $2 WHERE id = $3")
            .bind(name.trim())
            .bind(description)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| name_taken(e, name))?;
        Self::write_permissions(&mut tx, id, &grants_of(&permissions, &plugin_permissions)).await?;
        tx.commit().await?;
        Self::get(pool, id).await
    }

    async fn write_permissions(
        tx: &mut sqlx::Transaction<'_, Db>,
        role_id: i32,
        grants: &Grants,
    ) -> AppResult<()> {
        sqlx::query("DELETE FROM role_permissions WHERE role_id = $1")
            .bind(role_id)
            .execute(&mut **tx)
            .await?;
        let rows = grants
            .pairs()
            .into_iter()
            .map(|(r, a)| (r.key().to_string(), a))
            .chain(grants.plugin_pairs());
        for (r, a) in rows {
            sqlx::query(
                "INSERT INTO role_permissions (role_id, resource, action) VALUES ($1, $2, $3)",
            )
            .bind(role_id)
            .bind(r)
            .bind(a.key())
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Deletes a role nobody holds and no pending invitation names.
    pub async fn delete(pool: &DbPool, id: i32) -> AppResult<()> {
        let role = Self::get(pool, id).await?;
        if role.assignment_count > 0 {
            return Err(AppError::Conflict(format!(
                "Role '{}' is still assigned {} time(s)",
                role.name, role.assignment_count
            )));
        }
        // Accepting one would otherwise create the account without the role.
        let (pending,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM invitations WHERE role_id = $1 AND status = 'pending'",
        )
        .bind(id)
        .fetch_one(pool)
        .await?;
        if pending > 0 {
            return Err(AppError::Conflict(format!(
                "Role '{}' is named by {pending} pending invitation(s)",
                role.name
            )));
        }
        sqlx::query("DELETE FROM roles WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Gives `role_id` to a user. Giving the same role in the same place twice
    /// is a no-op.
    pub async fn assign<'e, E>(
        executor: E,
        user_id: i32,
        role_id: i32,
        project_id: Option<i32>,
    ) -> AppResult<()>
    where
        E: sqlx::Executor<'e, Database = Db>,
    {
        sqlx::query(
            "INSERT INTO role_assignments (user_id, role_id, project_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(role_id)
        .bind(project_id)
        .execute(executor)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(d) if d.is_foreign_key_violation() => {
                AppError::NotFound("User, role or project not found".to_string())
            }
            other => other.into(),
        })?;
        Ok(())
    }

    /// Bring a user's SSO-managed assignments in line with their groups.
    ///
    /// The roles named in `mapping` belong to SSO: every assignment of them is
    /// replaced by the ones `matched` asks for. Any other role, given by hand,
    /// and the user's personal permissions are left alone. Roles and projects
    /// are looked up now, so the mapping may name ones created after startup;
    /// one that does not exist is logged and skipped rather than failing the
    /// login.
    pub async fn sync_sso_assignments(
        tx: &mut sqlx::Transaction<'_, Db>,
        user_id: i32,
        mapping: &[crate::config::GroupRole],
        matched: &[crate::config::GroupRole],
    ) -> AppResult<()> {
        if mapping.is_empty() {
            return Ok(());
        }
        // SQLite's BEGIN IMMEDIATE already runs this alone. On PostgreSQL two
        // sign-ins of one account could both delete before either inserts and
        // leave the union of their groups' access, so take the user's row
        // first and let the replacements run one after the other. NO KEY
        // UPDATE, because inserting an identity already holds KEY SHARE on
        // this row through its foreign key; FOR UPDATE would deadlock two
        // links of the same account.
        #[cfg(feature = "postgres")]
        sqlx::query("SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE")
            .bind(user_id)
            .execute(&mut **tx)
            .await?;

        let mut role_ids: HashMap<&str, i32> = HashMap::new();
        for entry in mapping {
            if role_ids.contains_key(entry.role.as_str()) {
                continue;
            }
            let id: Option<(i32,)> =
                sqlx::query_as("SELECT id FROM roles WHERE name = $1 AND user_id IS NULL")
                    .bind(&entry.role)
                    .fetch_optional(&mut **tx)
                    .await?;
            match id {
                Some((id,)) => {
                    role_ids.insert(&entry.role, id);
                }
                None => log::warn!(
                    "OIDC_GROUP_ROLES names role {:?}, which does not exist; skipped",
                    entry.role
                ),
            }
        }

        for &role_id in role_ids.values() {
            sqlx::query("DELETE FROM role_assignments WHERE user_id = $1 AND role_id = $2")
                .bind(user_id)
                .bind(role_id)
                .execute(&mut **tx)
                .await?;
        }

        for entry in matched {
            let Some(&role_id) = role_ids.get(entry.role.as_str()) else {
                continue;
            };
            let project_id = match &entry.project_slug {
                None => None,
                Some(slug) => {
                    let id: Option<(i32,)> =
                        sqlx::query_as("SELECT id FROM projects WHERE slug = $1")
                            .bind(slug)
                            .fetch_optional(&mut **tx)
                            .await?;
                    let Some((id,)) = id else {
                        log::warn!(
                            "OIDC_GROUP_ROLES names project {slug:?}, which does not exist; skipped"
                        );
                        continue;
                    };
                    Some(id)
                }
            };
            Self::assign(&mut **tx, user_id, role_id, project_id).await?;
        }
        Ok(())
    }

    /// Gives a user the roles they already hold on one more project.
    pub async fn extend_to_project(pool: &DbPool, user_id: i32, project_id: i32) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO role_assignments (user_id, role_id, project_id)
            SELECT DISTINCT user_id, role_id, CAST($2 AS INTEGER)
            FROM role_assignments WHERE user_id = $1
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(project_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Replaces everything a user holds, in one transaction: their assignments
    /// and their personal permissions, whose role goes when `personal` is `None`.
    pub async fn set_assignments(
        pool: &DbPool,
        user_id: i32,
        assignments: &[(i32, Option<i32>)],
        personal: Option<&PersonalAccess>,
    ) -> AppResult<()> {
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM role_assignments WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        for &(role_id, project_id) in assignments {
            Self::assign(&mut *tx, user_id, role_id, project_id).await?;
        }
        let Some(personal) = personal else {
            sqlx::query("DELETE FROM roles WHERE user_id = $1")
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(());
        };
        let existing: Option<(i32,)> = sqlx::query_as("SELECT id FROM roles WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
        let role_id = match existing {
            Some((id,)) => id,
            None => {
                let (id,): (i32,) = sqlx::query_as(
                    "INSERT INTO roles (name, user_id) VALUES ('Custom', $1) RETURNING id",
                )
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await?;
                id
            }
        };
        Self::write_permissions(
            &mut tx,
            role_id,
            &grants_of(
                &personal.permissions,
                personal.plugin_permissions.as_deref().unwrap_or_default(),
            ),
        )
        .await?;
        match &personal.project_ids {
            None => Self::assign(&mut *tx, user_id, role_id, None).await?,
            Some(ids) => {
                for &id in ids {
                    Self::assign(&mut *tx, user_id, role_id, Some(id)).await?;
                }
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// The permissions a user was given directly: Rustrak's, and plugins'.
    pub async fn personal_permissions(
        pool: &DbPool,
        user_id: i32,
    ) -> AppResult<(Vec<Permission>, Vec<PluginGrant>)> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT p.resource, p.action
            FROM role_permissions p JOIN roles r ON r.id = p.role_id
            WHERE r.user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        Ok(split_rows(
            rows.iter().map(|(r, a)| (r.as_str(), a.as_str())),
        ))
    }

    /// Replaces the roles a user holds on one project (empty removes them).
    /// Their personal permissions stay: those are set with `set_assignments`.
    pub async fn set_project_roles(
        pool: &DbPool,
        project_id: i32,
        user_id: i32,
        role_ids: &[i32],
    ) -> AppResult<()> {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "DELETE FROM role_assignments WHERE user_id = $1 AND project_id = $2
             AND role_id NOT IN (SELECT id FROM roles WHERE user_id IS NOT NULL)",
        )
        .bind(user_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;
        for &role_id in role_ids {
            Self::assign(&mut *tx, user_id, role_id, Some(project_id)).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn assignments_for(pool: &DbPool, user_id: i32) -> AppResult<Vec<RoleAssignment>> {
        Ok(sqlx::query_as(
            r#"
            SELECT a.id, a.user_id, a.role_id, r.name AS role_name, a.project_id,
                   r.user_id IS NOT NULL AS is_personal, a.created_at
            FROM role_assignments a JOIN roles r ON r.id = a.role_id
            WHERE a.user_id = $1 ORDER BY a.project_id IS NOT NULL, a.project_id, r.name
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?)
    }

    /// What a user may do: everything for an instance owner
    /// (`users.role = 'admin'`), otherwise the sum of their assigned roles.
    pub async fn access_for(pool: &DbPool, user: &User) -> AppResult<Access> {
        let user_id = user.id;
        if UserRole::from_db(&user.role) == UserRole::Admin {
            return Ok(Access::new(vec![Assignment {
                project_id: None,
                grants: Grants::all(),
            }]));
        }
        let rows: Vec<(Option<i32>, String, String)> = sqlx::query_as(
            r#"
            SELECT a.project_id, p.resource, p.action
            FROM role_assignments a
            JOIN role_permissions p ON p.role_id = a.role_id
            WHERE a.user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        Ok(Access::new(
            rows.into_iter()
                .filter_map(|(project_id, r, a)| Some((project_id, parse_row(&r, &a)?)))
                .map(|(project_id, row)| match row {
                    Row::Core(r, a) => Assignment {
                        // Workspace permissions apply wherever the role is held.
                        project_id: project_id.filter(|_| r.scope() == Scope::Project),
                        grants: Grants::of([(r, a)]),
                    },
                    // A plugin's scope is its manifest's: `can_plugin` names it.
                    Row::Plugin(g) => Assignment {
                        project_id,
                        grants: Grants::default().with_plugins([(g.resource, g.action)]),
                    },
                })
                .collect(),
        ))
    }

    /// Everyone holding a role of the list that reaches this project, by email.
    /// Owners hold no role, and personal permissions are no role of the list.
    pub async fn project_members(
        pool: &DbPool,
        project_id: i32,
        host: &PluginHost,
    ) -> AppResult<Vec<ProjectMember>> {
        let rows: Vec<(i32, String, i32, String, Option<i32>)> = sqlx::query_as(
            r#"
            SELECT u.id, u.email, r.id, r.name, a.project_id
            FROM role_assignments a
            JOIN users u ON u.id = a.user_id
            JOIN roles r ON r.id = a.role_id
            WHERE (a.project_id = $1 OR a.project_id IS NULL) AND r.user_id IS NULL
            ORDER BY u.email, r.name
            "#,
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        let reaching: Vec<i32> = Self::list_all(pool)
            .await?
            .into_iter()
            .filter(|r| {
                r.permissions
                    .iter()
                    .any(|p| p.resource.scope() == Scope::Project)
                    || r.plugin_permissions.iter().any(|g| {
                        host.permission_scope(&g.resource, g.action) == Some(Scope::Project)
                    })
            })
            .map(|r| r.id)
            .collect();
        let rows = rows
            .into_iter()
            .filter(|(_, _, id, _, _)| reaching.contains(id));
        let mut members: Vec<ProjectMember> = Vec::new();
        for (user_id, email, id, name, place) in rows {
            let role = MemberRole {
                id,
                name,
                inherited: place.is_none(),
            };
            match members.last_mut() {
                Some(m) if m.user_id == user_id => {
                    match m.roles.iter_mut().find(|r| r.id == role.id) {
                        Some(held) => held.inherited |= role.inherited,
                        None => m.roles.push(role),
                    }
                }
                _ => members.push(ProjectMember {
                    user_id,
                    email,
                    roles: vec![role],
                }),
            }
        }
        Ok(members)
    }
}
