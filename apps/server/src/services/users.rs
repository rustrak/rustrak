use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::user::OidcIdentity;
use crate::models::{CreateUserRequest, User, UserRole};

pub struct UsersService;

/// Where an SSO login ends up.
#[derive(Debug)]
pub enum OidcOutcome {
    /// The identity resolved to an account; the browser can be signed in.
    SignedIn(User),
    /// The identity matches this existing account, but may only be linked to
    /// it once the account's password has been given.
    ConfirmWithPassword(User),
}

/// What an SSO login may do with an identity that is not linked to an
/// account yet.
#[derive(Debug, Clone, Default)]
pub struct OidcLinkPolicy {
    /// Create a local account when no account has the identity's email.
    pub auto_provision: bool,
    /// Attach the identity to an existing account with the same verified email.
    pub link_existing_accounts: bool,
    /// Groups that grant the global `admin` role. Empty leaves roles alone.
    pub admin_groups: Vec<String>,
}

impl OidcLinkPolicy {
    /// The role an identity's groups ask for, or `None` when the operator has
    /// not configured a mapping and roles are therefore left alone.
    pub fn role_for(&self, groups: &[String]) -> Option<UserRole> {
        let matched = self.matched_admin_groups(groups);
        self.role_from_matched(&matched)
    }

    /// The identity's groups that are admin groups right now.
    ///
    /// Bounded by the operator's list, never by what the provider sends, which
    /// is what makes it safe to keep in a session cookie until the account's
    /// password arrives.
    pub fn matched_admin_groups(&self, groups: &[String]) -> Vec<String> {
        self.admin_groups
            .iter()
            .filter(|admin| groups.iter().any(|group| group == *admin))
            .cloned()
            .collect()
    }

