use chrono::Utc;

use crate::config::RateLimitConfig;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::{Installation, Project, QuotaWindows};

pub struct RateLimitService;

/// Counts one event in the current minute and hour windows of `$table`,
/// unless either is already full. A row whose stored window is older starts
/// over at one. `$1`/`$2` are the window numbers, `$3`/`$4` the limits.
macro_rules! consume_sql {
    ($table:literal, $row:literal, $minute_limit:literal, $hour_limit:literal) => {
        concat!(
            "UPDATE ", $table, " SET ",
            "quota_minute_count = CASE WHEN quota_minute_window = $1 THEN quota_minute_count + 1 ELSE 1 END, ",
            "quota_minute_window = $1, ",
            "quota_hour_count = CASE WHEN quota_hour_window = $2 THEN quota_hour_count + 1 ELSE 1 END, ",
            "quota_hour_window = $2 ",
            "WHERE ", $row, " ",
            "AND (quota_minute_window <> $1 OR quota_minute_count < ", $minute_limit, ") ",
            "AND (quota_hour_window <> $2 OR quota_hour_count < ", $hour_limit, ")"
        )
    };
}

/// Seconds until a full window of `quota` rolls over, or `None` while both
/// the current minute and the current hour still have room.
fn seconds_while_full(
    quota: QuotaWindows,
    (minute_limit, hour_limit): (i64, i64),
    now: chrono::DateTime<Utc>,
) -> Option<u64> {
    let full = |window: i64, count: i64, limit: i64, length: i64| {
        (window == now.timestamp().div_euclid(length) && count >= limit)
            .then(|| (length - now.timestamp().rem_euclid(length)) as u64)
    };
    full(
        quota.quota_hour_window,
        quota.quota_hour_count,
        hour_limit,
        3600,
    )
    .or_else(|| {
        full(
            quota.quota_minute_window,
            quota.quota_minute_count,
            minute_limit,
            60,
        )
    })
}

/// The limits a project's events count against: the operator's
/// `MAX_EVENTS_PER_PROJECT_*`, tightened by the project's own where it has one.
fn project_limits(config: &RateLimitConfig, project: &Project) -> (i64, i64) {
    let tighten = |operator: i64, own: Option<i64>| own.map_or(operator, |own| own.min(operator));
    (
        tighten(
            config.max_events_per_project_per_minute,
            project.rate_limit_per_minute,
        ),
        tighten(
            config.max_events_per_project_per_hour,
            project.rate_limit_per_hour,
        ),
    )
}

/// Result when quota is exceeded
#[derive(Debug)]
pub struct QuotaExceeded {
    /// Seconds until the quota resets
    pub retry_after: u64,
    /// Which row was full: the installation or the project.
    pub scope: QuotaScope,
}

#[derive(Debug)]
pub enum QuotaScope {
    Installation,
    Project,
}

impl QuotaExceeded {
    /// The `X-Sentry-Rate-Limits` value Relay would send for this limit:
    /// `retry_after:categories:scope`, with no categories because the quota
    /// counts every event alike. The installation is Relay's organization.
    pub fn sentry_rate_limits_header(&self) -> String {
        let scope = match self.scope {
            QuotaScope::Installation => "organization",
            QuotaScope::Project => "project",
        };
        format!("{}::{scope}", self.retry_after)
    }
}

impl RateLimitService {
    /// Gets the installation singleton
    pub async fn get_installation(pool: &DbPool) -> AppResult<Installation> {
        let installation =
            sqlx::query_as::<_, Installation>("SELECT * FROM installation WHERE id = 1")
                .fetch_one(pool)
                .await?;
        Ok(installation)
    }

