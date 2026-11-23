pub mod file;

use log::{info, warn};

use slug::slugify;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::{CreateProject, CreateUserRequest, Product, UpdateProject, User, UserRole};
use crate::services::project::ProjectRename;
use crate::services::{AuthTokenService, DeclaredToken, ProjectService, UsersService};
use file::{BootstrapFile, ProjectSpec, SuperuserSpec};

/// Stores `RUSTRAK_BOOTSTRAP_TOKEN`'s value as the first API token of an
/// instance that has none (#356). The operator chose the value, so nothing is
/// printed: stdout and stderr end up in `docker logs` and journald. A value
/// that is not a token creates nothing and says how to make one, without
/// stopping a server that may have run with the old `=1` form for months.
pub async fn bootstrap_token(pool: &DbPool, value: &str) -> AppResult<()> {
    if !crate::models::token::is_valid_token_format(value) {
        warn!(
            "RUSTRAK_BOOTSTRAP_TOKEN is not a token, so none was created: set it to 40 \
             lowercase hex characters, for example from `openssl rand -hex 20`"
        );
        return Ok(());
    }
    if AuthTokenService::has_any_token(pool).await? {
        info!("Auth tokens already exist, skipping RUSTRAK_BOOTSTRAP_TOKEN");
        return Ok(());
    }
    AuthTokenService::create_instance_token(
        pool,
        value,
        "Bootstrap token (RUSTRAK_BOOTSTRAP_TOKEN)",
    )
    .await?;
    info!("Bootstrap token created from RUSTRAK_BOOTSTRAP_TOKEN");
    Ok(())
}

/// Bootstrap initial superuser from the CREATE_SUPERUSER env var, which the
/// caller reads, so nothing here depends on process-wide state.
/// Format: "email:password"
/// Only creates user if database is empty
pub async fn create_superuser_if_needed(
    pool: &DbPool,
    create_superuser: Option<&str>,
) -> AppResult<()> {
    let create_superuser = match create_superuser {
        Some(val) if !val.is_empty() => val,
        _ => {
            info!("CREATE_SUPERUSER not set, skipping superuser creation");
            return Ok(());
        }
    };

    // Check if any users exist
    let user_count = UsersService::user_count(pool).await?;
    if user_count > 0 {
        warn!("CREATE_SUPERUSER set but users already exist. Skipping superuser creation.");
        return Ok(());
    }

    // Parse email:password
    let parts: Vec<&str> = create_superuser.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(crate::error::AppError::Validation(
            "CREATE_SUPERUSER must be in format 'email:password'".to_string(),
        ));
    }

    let email = crate::models::User::normalize_email(parts[0]);
    let password = parts[1];

    if password.is_empty() {
        return Err(crate::error::AppError::Validation(
            "Password is required".to_string(),
        ));
    }

    // Create superuser
    let req = CreateUserRequest {
        email: email.clone(),
        password: password.to_string(),
    };

    UsersService::create_user(pool, &req, UserRole::Admin).await?;
    info!("✅ Superuser created: {}", email);

    Ok(())
}

/// Why `rustrak.toml` could not be applied. The server does not start.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ApplyError(String);

/// Applies `rustrak.toml` to the database, at every start, before the server
/// listens.
///
/// - **Projects** are identified by `id`. A new one is created with the
///   declared id and key; an existing one has its name, slug, platform and
///   disabled products brought in line with the file, and only the fields the
///   file sets. Its key never changes: a different key is refused. A project
///   left out of the file is left alone, events and all.
/// - **Tokens** are identified by `name`. The file is their source of truth:
///   a new value rotates one, and a name removed from the file revokes it.
///   Tokens created from the dashboard are never touched.
///
/// Every key is checked before anything is written. Renames are applied
/// together, so declared projects may exchange names and slugs; the tokens are
/// applied together too. Otherwise each item is applied on its own and an
/// error stops at that item; applying the same file again picks up where it
/// stopped.
pub async fn apply(pool: &DbPool, file: &BootstrapFile) -> Result<(), ApplyError> {
    // Projects first: they check every key before writing anything.
    apply_projects(pool, &file.projects).await?;
    if let Some(su) = &file.superuser {
        apply_superuser(pool, su)
            .await
            .map_err(|e| ApplyError(format!("superuser: {e}")))?;
    }
    // At every start rather than after each insert, so a start that stopped
    // between creating a project and this is repaired by the next one. Nothing
    // creates a project before apply has finished.
    if !file.projects.is_empty() {
        ProjectService::sync_id_sequence(pool)
            .await
            .map_err(|e| ApplyError(format!("moving the project id sequence: {e}")))?;
    }
    let declared: Vec<DeclaredToken<'_>> = file
        .tokens
        .iter()
        .map(|t| DeclaredToken {
            name: &t.name,
            raw: &t.token,
            description: t.description.as_deref(),
        })
        .collect();
    let revoked = AuthTokenService::sync_declared(pool, &declared)
        .await
        .map_err(|e| ApplyError(format!("tokens: {e}")))?;
    for name in revoked {
        info!("rustrak.toml: revoked token {name:?}, no longer in the file");
    }
    Ok(())
}

/// The first owner, as `CREATE_SUPERUSER` creates it: only on an instance
/// with no users yet, and never touched after.
async fn apply_superuser(pool: &DbPool, su: &SuperuserSpec) -> AppResult<()> {
    if UsersService::user_count(pool).await? > 0 {
        return Ok(());
    }
    let email = User::normalize_email(&su.email);
    let req = CreateUserRequest {
        email: email.clone(),
        password: su.password.clone(),
    };
    UsersService::create_user(pool, &req, UserRole::Admin).await?;
    info!("rustrak.toml: created superuser {email}");
    Ok(())
}

