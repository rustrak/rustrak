use sha2::{Digest, Sha256};

use crate::auth::generate_token;
use crate::auth::rbac::{Action, Grants, Resource};
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{AuthToken, CreateAuthToken, Permission};

pub struct AuthTokenService;

const COLUMNS: &str = "id, token, token_prefix, description, user_id, created_at, last_used_at, expires_at, all_permissions, all_projects";

/// The form a token is stored and looked up in.
fn hash(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

impl AuthTokenService {
    /// Fills in the permissions and projects a token is limited to.
    async fn load_scope(pool: &DbPool, mut tokens: Vec<AuthToken>) -> AppResult<Vec<AuthToken>> {
        let perms: Vec<(i32, String, String)> =
            sqlx::query_as("SELECT token_id, resource, action FROM auth_token_permissions")
                .fetch_all(pool)
                .await?;
        let projects: Vec<(i32, i32)> = sqlx::query_as(
            "SELECT token_id, project_id FROM auth_token_projects ORDER BY project_id",
        )
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

    /// Checks if any tokens exist (for bootstrap check)
    pub async fn has_any_token(pool: &DbPool) -> AppResult<bool> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM auth_tokens")
            .fetch_one(pool)
            .await?;

        Ok(count.0 > 0)
    }
}