    /// The role for a link interrupted by the password step, decided against
    /// the configuration as it is now.
    ///
    /// The matched groups were read from the token minutes earlier; a group the
    /// operator has since removed from `OIDC_ADMIN_GROUPS`, or a mapping that
    /// has since been turned off, must not still grant admin.
    pub fn role_from_matched(&self, matched: &[String]) -> Option<UserRole> {
        if self.admin_groups.is_empty() {
            return None;
        }
        Some(
            if matched
                .iter()
                .any(|group| self.admin_groups.iter().any(|admin| admin == group))
            {
                UserRole::Admin
            } else {
                UserRole::Member
            },
        )
    }
}

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
    /// or a password-inaccessible account is provisioned; `policy` says which
    /// of the two are allowed.
    pub async fn find_or_provision_oidc(
        pool: &DbPool,
        identity: &OidcIdentity,
        policy: OidcLinkPolicy,
    ) -> AppResult<OidcOutcome> {
        let OidcIdentity {
            issuer,
            subject,
            email,
            email_verified,
            groups,
        } = identity;
        let (issuer, subject, email, email_verified) = (
            issuer.as_str(),
            subject.as_str(),
            email.as_str(),
            *email_verified,
        );
        let mapped_role = policy.role_for(groups);

        if let Some(mut user) = Self::get_by_oidc_identity(pool, issuer, subject).await? {
            user.ensure_active()?;
            Self::touch_oidc_login(pool, issuer, subject, &mut user, mapped_role).await?;
            return Ok(OidcOutcome::SignedIn(user));
        }

        let raw_email = email.trim();
        let normalized_email = User::normalize_email(email);

        // A provisioned account is OIDC-only: this password exists because the
        // column is NOT NULL, and nothing ever learns it. Hash it here, off the
        // Actix worker, instead of inside `create_user` below — Argon2 takes
        // ~100ms and must not run once the transaction holds the `users` lock,
        // where it would stall every other writer to that table (including
        // last-login updates from users who are already provisioned).
        let password_hash = if policy.auto_provision {
            Some(
                tokio::task::spawn_blocking(|| {
                    User::hash_password(&uuid::Uuid::new_v4().to_string())
                })
                .await
                .map_err(|e| AppError::Internal(format!("Password hashing task failed: {e}")))??,
            )
        } else {
            None
        };

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

        // Legacy rows written before normalization may differ only in casing.
        // When several match, only an exact-case match may be linked, so the
        // identity never lands on the wrong account.
        let ambiguous = candidates.len() > 1 && candidates[0].email != raw_email;
        if ambiguous {
            return Err(AppError::Forbidden(
                "Multiple legacy accounts match this email address; exact case match required to link SSO identity"
                    .to_string(),
            ));
        }
        let user = candidates.into_iter().next();

        let mut user = if let Some(existing) = user {
            existing.ensure_active()?;
            // Linking on the provider's word alone needs both the operator's
            // consent and a verified email; otherwise the owner confirms.
            if !policy.link_existing_accounts || !email_verified {
                return Ok(OidcOutcome::ConfirmWithPassword(existing));
            }
            existing
        } else {
            let Some(password_hash) = password_hash else {
                return Err(AppError::Forbidden(
                    "No Rustrak account is linked to this SSO identity".to_string(),
                ));
            };
            let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
                .fetch_one(&mut *tx)
                .await?;
            // The first account on an installation still bootstraps as admin:
            // a mapping that nobody satisfies must not leave a fresh instance
            // with no administrator at all.
            let role = if count.0 == 0 {
                UserRole::Admin
            } else {
                mapped_role.unwrap_or(UserRole::Member)
            };
            Self::create_user_with_password_hash(&mut *tx, &normalized_email, &password_hash, role)
                .await?
        };
        Self::apply_sso_role(&mut tx, &mut user, mapped_role).await?;

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
                .map(OidcOutcome::SignedIn)
                .ok_or_else(|| AppError::Internal("Failed to resolve SSO identity".to_string()));
        }

        sqlx::query("UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = $1")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(OidcOutcome::SignedIn(user))
    }

    /// Link an SSO identity to an existing account once its owner has given
    /// the account's password.
    pub async fn confirm_oidc_link(
        pool: &DbPool,
        identity: &OidcIdentity,
        user_id: i32,
        password: &str,
        role: Option<UserRole>,
    ) -> AppResult<User> {
        let invalid = || AppError::Unauthorized("Invalid credentials".to_string());
        let user = Self::get_by_id(pool, user_id).await?.ok_or_else(invalid)?;
        user.ensure_active()?;

        // Argon2 off the Actix worker, as account provisioning does.
        let (user, password) = (user, password.to_string());
        let (mut user, matches) = tokio::task::spawn_blocking(move || {
            let matches = user.verify_password(&password);
            (user, matches)
        })
        .await
        .map_err(|e| AppError::Internal(format!("Password check task failed: {e}")))?;
        if !matches? {
            return Err(invalid());
        }

        let mut tx = crate::db::begin_write(pool).await?;
        let inserted = sqlx::query(
            r#"
            INSERT INTO oidc_identities (user_id, issuer, subject, email_at_link)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (issuer, subject) DO NOTHING
            "#,
        )
        .bind(user.id)
        .bind(&identity.issuer)
        .bind(&identity.subject)
        .bind(identity.email.trim())
        .execute(&mut *tx)
        .await?;
        // The role follows the groups only once the account's owner has proved
        // they are the person behind the identity (the password above), and
        // only when the link actually landed: before that point anyone who can
        // authenticate at the provider with this email could change it.
        if inserted.rows_affected() > 0 {
            Self::apply_sso_role(&mut tx, &mut user, role).await?;
        }
        sqlx::query("UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = $1")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        if inserted.rows_affected() == 0 {
            // Linked meanwhile, by this confirmation in another tab or by an
            // automatic link. Only the same account may carry on.
            let owner = Self::get_by_oidc_identity(pool, &identity.issuer, &identity.subject)
                .await?
                .ok_or_else(|| AppError::Internal("Failed to resolve SSO identity".to_string()))?;
            if owner.id != user.id {
                return Err(AppError::Conflict(
                    "This SSO identity is already linked to another account".to_string(),
                ));
            }
        }

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
    /// and bring the account's role in line with the operator's group mapping,
    /// all in one transaction so the rows cannot disagree.
    async fn touch_oidc_login(
        pool: &DbPool,
        issuer: &str,
        subject: &str,
        user: &mut User,
        role: Option<UserRole>,
    ) -> AppResult<()> {
        let mut tx = crate::db::begin_write(pool).await?;
        Self::apply_sso_role(&mut tx, user, role).await?;
        sqlx::query(
            "UPDATE oidc_identities SET last_login = CURRENT_TIMESTAMP WHERE issuer = $1 AND subject = $2",
        )
        .bind(issuer)
        .bind(subject)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = $1")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Write an SSO-derived role, unless that would take admin away from the
    /// primary account.
    ///
    /// The primary account is the bootstrap superuser and is protected from
    /// demotion everywhere else; a group mapping must not become the way around
    /// that, or a misconfigured claim could lock the instance's owner out.
    async fn apply_sso_role(
        tx: &mut sqlx::Transaction<'_, crate::db::Db>,
        user: &mut User,
        role: Option<UserRole>,
    ) -> AppResult<()> {
        let Some(role) = role else {
            return Ok(());
        };
        if user.role == role.as_str() {
            return Ok(());
        }
        if Self::primary_user_id(&mut **tx).await? == Some(user.id)
            && user.is_admin()
            && role == UserRole::Member
        {
            return Ok(());
        }

        sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
            .bind(role.as_str())
            .bind(user.id)
            .execute(&mut **tx)
            .await?;
        user.role = role.as_str().to_string();
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
    ///
    /// Takes a pool or a transaction, so a write that guards the primary
    /// account can read it under the same lock.
    pub async fn primary_user_id<'e, E>(executor: E) -> AppResult<Option<i32>>
    where
        E: sqlx::Executor<'e, Database = crate::db::Db>,
    {
        let row: (Option<i32>,) = sqlx::query_as("SELECT MIN(id) FROM users")
            .fetch_one(executor)
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

    pub async fn change_password(
        pool: &DbPool,
        user: User,
        current_password: &str,
        new_password: &str,
    ) -> AppResult<()> {
        // Same rule as accepting an invitation: required, no length policy.
        if new_password.is_empty() {
            return Err(AppError::Validation("New password is required".to_string())
                .with_field("new_password", FieldErrorCode::Required));
        }

        let user_id = user.id;
        let verified_hash = user.password_hash.clone();
        // Argon2 off the Actix worker, as the SSO link confirmation does.
        let (current_password, new_password) =
            (current_password.to_string(), new_password.to_string());
        let password_hash = tokio::task::spawn_blocking(move || {
            if !user.verify_password(&current_password)? {
                return Err(
                    AppError::Validation("Current password is incorrect".to_string())
                        .with_field("current_password", FieldErrorCode::Incorrect),
                );
            }
            User::hash_password(&new_password)
        })
        .await
        .map_err(|e| AppError::Internal(format!("Password hashing task failed: {e}")))??;

        // Only the hash that was verified may be replaced: an overlapping
        // request that checked the same old password must not win.
        let updated =
            sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2 AND password_hash = $3")
                .bind(password_hash)
                .bind(user_id)
                .bind(verified_hash)
                .execute(pool)
                .await?;
        if updated.rows_affected() == 0 {
            return Err(
                AppError::Validation("Current password is incorrect".to_string())
                    .with_field("current_password", FieldErrorCode::Incorrect),
            );
        }
        // Whoever held the old password may hold a session too.
        crate::services::AuthSessionService::revoke_user_sessions(pool, user_id).await?;
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