/// Prefixes an error with the project it is about.
fn at(id: i32) -> impl Fn(String) -> ApplyError {
    move |e| ApplyError(format!("project {id}: {e}"))
}

/// Existing projects first, so a new one can take a name or slug an existing
/// one gives up in the same file.
async fn apply_projects(pool: &DbPool, specs: &[ProjectSpec]) -> Result<(), ApplyError> {
    let mut existing = Vec::new();
    let mut new = Vec::new();
    let mut renames = Vec::new();
    for spec in specs {
        let stored: Option<(Uuid, bool)> =
            sqlx::query_as("SELECT sentry_key, deleted_at IS NOT NULL FROM projects WHERE id = $1")
                .bind(spec.id)
                .fetch_optional(pool)
                .await
                .map_err(|e| at(spec.id)(e.to_string()))?;
        match stored {
            Some((_, true)) => {
                // Its rows are still being removed in the background. Reviving
                // it would hand its DSN to whatever is left; once the deletion
                // finishes, the next start creates it again.
                warn!(
                    "rustrak.toml: project {} is being deleted; it will be created again on a later start",
                    spec.id
                );
            }
            Some((key, false)) => {
                if let Some(declared) = spec.sentry_key.filter(|&k| k != key) {
                    return Err(at(spec.id)(format!(
                        "the file declares sentry_key {} but the project's is {}. A key never \
                         changes, since every client using the DSN would stop reporting",
                        declared.simple(),
                        key.simple()
                    )));
                }
                let current = ProjectService::get_by_id(pool, spec.id)
                    .await
                    .map_err(|e| at(spec.id)(e.to_string()))?;
                let name = spec.name.trim().to_string();
                let slug = spec
                    .slug
                    .as_deref()
                    .map(slugify)
                    .unwrap_or(current.slug.clone());
                if name != current.name || slug != current.slug {
                    renames.push(ProjectRename {
                        id: spec.id,
                        name,
                        slug,
                    });
                }
                existing.push(spec);
            }
            None => {
                if let Some(key) = spec.sentry_key {
                    let holder: Option<i32> =
                        sqlx::query_scalar("SELECT id FROM projects WHERE sentry_key = $1")
                            .bind(key)
                            .fetch_optional(pool)
                            .await
                            .map_err(|e| at(spec.id)(e.to_string()))?;
                    if let Some(holder) = holder {
                        return Err(at(spec.id)(format!(
                            "sentry_key {} is already the key of project {holder}",
                            key.simple()
                        )));
                    }
                }
                new.push(spec);
            }
        }
    }
    // Declared slugs first, so a slug derived from another project's name
    // steps aside (`-1`) instead of taking it.
    new.sort_by_key(|spec| spec.slug.is_none());

    ProjectService::rename_many(pool, &renames)
        .await
        .map_err(|e| ApplyError(e.to_string()))?;
    for r in &renames {
        info!(
            "rustrak.toml: renamed project {} to {} ({})",
            r.id, r.name, r.slug
        );
    }
    for spec in existing {
        reconcile_project(pool, spec)
            .await
            .map_err(|e| at(spec.id)(e.to_string()))?;
    }
    for spec in new {
        create_project(pool, spec).await.map_err(at(spec.id))?;
    }
    Ok(())
}

async fn create_project(pool: &DbPool, spec: &ProjectSpec) -> Result<(), String> {
    // A declared key was checked in `apply_projects`.
    let key = spec.sentry_key.unwrap_or_else(Uuid::new_v4);
    let input = CreateProject {
        name: spec.name.trim().to_string(),
        slug: spec.slug.clone(),
        platform: spec.platform.clone(),
    };
    ProjectService::create_with_identity(pool, input, spec.id, key)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(off) = spec
        .disabled_products
        .as_ref()
        .filter(|off| !off.is_empty())
    {
        let update = UpdateProject {
            disabled_products: Some(off.clone()),
            ..Default::default()
        };
        ProjectService::update(pool, spec.id, update)
            .await
            .map_err(|e| e.to_string())?;
    }
    info!(
        "rustrak.toml: created project {} ({})",
        spec.id,
        spec.name.trim()
    );
    Ok(())
}

/// Everything but the name and slug, which [`apply_projects`] renames.
/// Writes only what differs, so a restart with an unchanged file writes nothing.
async fn reconcile_project(pool: &DbPool, spec: &ProjectSpec) -> AppResult<()> {
    let current = ProjectService::get_by_id(pool, spec.id).await?;
    let mut update = UpdateProject::default();
    let mut changed = false;

    if let Some(platform) = spec
        .platform
        .as_deref()
        .filter(|&p| current.platform.as_deref() != Some(p))
    {
        update.platform = Some(platform.to_string());
        changed = true;
    }
    if let Some(off) = spec
        .disabled_products
        .as_ref()
        .filter(|off| !same_products(off, &current.disabled_products.0))
    {
        update.disabled_products = Some(off.clone());
        changed = true;
    }

    if changed {
        ProjectService::update(pool, spec.id, update).await?;
        info!("rustrak.toml: updated project {}", spec.id);
    }
    Ok(())
}

/// Order carries no meaning in the set of disabled products.
fn same_products(a: &[Product], b: &[Product]) -> bool {
    a.iter().all(|p| b.contains(p)) && b.iter().all(|p| a.contains(p))
}
