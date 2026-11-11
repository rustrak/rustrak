//! Dashboard sign-in sessions, kept in the `sessions` table.
//!
//! actix-session still owns the cookie and the request lifecycle; this is its
//! store. The cookie carries only an opaque random id and the table stores
//! that id's SHA-256, so a logout or a revocation ends a session for good
//! (OWASP Session Management Cheat Sheet; Django's DB sessions, which Sentry
//! uses).

use std::collections::HashMap;

use actix_session::storage::{LoadError, SaveError, SessionKey, SessionStore, UpdateError};
use actix_web::cookie::time::Duration as StateTtl;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

use crate::auth::session::SESSION_USER_ID_KEY;
use crate::db::DbPool;
use crate::error::AppResult;

/// Signed out after this long without a request: the state TTL actix-session
/// applied by default before sessions moved server-side.
fn idle_timeout() -> Duration {
    Duration::days(1)
}

/// Signed out this long after the session began, however active: Django's
/// `SESSION_COOKIE_AGE`, which Sentry keeps.
fn absolute_lifetime() -> Duration {
    Duration::days(14)
}

/// `last_seen_at` is written at most this often, so a dashboard polling the
/// API is not one SQLite write per request.
fn touch_interval() -> Duration {
    Duration::minutes(1)
}

fn hash(id: &str) -> String {
    hex::encode(Sha256::digest(id.as_bytes()))
}

/// The signed-in user a session's state names, if any. Values are JSON, and
/// an `i32` is its own JSON.
fn user_id(state: &HashMap<String, String>) -> Option<i32> {
    state.get(SESSION_USER_ID_KEY)?.parse().ok()
}

pub struct AuthSessionService;

impl AuthSessionService {
    /// Ends every session `user_id` has, everywhere. The one place a user is
    /// signed out from outside their own browser.
    pub async fn revoke_user_sessions(pool: &DbPool, user_id: i32) -> AppResult<u64> {
        let done = sqlx::query("DELETE FROM sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await?;
        Ok(done.rows_affected())
    }

    /// Deletes the sessions that can no longer be used: past their lifetime
    /// or idle too long.
    pub async fn purge_expired(pool: &DbPool) -> AppResult<u64> {
        let now = Utc::now();
        let done = sqlx::query("DELETE FROM sessions WHERE expires_at <= $1 OR last_seen_at <= $2")
            .bind(now)
            .bind(now - idle_timeout())
            .execute(pool)
            .await?;
        Ok(done.rows_affected())
    }
}

/// The actix-session store over the `sessions` table.
pub struct DbSessionStore {
    pool: DbPool,
}

impl DbSessionStore {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

impl SessionStore for DbSessionStore {
    async fn load(&self, key: &SessionKey) -> Result<Option<HashMap<String, String>>, LoadError> {
        let now = Utc::now();
        let id_hash = hash(key.as_ref());
        let row: Option<(String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT data, last_seen_at FROM sessions \
             WHERE id_hash = $1 AND expires_at > $2 AND last_seen_at > $3",
        )
        .bind(&id_hash)
        .bind(now)
        .bind(now - idle_timeout())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| LoadError::Other(e.into()))?;
        let Some((data, last_seen_at)) = row else {
            return Ok(None);
        };

        if last_seen_at < now - touch_interval() {
            sqlx::query("UPDATE sessions SET last_seen_at = $1 WHERE id_hash = $2")
                .bind(now)
                .bind(&id_hash)
                .execute(&self.pool)
                .await
                .map_err(|e| LoadError::Other(e.into()))?;
        }

        serde_json::from_str(&data)
            .map(Some)
            .map_err(|e| LoadError::Deserialization(e.into()))
    }

    async fn save(
        &self,
        state: HashMap<String, String>,
        _ttl: &StateTtl,
    ) -> Result<SessionKey, SaveError> {
        use rand::RngExt;
        // 256 bits from the thread's CSPRNG.
        let bytes: [u8; 32] = rand::rng().random();
        let id = hex::encode(bytes);
        let data = serde_json::to_string(&state).map_err(|e| SaveError::Serialization(e.into()))?;
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO sessions (id_hash, user_id, data, created_at, last_seen_at, expires_at) \
             VALUES ($1, $2, $3, $4, $4, $5)",
        )
        .bind(hash(&id))
        .bind(user_id(&state))
        .bind(data)
        .bind(now)
        .bind(now + absolute_lifetime())
        .execute(&self.pool)
        .await
        .map_err(|e| SaveError::Other(e.into()))?;
        id.try_into()
            .map_err(|e| SaveError::Other(anyhow::Error::new(e)))
    }

    /// A row that is gone (revoked while this request ran) stays gone: the
    /// write is dropped rather than bringing the session back.
    async fn update(
        &self,
        key: SessionKey,
        state: HashMap<String, String>,
        _ttl: &StateTtl,
    ) -> Result<SessionKey, UpdateError> {
        let data =
            serde_json::to_string(&state).map_err(|e| UpdateError::Serialization(e.into()))?;
        sqlx::query("UPDATE sessions SET data = $1, user_id = $2 WHERE id_hash = $3")
            .bind(data)
            .bind(user_id(&state))
            .bind(hash(key.as_ref()))
            .execute(&self.pool)
            .await
            .map_err(|e| UpdateError::Other(e.into()))?;
        Ok(key)
    }

    /// Never called: the middleware extends a session only when its state
    /// changes, and `load` keeps `last_seen_at` instead.
    async fn update_ttl(&self, _key: &SessionKey, _ttl: &StateTtl) -> Result<(), anyhow::Error> {
        Ok(())
    }

    async fn delete(&self, key: &SessionKey) -> Result<(), anyhow::Error> {
        sqlx::query("DELETE FROM sessions WHERE id_hash = $1")
            .bind(hash(key.as_ref()))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
