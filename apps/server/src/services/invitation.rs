use chrono::{Duration, Utc};

use crate::auth::generate_token;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::{CreateInvitation, CreateUserRequest, Invitation, User, UserRole};
use crate::services::UsersService;

/// How long an invitation link stays valid.
const INVITATION_TTL_HOURS: i64 = 48;

pub struct InvitationService;

impl InvitationService {
    /// Creates a pending invitation. Rejects if the email already belongs to a
    /// user or already has a pending invitation.
    pub async fn create(
        pool: &DbPool,
        input: CreateInvitation,
        invited_by: i32,
    ) -> AppResult<Invitation> {
        let role = UserRole::parse(&input.role)
            .ok_or_else(|| AppError::Validation(format!("Invalid role: {}", input.role)))?;

        let email = User::normalize_email(&input.email);

        if !crate::routes::auth::is_valid_email(&email) {
            return Err(AppError::Validation("Invalid email format".to_string()));
        }

        if UsersService::get_by_email(pool, &email).await?.is_some() {
            return Err(
                AppError::Conflict("A user with that email already exists".to_string())
                    .with_field("email", FieldErrorCode::AlreadyExists),
            );
        }

        if Self::pending_for_email(pool, &email).await?.is_some() {
            return Err(AppError::Conflict(
                "A pending invitation for that email already exists".to_string(),
            )
            .with_field("email", FieldErrorCode::AlreadyExists));
        }

        let token = generate_token();
        let expires_at = Utc::now() + Duration::hours(INVITATION_TTL_HOURS);

        let invitation = sqlx::query_as::<_, Invitation>(
            r#"
            INSERT INTO invitations (token, email, role, status, expires_at, invited_by)
            VALUES ($1, $2, $3, 'pending', $4, $5)
            RETURNING token, email, role, status, expires_at, invited_by, created_at, accepted_at
            "#,
        )
        .bind(&token)
        .bind(&email)
        .bind(role.as_str())
        .bind(expires_at)
        .bind(invited_by)
        .fetch_one(pool)
        .await?;

        Ok(invitation)
    }

    /// Fetches an invitation by its token.
    pub async fn get(pool: &DbPool, token: &str) -> AppResult<Option<Invitation>> {
        let invitation = sqlx::query_as::<_, Invitation>(
            r#"
            SELECT token, email, role, status, expires_at, invited_by, created_at, accepted_at
            FROM invitations WHERE token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(pool)
        .await?;

        Ok(invitation)
    }

    /// Lists all invitations, newest first.
    pub async fn list(pool: &DbPool) -> AppResult<Vec<Invitation>> {
        let invitations = sqlx::query_as::<_, Invitation>(
            r#"
            SELECT token, email, role, status, expires_at, invited_by, created_at, accepted_at
            FROM invitations ORDER BY created_at DESC
            "#,
        )
        .fetch_all(pool)
        .await?;

        Ok(invitations)
    }

    async fn pending_for_email(pool: &DbPool, email: &str) -> AppResult<Option<Invitation>> {
        let invitation = sqlx::query_as::<_, Invitation>(
            r#"
            SELECT token, email, role, status, expires_at, invited_by, created_at, accepted_at
            FROM invitations WHERE LOWER(email) = LOWER($1) AND status = 'pending'
            "#,
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;

        Ok(invitation)
    }

    /// Accepts an invitation: creates the user with the invitation's email and
    /// role, then marks the invitation accepted. The new user's email is taken
    /// from the invitation (not from caller input) so it cannot be spoofed.
    pub async fn accept(pool: &DbPool, token: &str, password: &str) -> AppResult<User> {
        let invitation = Self::get(pool, token)
            .await?
            .ok_or_else(|| AppError::Validation("Invalid invitation token".to_string()))?;

        if !invitation.is_acceptable(Utc::now()) {
            return Err(AppError::Validation(
                "Invitation is expired or already used".to_string(),
            ));
        }

        // No length policy — accounts created via invitation use the same rule as
        // login (password is simply required, not length-restricted).
        if password.is_empty() {
            return Err(AppError::Validation("Password is required".to_string()));
        }

        // An invitation from before email normalization can name a case
        // variant of an existing account.
        if UsersService::get_by_email(pool, &invitation.email)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict(
                "A user with that email already exists".to_string(),
            ));
        }

        let role = UserRole::from_db(&invitation.role);
        let req = CreateUserRequest {
            email: invitation.email.clone(),
            password: password.to_string(),
        };

        // Create the user and consume the invitation atomically: if either step
        // fails the whole thing rolls back, so we never leave a created user with
        // an invitation still marked pending.
        // Write-first (INSERT opens the tx; the read happens above) — deferred
        // BEGIN deliberate, see db::begin_write.
        let mut tx = pool.begin().await?;

        let user = UsersService::create_user(&mut *tx, &req, role).await?;

        // Consume only if still pending — guards against a concurrent accept/
        // revoke that slipped in between the check above and now.
        let consumed = sqlx::query(
            "UPDATE invitations SET status = 'accepted', accepted_at = CURRENT_TIMESTAMP WHERE token = $1 AND status = 'pending'",
        )
        .bind(token)
        .execute(&mut *tx)
        .await?;

        if consumed.rows_affected() == 0 {
            // Someone else consumed/revoked it first — roll back the user.
            return Err(AppError::Validation(
                "Invitation is expired or already used".to_string(),
            ));
        }

        tx.commit().await?;

        Ok(user)
    }

    /// Revokes a pending invitation.
    pub async fn revoke(pool: &DbPool, token: &str) -> AppResult<()> {
        let result = sqlx::query(
            "UPDATE invitations SET status = 'revoked' WHERE token = $1 AND status = 'pending'",
        )
        .bind(token)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(
                "Pending invitation not found".to_string(),
            ));
        }

        Ok(())
    }
}
