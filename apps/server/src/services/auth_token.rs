use sha2::{Digest, Sha256};

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::rbac::{Action, Grants, Resource};
use crate::models::token::generate_token;
use crate::models::{AuthToken, CreateAuthToken, Permission};

pub struct AuthTokenService;

/// A token `rustrak.toml` declares: its name there, its raw value and its
/// description.
pub struct DeclaredToken<'a> {
    pub name: &'a str,
    pub raw: &'a str,
    pub description: Option<&'a str>,
}

const COLUMNS: &str = "id, token, token_prefix, description, user_id, created_at, last_used_at, expires_at, all_permissions, all_projects";

/// The form a token is stored and looked up in.
fn hash(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

impl AuthTokenService {
    /// Fills in the permissions and projects a token is limited to.
    async fn load_scope(pool: &DbPool, mut tokens: Vec<AuthToken>) -> AppResult<Vec<AuthToken>> {
        if tokens.is_empty() {
            return Ok(tokens);
        }
        // Integers from our own rows: nothing a caller wrote reaches the SQL.
        let ids = tokens
            .iter()
            .map(|t| t.id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let perms: Vec<(i32, String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT token_id, resource, action FROM auth_token_permissions WHERE token_id IN ({ids})"
        )))
        .fetch_all(pool)
        .await?;
        let projects: Vec<(i32, i32)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT token_id, project_id FROM auth_token_projects WHERE token_id IN ({ids}) ORDER BY project_id"
        )))
        .fetch_all(pool)
        .await?;
        for t in &mut tokens {
            t.permissions = perms
                .iter()
                .filter(|(id, _, _)| *id == t.id)
                .filter_map(|(_, r, a)| {
                    let (resource, action) = (Resource::parse(r)?, Action::parse(a)?);
                    resource
                        .accepts(action)
                        .then_some(Permission { resource, action })
                })
                .collect();
            t.project_ids = projects
                .iter()
                .filter(|(id, _)| *id == t.id)
                .map(|(_, p)| *p)
                .collect();
        }
        Ok(tokens)
    }

    async fn fetch(pool: &DbPool, filter: &str, bind: Option<i32>) -> AppResult<Vec<AuthToken>> {
        let sql = format!("SELECT {COLUMNS} FROM auth_tokens {filter} ORDER BY created_at DESC");
        let mut q = sqlx::query_as::<_, AuthToken>(sqlx::AssertSqlSafe(sql));
        if let Some(b) = bind {
            q = q.bind(b);
        }
        Self::load_scope(pool, q.fetch_all(pool).await?).await
    }

    /// Lists all tokens
    pub async fn list(pool: &DbPool) -> AppResult<Vec<AuthToken>> {
        Self::fetch(pool, "", None).await
    }

    /// Lists tokens owned by a specific user, most recent first.
    pub async fn list_for_user(pool: &DbPool, user_id: i32) -> AppResult<Vec<AuthToken>> {
        Self::fetch(pool, "WHERE user_id = $1", Some(user_id)).await
    }

    /// Gets a token by ID
    pub async fn get_by_id(pool: &DbPool, id: i32) -> AppResult<AuthToken> {
        Self::fetch(pool, "WHERE id = $1", Some(id))
            .await?
            .pop()
            .ok_or_else(|| AppError::NotFound(format!("Token with id {} not found", id)))
    }

    /// Looks a presented raw token up by its hash (for authentication).
    pub async fn get_by_token(pool: &DbPool, raw: &str) -> AppResult<Option<AuthToken>> {
        let sql = format!("SELECT {COLUMNS} FROM auth_tokens WHERE token = $1");
        let found = sqlx::query_as::<_, AuthToken>(sqlx::AssertSqlSafe(sql))
            .bind(hash(raw))
            .fetch_optional(pool)
            .await?;
        Ok(Self::load_scope(pool, found.into_iter().collect())
            .await?
            .pop())
    }

    /// Creates a new legacy/instance-level token (no owning user).
    pub async fn create(pool: &DbPool, input: CreateAuthToken) -> AppResult<AuthToken> {
        Self::create_for_user(pool, input, None).await
    }

    /// Creates a token, optionally owned by a user (`None` = legacy/instance
    /// token). The returned token's `token` is the raw value, the only time
    /// it exists: only its hash is stored.
    pub async fn create_for_user(
        pool: &DbPool,
        input: CreateAuthToken,
        user_id: Option<i32>,
    ) -> AppResult<AuthToken> {
        let raw = generate_token();
        let mut tx = pool.begin().await?;
        let (id,): (i32,) = sqlx::query_as(
            r#"
            INSERT INTO auth_tokens (token, token_prefix, description, user_id, expires_at, all_permissions, all_projects)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(hash(&raw))
        .bind(&raw[..8])
        .bind(&input.description)
        .bind(user_id)
        .bind(input.expires_at)
        .bind(input.permissions.is_none())
        .bind(input.project_ids.is_none())
        .fetch_one(&mut *tx)
        .await?;
        let grants = Grants::of(
            input
                .permissions
                .iter()
                .flatten()
                .map(|p| (p.resource, p.action)),
        );
        for (r, a) in grants.pairs() {
            sqlx::query(
                "INSERT INTO auth_token_permissions (token_id, resource, action) VALUES ($1, $2, $3)",
            )
            .bind(id)
            .bind(r.key())
            .bind(a.key())
            .execute(&mut *tx)
            .await?;
        }
        for project_id in input.project_ids.iter().flatten() {
            sqlx::query(
                "INSERT INTO auth_token_projects (token_id, project_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| match e {
                sqlx::Error::Database(d) if d.is_foreign_key_violation() => {
                    AppError::NotFound(format!("Project {project_id} not found"))
                }
                other => other.into(),
            })?;
        }
        tx.commit().await?;
        let mut token = Self::get_by_id(pool, id).await?;
        token.token = raw;
        Ok(token)
    }

    /// Hashes tokens written before tokens were stored hashed. A raw token is
    /// 40 hex characters and a hash 64, so this is safe to run at every start.
    pub async fn hash_plaintext_tokens(pool: &DbPool) -> AppResult<u64> {
        let plain: Vec<(i32, String)> =
            sqlx::query_as("SELECT id, token FROM auth_tokens WHERE length(token) = 40")
                .fetch_all(pool)
                .await?;
        for (id, raw) in &plain {
            sqlx::query("UPDATE auth_tokens SET token = $1 WHERE id = $2")
                .bind(hash(raw.trim()))
                .bind(id)
                .execute(pool)
                .await?;
        }
        Ok(plain.len() as u64)
    }

    /// Deletes a token (revoke)
    pub async fn delete(pool: &DbPool, id: i32) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM auth_tokens WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Token with id {} not found",
                id
            )));
        }

        Ok(())
    }

    /// Updates last_used_at timestamp
    pub async fn update_last_used(pool: &DbPool, id: i32) -> AppResult<()> {
        sqlx::query("UPDATE auth_tokens SET last_used_at = CURRENT_TIMESTAMP WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Makes the instance tokens `rustrak.toml` declares exactly the ones in
    /// `declared`, in one transaction: a name no longer declared is revoked, a
    /// new one is created, and one whose value or description changed is
    /// updated in place (a new value revokes the old one: that is how a
    /// declared token rotates). Tokens created any other way are never
    /// touched, and a refusal leaves every declared token as it was. Returns
    /// the names revoked.
    pub async fn sync_declared(
        pool: &DbPool,
        declared: &[DeclaredToken<'_>],
    ) -> AppResult<Vec<String>> {
        let mut tx = pool.begin().await?;
        let stored: Vec<(i32, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, config_name, token, description FROM auth_tokens \
             WHERE config_name IS NOT NULL",
        )
        .fetch_all(&mut *tx)
        .await?;

        let mut revoked = Vec::new();
        for (id, name, _, _) in &stored {
            if !declared.iter().any(|d| d.name == name) {
                sqlx::query("DELETE FROM auth_tokens WHERE id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                revoked.push(name.clone());
            }
        }

        // A changed value may still be another declared row's (two names
        // swapping values, say), and `token` is unique: every changing row
        // first gives its value up for a placeholder no hash can equal.
        let wanted: Vec<(&DeclaredToken<'_>, String)> =
            declared.iter().map(|d| (d, hash(d.raw))).collect();
        for (d, hashed) in &wanted {
            if let Some((id, ..)) = stored
                .iter()
                .find(|(_, name, token, _)| name == d.name && token != hashed)
            {
                sqlx::query("UPDATE auth_tokens SET token = $1 WHERE id = $2")
                    .bind(format!("rotating-{id}"))
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        for (d, hashed) in &wanted {
            let current = stored.iter().find(|(_, name, ..)| name == d.name);
            let result = match current {
                Some((_, _, token, desc)) if token == hashed && desc.as_deref() == d.description => {
                    continue;
                }
                Some((id, ..)) => {
                    sqlx::query(
                        "UPDATE auth_tokens SET token = $1, token_prefix = $2, description = $3 \
                         WHERE id = $4",
                    )
                    .bind(hashed)
                    .bind(&d.raw[..8])
                    .bind(d.description)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                }
                None => {
                    sqlx::query(
                        "INSERT INTO auth_tokens \
                         (token, token_prefix, description, user_id, all_permissions, all_projects, config_name) \
                         VALUES ($1, $2, $3, NULL, TRUE, TRUE, $4)",
                    )
                    .bind(hashed)
                    .bind(&d.raw[..8])
                    .bind(d.description)
                    .bind(d.name)
                    .execute(&mut *tx)
                    .await
                }
            };
            match result {
                Ok(_) => {}
                // Never the value in the message: it is a credential.
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                    return Err(AppError::Conflict(format!(
                        "token {:?}: its value is already another API token's",
                        d.name
                    )));
                }
                Err(e) => return Err(e.into()),
            }
        }

        tx.commit().await?;
        Ok(revoked)
    }

    /// Checks if any tokens exist (for bootstrap check)
    /// Stores `raw` as a token that acts on the whole instance with every
    /// permission, owned by nobody. Only its hash and prefix are kept.
    pub async fn create_instance_token(
        pool: &DbPool,
        raw: &str,
        description: &str,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO auth_tokens (token, token_prefix, description, user_id, all_permissions, all_projects) \
             VALUES ($1, $2, $3, NULL, TRUE, TRUE)",
        )
        .bind(hash(raw))
        .bind(&raw[..8])
        .bind(description)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn has_any_token(pool: &DbPool) -> AppResult<bool> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM auth_tokens")
            .fetch_one(pool)
            .await?;

        Ok(count.0 > 0)
    }
}
