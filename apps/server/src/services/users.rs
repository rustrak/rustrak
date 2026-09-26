use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{CreateUserRequest, User, UserRole};

pub struct UsersService;

impl UsersService {
    /// Creates a new user with the given global role.
    ///
    /// Generic over the executor so it can run on a pool (`&DbPool`) or inside a
    /// transaction (`&mut *tx`) — the latter lets callers create a user and do
    /// follow-up writes atomically (e.g. consuming an invitation).
    pub async fn create_user<'e, E>(
        executor: E,
        req: &CreateUserRequest,
        role: UserRole,
    ) -> AppResult<User>
    where
        E: sqlx::Executor<'e, Database = crate::db::Db>,
    {
        let password_hash = User::hash_password(&req.password)?;
        let email = User::normalize_email(&req.email);
        Self::create_user_with_password_hash(executor, &email, &password_hash, role).await
    }

    /// Creates a user from an already-computed password hash.
    ///
    /// Separate from [`Self::create_user`] so a caller that must not run Argon2
    /// itself — the OIDC provisioning path, which holds the `users` lock — can
    /// pay for the hash before it opens its transaction.
    pub async fn create_user_with_password_hash<'e, E>(
        executor: E,
        email: &str,
        password_hash: &str,
        role: UserRole,
    ) -> AppResult<User>
    where
        E: sqlx::Executor<'e, Database = crate::db::Db>,
    {
        let email = User::normalize_email(email);
        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (email, password_hash, role)
            VALUES ($1, $2, $3)
            RETURNING id, email, password_hash, is_active, role, created_at, last_login, language, timezone
            "#,
        )
        .bind(&email)
        .bind(password_hash)
        .bind(role.as_str())
        .fetch_one(executor)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
                AppError::Validation("Email already exists".to_string())
            }
            _ => AppError::Internal(format!("Failed to create user: {}", e)),
        })?;

        Ok(user)
    }

    /// Gets a user by email, case-insensitively. Rows written before
    /// normalization may differ only in casing; an exact match wins, then
    /// the oldest account.
    pub async fn get_by_email(pool: &DbPool, email: &str) -> AppResult<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash, is_active, role, created_at, last_login, language, timezone
            FROM users
            WHERE LOWER(email) = LOWER($1)
            ORDER BY (email = $1) DESC, id ASC
            LIMIT 1
            "#,
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;

        Ok(user)
    }

    /// Resolve an immutable OIDC identity to a local user. On first login the
    /// identity is linked to an existing account with the same verified email,
    /// or a password-inaccessible local account is provisioned when enabled.
    pub async fn find_or_provision_oidc(
        pool: &DbPool,
        issuer: &str,
        subject: &str,
        email: &str,
        email_verified: bool,
        auto_provision: bool,
    ) -> AppResult<User> {
        if let Some(user) = Self::get_by_oidc_identity(pool, issuer, subject).await? {
            if !user.is_active {
                return Err(AppError::Unauthorized("Account is disabled".to_string()));
            }
            Self::touch_oidc_login(pool, issuer, subject, user.id).await?;
            return Ok(user);
        }

        if !auto_provision {
            return Err(AppError::Forbidden(
                "No Rustrak account is linked to this SSO identity".to_string(),
            ));
        }

        let raw_email = email.trim();
        let normalized_email = User::normalize_email(email);

        // A provisioned account is OIDC-only: this password exists because the
        // column is NOT NULL, and nothing ever learns it. Hash it here, off the
        // Actix worker, instead of inside `create_user` below — Argon2 takes
        // ~100ms and must not run once the transaction holds the `users` lock,
        // where it would stall every other writer to that table (including
        // last-login updates from users who are already provisioned).
        let password_hash =
            tokio::task::spawn_blocking(|| User::hash_password(&uuid::Uuid::new_v4().to_string()))
                .await
                .map_err(|e| AppError::Internal(format!("Password hashing task failed: {e}")))??;

        let mut tx = crate::db::begin_write(pool).await?;

        // SQLite's BEGIN IMMEDIATE already serializes this read-then-write
        // sequence. PostgreSQL needs an explicit lock so two simultaneous
        // first logins cannot both observe an empty users table and become
        // administrators.
        #[cfg(feature = "postgres")]
        sqlx::query("LOCK TABLE users IN SHARE ROW EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await?;

        let candidates = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash, is_active, role, created_at, last_login, language, timezone
            FROM users
            WHERE LOWER(email) = LOWER($1)
            ORDER BY (email = $2) DESC, id ASC
            "#,
        )
        .bind(&normalized_email)
        .bind(raw_email)
        .fetch_all(&mut *tx)
        .await?;

        let user = match candidates.len() {
            0 => None,
            1 => Some(candidates.into_iter().next().unwrap()),
            _ => {
                // Legacy rows written before normalization may differ only in casing.
                // When multiple accounts match, require an exact case match with the
                // provider email to prevent linking the identity to the wrong account.
                if candidates[0].email == raw_email {
                    Some(candidates.into_iter().next().unwrap())
                } else {
                    return Err(AppError::Forbidden(
                        "Multiple legacy accounts match this email address; exact case match required to link SSO identity"
                            .to_string(),
                    ));
                }
            }
        };

        let user = if let Some(existing) = user {
            if !existing.is_active {
                return Err(AppError::Unauthorized("Account is disabled".to_string()));
            }
            if !email_verified {
                return Err(AppError::Forbidden(
                    "Cannot link an unverified SSO email to an existing account".to_string(),
                ));
            }
            existing
        } else {
            let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
                .fetch_one(&mut *tx)
                .await?;
            let role = if count.0 == 0 {
                UserRole::Admin
            } else {
                UserRole::Member
            };
            Self::create_user_with_password_hash(&mut *tx, &normalized_email, &password_hash, role)
                .await?
        };

        let inserted = sqlx::query(
            r#"
            INSERT INTO oidc_identities (user_id, issuer, subject, email_at_link)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (issuer, subject) DO NOTHING
            "#,
        )
        .bind(user.id)
        .bind(issuer)
        .bind(subject)
        .bind(raw_email)
        .execute(&mut *tx)
        .await?;

        if inserted.rows_affected() == 0 {
            // Another callback linked the same identity concurrently. Avoid
            // committing a now-unreferenced user and resolve the winner.
            tx.rollback().await?;
            return Self::get_by_oidc_identity(pool, issuer, subject)
                .await?
                .ok_or_else(|| AppError::Internal("Failed to resolve SSO identity".to_string()));
        }

        sqlx::query("UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = $1")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(user)
    }

    /// Look up the local account already linked to an `(issuer, subject)` pair.
    /// The pair is immutable, so a provider-side email change does not move the
    /// account; only an explicit new link does.
    async fn get_by_oidc_identity(
        pool: &DbPool,
        issuer: &str,
        subject: &str,
    ) -> AppResult<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT u.id, u.email, u.password_hash, u.is_active, u.role,
                   u.created_at, u.last_login, u.language, u.timezone
            FROM users u
            INNER JOIN oidc_identities oi ON oi.user_id = u.id
            WHERE oi.issuer = $1 AND oi.subject = $2
            "#,
        )
        .bind(issuer)
        .bind(subject)
        .fetch_optional(pool)
        .await?;
        Ok(user)
    }

    /// Stamp a successful SSO login on both the identity and the local account,
    /// in one transaction so the two cannot disagree about when the user last
    /// signed in.
    async fn touch_oidc_login(
        pool: &DbPool,
        issuer: &str,
        subject: &str,
        user_id: i32,
    ) -> AppResult<()> {
        let mut tx = crate::db::begin_write(pool).await?;
        sqlx::query(
            "UPDATE oidc_identities SET last_login = CURRENT_TIMESTAMP WHERE issuer = $1 AND subject = $2",
        )
        .bind(issuer)
        .bind(subject)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Gets a user by ID
    pub async fn get_by_id(pool: &DbPool, user_id: i32) -> AppResult<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash, is_active, role, created_at, last_login, language, timezone
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

        Ok(user)
    }

    /// Lists all users (team roster), most recent first.
    pub async fn list(pool: &DbPool) -> AppResult<Vec<User>> {
        let users = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash, is_active, role, created_at, last_login, language, timezone
            FROM users
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(pool)
        .await?;

        Ok(users)
    }

    /// The primary user = the first-registered account (lowest id), i.e. the
    /// bootstrap superuser Rustrak was set up with. It is protected from demotion
    /// and deletion. Returns `None` only when there are no users.
    pub async fn primary_user_id(pool: &DbPool) -> AppResult<Option<i32>> {
        let row: (Option<i32>,) = sqlx::query_as("SELECT MIN(id) FROM users")
            .fetch_one(pool)
            .await?;

        Ok(row.0)
    }

    /// Permanently deletes a user. Memberships and owned tokens cascade
    /// (`ON DELETE CASCADE`); invitations they sent keep `invited_by = NULL`.
    pub async fn delete(pool: &DbPool, user_id: i32) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("User {} not found", user_id)));
        }

        Ok(())
    }

    /// Updates a user's global role.
    pub async fn update_role(pool: &DbPool, user_id: i32, role: UserRole) -> AppResult<()> {
        let result = sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
            .bind(role.as_str())
            .bind(user_id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("User {} not found", user_id)));
        }

        Ok(())
    }

    /// Updates the last login timestamp for a user
    pub async fn update_last_login(pool: &DbPool, user_id: i32) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET last_login = CURRENT_TIMESTAMP
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Writes the reader's dashboard preferences.
    ///
    /// Both are `Option<Option<String>>` shaped by the caller: the outer level
    /// is "was this field in the request", the inner is "what was it set to".
    /// A `PATCH` that names only `language` must leave `timezone` alone, and a
    /// `PATCH` that sets a field to `null` must clear it -- one `Option` cannot
    /// tell those two apart.
    pub async fn update_preferences(
        pool: &DbPool,
        user_id: i32,
        language: Option<Option<String>>,
        timezone: Option<Option<String>>,
    ) -> AppResult<()> {
        if let Some(language) = language {
            sqlx::query("UPDATE users SET language = $1 WHERE id = $2")
                .bind(language)
                .bind(user_id)
                .execute(pool)
                .await?;
        }
        if let Some(timezone) = timezone {
            sqlx::query("UPDATE users SET timezone = $1 WHERE id = $2")
                .bind(timezone)
                .bind(user_id)
                .execute(pool)
                .await?;
        }
        Ok(())
    }

    /// Counts total number of users
    pub async fn user_count(pool: &DbPool) -> AppResult<i64> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM users
            "#,
        )
        .fetch_one(pool)
        .await?;

        Ok(count.0)
    }

    /// Counts how many *active* admins exist (to prevent demoting the last usable
    /// admin and locking the instance out — inactive admins cannot act).
    pub async fn admin_count(pool: &DbPool) -> AppResult<i64> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = 'admin' AND is_active = true")
                .fetch_one(pool)
                .await?;

        Ok(count.0)
    }
}