    /// Checks if quota is exceeded for installation or project (call during ingest)
    /// Returns Some(QuotaExceeded) if rate limited, None if allowed
    ///
    /// `project` is the row the caller has just loaded (the ingest extractor,
    /// or the digest's own lookup): it is read as the project's current quota
    /// state rather than fetched a second time.
    pub async fn check_quota(
        pool: &DbPool,
        project: &Project,
        config: &RateLimitConfig,
    ) -> AppResult<Option<QuotaExceeded>> {
        let now = Utc::now();

        let installation = Self::get_installation(pool).await?;
        let limits = (config.max_events_per_minute, config.max_events_per_hour);
        if let Some(retry_after) = seconds_while_full(installation.quota, limits, now) {
            return Ok(Some(QuotaExceeded {
                retry_after,
                scope: QuotaScope::Installation,
            }));
        }

        let limits = project_limits(config, project);
        if let Some(retry_after) = seconds_while_full(project.quota, limits, now) {
            return Ok(Some(QuotaExceeded {
                retry_after,
                scope: QuotaScope::Project,
            }));
        }

        Ok(None)
    }

    /// Counts one event against every quota window, or none of them.
    ///
    /// The check and the increment are one statement per row, so two digests
    /// can never both take the last slot: the same all-or-nothing contract as
    /// Relay's `is_rate_limited.lua`. Runs inside the digest transaction; on
    /// `Some`, the caller rolls back, which also undoes the installation count
    /// when only the project was full. The scope names the row that was full.
    pub async fn try_consume(
        executor: &mut <crate::db::Db as sqlx::Database>::Connection,
        project: &Project,
        config: &RateLimitConfig,
        now: chrono::DateTime<Utc>,
    ) -> AppResult<Option<QuotaScope>> {
        let minute = now.timestamp().div_euclid(60);
        let hour = now.timestamp().div_euclid(3600);

        let installation = sqlx::query(consume_sql!("installation", "id = 1", "$3", "$4"))
            .bind(minute)
            .bind(hour)
            .bind(config.max_events_per_minute)
            .bind(config.max_events_per_hour)
            .execute(&mut *executor)
            .await?;
        if installation.rows_affected() == 0 {
            return Ok(Some(QuotaScope::Installation));
        }

        // The project's own limits are read from the row inside the UPDATE, not
        // from `project`: a limit lowered after the digest loaded the row must
        // still hold. A NULL own limit compares false and leaves the
        // operator's.
        let consumed = sqlx::query(consume_sql!(
            "projects",
            "id = $5",
            "CASE WHEN rate_limit_per_minute < $3 THEN rate_limit_per_minute ELSE $3 END",
            "CASE WHEN rate_limit_per_hour < $4 THEN rate_limit_per_hour ELSE $4 END"
        ))
        .bind(minute)
        .bind(hour)
        .bind(config.max_events_per_project_per_minute)
        .bind(config.max_events_per_project_per_hour)
        .bind(project.id)
        .execute(&mut *executor)
        .await?;
        if consumed.rows_affected() == 0 {
            return Ok(Some(QuotaScope::Project));
        }

        Ok(None)
    }

    /// Counts one event the quota dropped against the project it was sent to,
    /// whichever row was full. Runs after the digest rolled back, on its own.
    pub async fn record_rate_limited(pool: &DbPool, project_id: i32) -> AppResult<()> {
        sqlx::query(
            "UPDATE projects SET rate_limited_event_count = rate_limited_event_count + 1 WHERE id = $1",
        )
        .bind(project_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Counts the stored event on the installation and its project, in the
    /// digest transaction that stores it.
    pub async fn increment_event_counters(
        executor: &mut <crate::db::Db as sqlx::Database>::Connection,
        project_id: i32,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE installation SET digested_event_count = digested_event_count + 1 WHERE id = 1",
        )
        .execute(&mut *executor)
        .await?;
        sqlx::query(
            "UPDATE projects SET stored_event_count = stored_event_count + 1, digested_event_count = digested_event_count + 1 WHERE id = $1",
        )
        .bind(project_id)
        .execute(&mut *executor)
        .await?;
        Ok(())
    }
}
