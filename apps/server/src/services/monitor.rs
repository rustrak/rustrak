//! Cron monitors: check-in processing, the missed/timeout sweep and reads.
//!
//! Mirrors Sentry's `monitors/consumers/monitor_consumer.py` (check-ins),
//! `monitors/logic/incidents.py` (thresholds and recovery) and
//! `monitors/clock_tasks/check_missed.py` / `check_timeout.py` (the sweep).
//! Sentry drives the sweep from a Kafka-timestamp clock to survive ingest
//! backlogs across partitions; one Rustrak process ingests synchronously, so
//! the wall clock is the clock.

use chrono::{DateTime, Duration, DurationRound, Utc};
use sqlx::types::Json;
use uuid::Uuid;

use crate::db::{begin_write, Db, DbPool};
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::monitor::{
    slugify_monitor_slug, validate_monitor_name, CheckIn, CheckInResponse, CheckInStatus,
    CreateMonitor, FailureReason, MonitorConfig, MonitorEnvironmentResponse,
    MonitorEnvironmentStatus, MonitorFailure, MonitorResponse, UpdateMonitor, MAX_MAX_RUNTIME,
};
use crate::services::hash_grouping_key;

type Tx<'a> = sqlx::Transaction<'a, Db>;

/// Check-ins shown per environment on a monitor's timeline.
const RECENT_CHECKINS: i64 = 48;
/// How long check-ins are kept (Sentry keeps them 90 days).
const CHECKIN_RETENTION_DAYS: i64 = 90;
/// Most check-ins deleted per sweep.
const RETENTION_BATCH: i64 = 5_000;
/// Longest duration stored, in ms (Sentry's `BoundedPositiveIntegerField`).
const MAX_DURATION_MS: i64 = i32::MAX as i64;

#[derive(sqlx::FromRow)]
struct MonitorRow {
    id: Uuid,
    project_id: i32,
    slug: String,
    name: String,
    config: Json<MonitorConfig>,
    is_muted: bool,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct EnvRow {
    id: Uuid,
    environment: String,
    status: String,
    next_checkin: Option<DateTime<Utc>>,
    next_checkin_latest: Option<DateTime<Utc>>,
    last_checkin: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct CheckInRow {
    id: Uuid,
    guid: Uuid,
    monitor_environment_id: Uuid,
    status: String,
    duration_ms: Option<i64>,
    date_added: DateTime<Utc>,
}

impl CheckInRow {
    fn status(&self) -> CheckInStatus {
        CheckInStatus::parse(&self.status)
    }
}

pub struct MonitorService;

impl MonitorService {
    /// Records one check-in. Returns the failures that must be reported as
    /// issue events (empty unless an incident opened or is ongoing).
    pub async fn process_check_in(
        pool: &DbPool,
        project_id: i32,
        check_in: CheckIn,
        received: DateTime<Utc>,
    ) -> AppResult<Vec<MonitorFailure>> {
        let mut tx = begin_write(pool).await?;
        let monitor = upsert_monitor(&mut tx, project_id, &check_in).await?;
        let env = ensure_environment(&mut tx, monitor.id, &check_in.environment).await?;

        let existing = if check_in.check_in_id.is_nil() {
            // A nil id means "the latest open check-in" (SDK spec 1.2.0).
            sqlx::query_as::<_, CheckInRow>(
                r#"
                SELECT id, guid, monitor_environment_id, status, duration_ms,
                       date_added
                FROM monitor_checkins
                WHERE monitor_environment_id = $1 AND status = 'in_progress'
                ORDER BY date_added DESC
                LIMIT 1
                "#,
            )
            .bind(env.id)
            .fetch_optional(&mut *tx)
            .await?
        } else {
            sqlx::query_as::<_, CheckInRow>(
                r#"
                SELECT id, guid, monitor_environment_id, status, duration_ms,
                       date_added
                FROM monitor_checkins
                WHERE monitor_id = $1 AND guid = $2
                "#,
            )
            .bind(monitor.id)
            .bind(check_in.check_in_id)
            .fetch_optional(&mut *tx)
            .await?
        };

        let config = &monitor.config.0;
        let duration_ms = check_in
            .duration
            .map(|secs| ((secs * 1000.0) as i64).min(MAX_DURATION_MS));

        let stored = match existing {
            Some(existing) => {
                if existing.monitor_environment_id != env.id {
                    return Err(AppError::Validation(
                        "check-in environment does not match the existing check-in".to_string(),
                    ));
                }
                match update_existing(&mut tx, existing, &check_in, duration_ms, config, received)
                    .await?
                {
                    Some(updated) => updated,
                    // A late `in_progress` for a finished run changes nothing.
                    None => {
                        tx.commit().await?;
                        return Ok(Vec::new());
                    }
                }
            }
            None => {
                let guid = if check_in.check_in_id.is_nil() {
                    Uuid::new_v4()
                } else {
                    check_in.check_in_id
                };
                let timeout_at = (check_in.status == CheckInStatus::InProgress)
                    .then(|| config.timeout_at(received));
                sqlx::query_as::<_, CheckInRow>(
                    r#"
                    INSERT INTO monitor_checkins (
                        id, guid, project_id, monitor_id, monitor_environment_id,
                        status, duration_ms, date_added, date_updated,
                        expected_time, timeout_at, trace_id
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, $9, $10, $11)
                    RETURNING id, guid, monitor_environment_id, status, duration_ms,
                              date_added
                    "#,
                )
                .bind(Uuid::new_v4())
                .bind(guid)
                .bind(project_id)
                .bind(monitor.id)
                .bind(env.id)
                .bind(check_in.status.as_str())
                .bind(duration_ms)
                .bind(received)
                .bind(env.next_checkin)
                .bind(timeout_at)
                .bind(check_in.trace_id.as_deref().map(|t| clamp(t, 64)))
                .fetch_one(&mut *tx)
                .await?
            }
        };

        let failures = if stored.status() == CheckInStatus::Error {
            if has_newer_result(&mut tx, env.id, stored.date_added).await? {
                Vec::new()
            } else {
                update_environment(
                    &mut tx,
                    &env,
                    config,
                    Some(stored.date_added),
                    received,
                    None,
                )
                .await?;
                let env = reload_environment(&mut tx, env.id).await?;
                mark_failed(&mut tx, &monitor, &env, &stored, FailureReason::Error).await?
            }
        } else {
            mark_ok(&mut tx, &monitor, &env, &stored, received).await?;
            Vec::new()
        };

        tx.commit().await?;
        Ok(failures)
    }

    /// Marks missed and timed-out check-ins as of `now`. Returns the
    /// failures to report. Idempotent: a second run at the same instant
    /// finds nothing.
    pub async fn sweep(pool: &DbPool, now: DateTime<Utc>) -> AppResult<Vec<MonitorFailure>> {
        let now = now.duration_trunc(Duration::minutes(1)).unwrap_or(now);
        let mut failures = Vec::new();

        let due: Vec<(Uuid, Uuid)> = sqlx::query_as(
            r#"
            SELECT e.id, e.monitor_id
            FROM monitor_environments e
            WHERE e.next_checkin_latest <= $1
            "#,
        )
        .bind(now)
        .fetch_all(pool)
        .await?;
        for (env_id, monitor_id) in due {
            let mut tx = begin_write(pool).await?;
            let env = reload_environment(&mut tx, env_id).await?;
            // Re-checked under the write lock: another sweep may have moved it.
            let (Some(expected), Some(latest)) = (env.next_checkin, env.next_checkin_latest) else {
                continue;
            };
            if latest > now {
                continue;
            }
            let monitor = load_monitor(&mut tx, monitor_id).await?;
            let config = &monitor.config.0;
            let missed = sqlx::query_as::<_, CheckInRow>(
                r#"
                INSERT INTO monitor_checkins (
                    id, guid, project_id, monitor_id, monitor_environment_id,
                    status, date_added, date_updated, expected_time
                ) VALUES ($1, $2, $3, $4, $5, 'missed', $6, $6, $6)
                RETURNING id, guid, monitor_environment_id, status, duration_ms,
                          date_added
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(Uuid::new_v4())
            .bind(monitor.project_id)
            .bind(monitor.id)
            .bind(env.id)
            .bind(expected)
            .fetch_one(&mut *tx)
            .await?;
            // Count from the most recent slot before now, not from the
            // missed one, so a long outage resumes on schedule instead of
            // replaying every slot it slept through.
            let most_recent = config.prev_checkin(expected, now);
            update_environment(&mut tx, &env, config, env.last_checkin, most_recent, None).await?;
            let env = reload_environment(&mut tx, env.id).await?;
            failures.extend(
                mark_failed(&mut tx, &monitor, &env, &missed, FailureReason::Missed).await?,
            );
            tx.commit().await?;
        }

        let open: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
            r#"
            SELECT id, monitor_id, monitor_environment_id
            FROM monitor_checkins
            WHERE status = 'in_progress' AND timeout_at <= $1
            "#,
        )
        .bind(now)
        .fetch_all(pool)
        .await?;
        for (checkin_id, monitor_id, env_id) in open {
            let mut tx = begin_write(pool).await?;
            let timed_out = sqlx::query_as::<_, CheckInRow>(
                r#"
                UPDATE monitor_checkins SET status = 'timeout'
                WHERE id = $1 AND status = 'in_progress'
                RETURNING id, guid, monitor_environment_id, status, duration_ms,
                          date_added
                "#,
            )
            .bind(checkin_id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(timed_out) = timed_out else {
                continue;
            };
            if !has_newer_result(&mut tx, env_id, timed_out.date_added).await? {
                let monitor = load_monitor(&mut tx, monitor_id).await?;
                let env = reload_environment(&mut tx, env_id).await?;
                failures.extend(
                    mark_failed(&mut tx, &monitor, &env, &timed_out, FailureReason::Timeout)
                        .await?,
                );
            }
            tx.commit().await?;
        }

        // Retention: a bounded batch per sweep keeps the write lock short and
        // still outpaces any schedule (one check-in a minute per monitor).
        sqlx::query(
            r#"
            DELETE FROM monitor_checkins WHERE id IN (
                SELECT id FROM monitor_checkins WHERE date_added < $1 LIMIT $2
            )
            "#,
        )
        .bind(now - Duration::days(CHECKIN_RETENTION_DAYS))
        .bind(RETENTION_BATCH)
        .execute(pool)
        .await?;

        Ok(failures)
    }

    pub async fn list(pool: &DbPool, project_id: i32) -> AppResult<Vec<MonitorResponse>> {
        let rows = sqlx::query_as::<_, MonitorRow>(
            r#"
            SELECT id, project_id, slug, name, config, is_muted, created_at
            FROM monitors WHERE project_id = $1
            ORDER BY name, slug
            "#,
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        let mut monitors = Vec::with_capacity(rows.len());
        for row in rows {
            monitors.push(to_response(pool, row).await?);
        }
        Ok(monitors)
    }

    pub async fn get_by_slug(
        pool: &DbPool,
        project_id: i32,
        slug: &str,
    ) -> AppResult<MonitorResponse> {
        let row = find_monitor(pool, project_id, slug).await?;
        to_response(pool, row).await
    }

    pub async fn set_muted(
        pool: &DbPool,
        project_id: i32,
        slug: &str,
        muted: bool,
    ) -> AppResult<()> {
        let done =
            sqlx::query("UPDATE monitors SET is_muted = $3 WHERE project_id = $1 AND slug = $2")
                .bind(project_id)
                .bind(slug)
                .bind(muted)
                .execute(pool)
                .await?;
        if done.rows_affected() == 0 {
            return Err(not_found(slug));
        }
        Ok(())
    }

    /// Check-ins of one monitor, newest first. Returns (rows, total).
    pub async fn list_checkins(
        pool: &DbPool,
        project_id: i32,
        slug: &str,
        environment: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<CheckInResponse>, i64)> {
        let monitor = find_monitor(pool, project_id, slug).await?;
        let page = page.max(1);
        let per_page = per_page.clamp(1, 100);

        let (total,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM monitor_checkins c
            JOIN monitor_environments e ON e.id = c.monitor_environment_id
            WHERE c.monitor_id = $1 AND ($2 IS NULL OR e.environment = $3)
            "#,
        )
        .bind(monitor.id)
        .bind(environment)
        .bind(environment)
        .fetch_one(pool)
        .await?;

        let rows = sqlx::query_as::<_, CheckInWithEnv>(
            r#"
            SELECT c.guid, e.environment, c.status, c.duration_ms, c.date_added,
                   c.date_updated, c.expected_time
            FROM monitor_checkins c
            JOIN monitor_environments e ON e.id = c.monitor_environment_id
            WHERE c.monitor_id = $1 AND ($2 IS NULL OR e.environment = $3)
            ORDER BY c.date_added DESC, c.id DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(monitor.id)
        .bind(environment)
        .bind(environment)
        .bind(per_page)
        .bind((page - 1) * per_page)
        .fetch_all(pool)
        .await?;

        Ok((
            rows.into_iter()
                .map(CheckInWithEnv::into_response)
                .collect(),
            total,
        ))
    }
}

#[derive(sqlx::FromRow)]
struct CheckInWithEnv {
    guid: Uuid,
    environment: String,
    status: String,
    duration_ms: Option<i64>,
    date_added: DateTime<Utc>,
    date_updated: DateTime<Utc>,
    expected_time: Option<DateTime<Utc>>,
}

impl CheckInWithEnv {
    fn into_response(self) -> CheckInResponse {
        CheckInResponse {
            id: self.guid,
            environment: self.environment,
            status: CheckInStatus::parse(&self.status),
            duration_ms: self.duration_ms,
            date_added: self.date_added,
            date_updated: self.date_updated,
            expected_time: self.expected_time,
        }
    }
}

fn not_found(slug: &str) -> AppError {
    AppError::NotFound(format!("Monitor {slug} not found"))
}

fn clamp(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

async fn find_monitor(pool: &DbPool, project_id: i32, slug: &str) -> AppResult<MonitorRow> {
    sqlx::query_as::<_, MonitorRow>(
        r#"
        SELECT id, project_id, slug, name, config, is_muted, created_at
        FROM monitors WHERE project_id = $1 AND slug = $2
        "#,
    )
    .bind(project_id)
    .bind(slug)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| not_found(slug))
}

async fn load_monitor(tx: &mut Tx<'_>, id: Uuid) -> AppResult<MonitorRow> {
    Ok(sqlx::query_as::<_, MonitorRow>(
        r#"
        SELECT id, project_id, slug, name, config, is_muted, created_at
        FROM monitors WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await?)
}

/// Finds the check-in's monitor, creating or reconfiguring it from
/// `monitor_config` (Sentry's `_ensure_monitor_with_config`). An invalid
/// config is ignored for an existing monitor and rejects the check-in for a
/// new one.
async fn upsert_monitor(
    tx: &mut Tx<'_>,
    project_id: i32,
    check_in: &CheckIn,
) -> AppResult<MonitorRow> {
    let existing = sqlx::query_as::<_, MonitorRow>(
        r#"
        SELECT id, project_id, slug, name, config, is_muted, created_at
        FROM monitors WHERE project_id = $1 AND slug = $2
        "#,
    )
    .bind(project_id)
    .bind(&check_in.monitor_slug)
    .fetch_optional(&mut **tx)
    .await?;

    match (existing, check_in.monitor_config.clone()) {
        (Some(mut monitor), Some(input)) => {
            match input.validate(Some(&monitor.config.0)) {
                Ok(config) if config != monitor.config.0 => {
                    sqlx::query("UPDATE monitors SET config = $2 WHERE id = $1")
                        .bind(monitor.id)
                        .bind(Json(&config))
                        .execute(&mut **tx)
                        .await?;
                    monitor.config = Json(config);
                }
                Ok(_) => {}
                Err(e) => log::warn!(
                    "ignoring invalid monitor_config for {}: {e}",
                    check_in.monitor_slug
                ),
            }
            Ok(monitor)
        }
        (Some(monitor), None) => Ok(monitor),
        (None, Some(input)) => {
            let config = input.validate(None)?;
            Ok(sqlx::query_as::<_, MonitorRow>(
                r#"
                INSERT INTO monitors (id, project_id, slug, name, config, is_muted, created_at)
                VALUES ($1, $2, $3, $3, $4, FALSE, $5)
                RETURNING id, project_id, slug, name, config, is_muted, created_at
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(project_id)
            .bind(&check_in.monitor_slug)
            .bind(Json(&config))
            .bind(Utc::now())
            .fetch_one(&mut **tx)
            .await?)
        }
        (None, None) => Err(AppError::NotFound(format!(
            "monitor {} (a check-in without monitor_config cannot create it)",
            check_in.monitor_slug
        ))),
    }
}

async fn ensure_environment(
    tx: &mut Tx<'_>,
    monitor_id: Uuid,
    environment: &str,
) -> AppResult<EnvRow> {
    let existing = sqlx::query_as::<_, EnvRow>(
        r#"
        SELECT id, environment, status, next_checkin, next_checkin_latest, last_checkin
        FROM monitor_environments WHERE monitor_id = $1 AND environment = $2
        "#,
    )
    .bind(monitor_id)
    .bind(environment)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(env) = existing {
        return Ok(env);
    }
    Ok(sqlx::query_as::<_, EnvRow>(
        r#"
        INSERT INTO monitor_environments (id, monitor_id, environment, status, created_at)
        VALUES ($1, $2, $3, 'active', $4)
        RETURNING id, environment, status, next_checkin, next_checkin_latest, last_checkin
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(monitor_id)
    .bind(environment)
    .bind(Utc::now())
    .fetch_one(&mut **tx)
    .await?)
}

async fn reload_environment(tx: &mut Tx<'_>, id: Uuid) -> AppResult<EnvRow> {
    Ok(sqlx::query_as::<_, EnvRow>(
        r#"
        SELECT id, environment, status, next_checkin, next_checkin_latest, last_checkin
        FROM monitor_environments WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await?)
}

/// Applies a check-in to an existing one (Sentry's
/// `update_existing_check_in`). `None` means nothing changed.
async fn update_existing(
    tx: &mut Tx<'_>,
    existing: CheckInRow,
    check_in: &CheckIn,
    duration_ms: Option<i64>,
    config: &MonitorConfig,
    received: DateTime<Utc>,
) -> AppResult<Option<CheckInRow>> {
    let current = existing.status();
    let incoming = check_in.status;
    let elapsed_ms = || {
        duration_ms.or_else(|| {
            Some(((received - existing.date_added).num_milliseconds().abs()).min(MAX_DURATION_MS))
        })
    };

    let (status, duration, timeout_at) = match current {
        CheckInStatus::Ok | CheckInStatus::Error => {
            if incoming == CheckInStatus::InProgress {
                return Ok(None);
            }
            return Err(AppError::Validation(
                "check-in has already completed".to_string(),
            ));
        }
        // A synthetic failure stays a failure; a late result only records
        // how long the run took.
        CheckInStatus::Timeout | CheckInStatus::Missed => {
            if incoming == CheckInStatus::InProgress {
                return Ok(None);
            }
            (current, elapsed_ms(), None)
        }
        CheckInStatus::InProgress | CheckInStatus::Unknown => {
            if incoming == CheckInStatus::InProgress {
                // A heartbeat: the run is alive, push its timeout out, never
                // past the hard ceiling.
                let ceiling = config.timeout_at(existing.date_added)
                    - Duration::minutes(config.max_runtime as i64)
                    + Duration::minutes(MAX_MAX_RUNTIME as i64);
                (
                    current,
                    existing.duration_ms,
                    Some(config.timeout_at(received).min(ceiling)),
                )
            } else {
                (incoming, elapsed_ms(), None)
            }
        }
    };

    Ok(Some(
        sqlx::query_as::<_, CheckInRow>(
            r#"
            UPDATE monitor_checkins
            SET status = $2, duration_ms = $3, date_updated = $4,
                timeout_at = COALESCE($5, timeout_at)
            WHERE id = $1
            RETURNING id, guid, monitor_environment_id, status, duration_ms,
                      date_added
            "#,
        )
        .bind(existing.id)
        .bind(status.as_str())
        .bind(duration)
        .bind(received)
        .bind(timeout_at)
        .fetch_one(&mut **tx)
        .await?,
    ))
}

/// Whether an ok/error check-in newer than `date_added` exists; an older
/// failure must not override it.
async fn has_newer_result(
    tx: &mut Tx<'_>,
    env_id: Uuid,
    date_added: DateTime<Utc>,
) -> AppResult<bool> {
    let newer: Option<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM monitor_checkins
        WHERE monitor_environment_id = $1 AND date_added > $2 AND status IN ('ok', 'error')
        LIMIT 1
        "#,
    )
    .bind(env_id)
    .bind(date_added)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(newer.is_some())
}

/// Moves the environment's expected check-in forward from `reference`
/// (Sentry's `update_monitor_environment`).
async fn update_environment(
    tx: &mut Tx<'_>,
    env: &EnvRow,
    config: &MonitorConfig,
    last_checkin: Option<DateTime<Utc>>,
    reference: DateTime<Utc>,
    status: Option<MonitorEnvironmentStatus>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE monitor_environments
        SET last_checkin = $2, next_checkin = $3, next_checkin_latest = $4,
            status = COALESCE($5, status)
        WHERE id = $1
        "#,
    )
    .bind(env.id)
    .bind(last_checkin)
    .bind(config.next_checkin(reference))
    .bind(config.next_checkin_latest(reference))
    .bind(status.map(|s| s.as_str()))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Sentry's `mark_ok`: tries to resolve an open incident, then moves the
/// schedule forward.
async fn mark_ok(
    tx: &mut Tx<'_>,
    monitor: &MonitorRow,
    env: &EnvRow,
    checkin: &CheckInRow,
    succeeded_at: DateTime<Utc>,
) -> AppResult<()> {
    let resolved = try_resolution(tx, monitor, env, checkin).await?;
    if env.last_checkin.is_none_or(|last| last <= succeeded_at) {
        update_environment(
            tx,
            env,
            &monitor.config.0,
            Some(checkin.date_added),
            succeeded_at,
            resolved.then_some(MonitorEnvironmentStatus::Ok),
        )
        .await?;
    }
    Ok(())
}

/// Sentry's `try_incident_resolution`: `recovery_threshold` consecutive ok
/// check-ins close the incident and resolve its issue.
async fn try_resolution(
    tx: &mut Tx<'_>,
    monitor: &MonitorRow,
    env: &EnvRow,
    checkin: &CheckInRow,
) -> AppResult<bool> {
    if MonitorEnvironmentStatus::parse(&env.status) == MonitorEnvironmentStatus::Ok
        || checkin.status() != CheckInStatus::Ok
    {
        return Ok(false);
    }
    let threshold = monitor.config.0.recovery_threshold.max(1) as i64;
    if threshold > 1 {
        let recent = recent_statuses(tx, env.id, None, threshold).await?;
        if recent.iter().any(|s| *s != CheckInStatus::Ok) {
            return Ok(false);
        }
    }
    let key_hash = hash_grouping_key(&MonitorFailure::fingerprint(env.id));
    sqlx::query(
        r#"
        UPDATE issues SET status = 'resolved', substatus = NULL, status_details = '{}'
        WHERE status = 'unresolved' AND id IN (
            SELECT issue_id FROM groupings WHERE project_id = $1 AND grouping_key_hash = $2
        )
        "#,
    )
    .bind(monitor.project_id)
    .bind(key_hash)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

/// Sentry's `try_incident_threshold`: opens an incident once
/// `failure_issue_threshold` consecutive check-ins failed, and reports every
/// failure while it stays open. A muted monitor reports nothing.
async fn mark_failed(
    tx: &mut Tx<'_>,
    monitor: &MonitorRow,
    env: &EnvRow,
    failed: &CheckInRow,
    reason: FailureReason,
) -> AppResult<Vec<MonitorFailure>> {
    match MonitorEnvironmentStatus::parse(&env.status) {
        MonitorEnvironmentStatus::Error => {}
        MonitorEnvironmentStatus::Ok | MonitorEnvironmentStatus::Active => {
            let threshold = monitor.config.0.failure_issue_threshold.max(1) as i64;
            if threshold > 1 {
                let recent =
                    recent_statuses(tx, env.id, Some(failed.date_added), threshold).await?;
                if (recent.len() as i64) < threshold || recent.contains(&CheckInStatus::Ok) {
                    return Ok(Vec::new());
                }
            }
            sqlx::query("UPDATE monitor_environments SET status = 'error' WHERE id = $1")
                .bind(env.id)
                .execute(&mut **tx)
                .await?;
        }
    }

    if monitor.is_muted {
        return Ok(Vec::new());
    }
    Ok(vec![MonitorFailure {
        project_id: monitor.project_id,
        monitor_slug: monitor.slug.clone(),
        monitor_name: monitor.name.clone(),
        environment: env.environment.clone(),
        monitor_environment_id: env.id,
        check_in_id: failed.guid,
        reason,
        occurred_at: failed.date_added,
    }])
}

/// Statuses of the `limit` newest check-ins of an environment, optionally
/// only those at or before `until`.
async fn recent_statuses(
    tx: &mut Tx<'_>,
    env_id: Uuid,
    until: Option<DateTime<Utc>>,
    limit: i64,
) -> AppResult<Vec<CheckInStatus>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT status FROM monitor_checkins
        WHERE monitor_environment_id = $1 AND ($2 IS NULL OR date_added <= $3)
        ORDER BY date_added DESC
        LIMIT $4
        "#,
    )
    .bind(env_id)
    .bind(until)
    .bind(until)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows.iter().map(|(s,)| CheckInStatus::parse(s)).collect())
}

async fn to_response(pool: &DbPool, row: MonitorRow) -> AppResult<MonitorResponse> {
    let envs = sqlx::query_as::<_, EnvRow>(
        r#"
        SELECT id, environment, status, next_checkin, next_checkin_latest, last_checkin
        FROM monitor_environments WHERE monitor_id = $1
        ORDER BY environment
        "#,
    )
    .bind(row.id)
    .fetch_all(pool)
    .await?;

    let mut environments = Vec::with_capacity(envs.len());
    for env in envs {
        let recent = sqlx::query_as::<_, CheckInWithEnv>(
            r#"
            SELECT c.guid, e.environment, c.status, c.duration_ms, c.date_added,
                   c.date_updated, c.expected_time
            FROM monitor_checkins c
            JOIN monitor_environments e ON e.id = c.monitor_environment_id
            WHERE c.monitor_environment_id = $1
            ORDER BY c.date_added DESC, c.id DESC
            LIMIT $2
            "#,
        )
        .bind(env.id)
        .bind(RECENT_CHECKINS)
        .fetch_all(pool)
        .await?;
        environments.push(MonitorEnvironmentResponse {
            id: env.id,
            status: MonitorEnvironmentStatus::parse(&env.status),
            environment: env.environment,
            next_checkin: env.next_checkin,
            next_checkin_latest: env.next_checkin_latest,
            last_checkin: env.last_checkin,
            recent_checkins: recent
                .into_iter()
                .map(CheckInWithEnv::into_response)
                .collect(),
        });
    }

    Ok(MonitorResponse {
        id: row.id,
        slug: row.slug,
        name: row.name,
        config: row.config.0,
        is_muted: row.is_muted,
        created_at: row.created_at,
        environments,
    })
}

impl MonitorService {
    /// Creates a monitor from the dashboard. It has no environment until its
    /// first check-in.
    pub async fn create(
        pool: &DbPool,
        project_id: i32,
        input: CreateMonitor,
    ) -> AppResult<MonitorResponse> {
        let name = validate_monitor_name(&input.name)?;
        let slug = slugify_monitor_slug(input.slug.as_deref().unwrap_or(&name));
        if slug.is_empty() {
            return Err(AppError::Validation("Monitor slug is empty".to_string())
                .with_field("slug", FieldErrorCode::Invalid));
        }
        let config = input
            .config
            .validate(None)
            .map_err(|e| e.with_field("config", FieldErrorCode::Invalid))?;

        let taken: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM monitors WHERE project_id = $1 AND slug = $2")
                .bind(project_id)
                .bind(&slug)
                .fetch_optional(pool)
                .await?;
        if taken.is_some() {
            return Err(
                AppError::Conflict(format!("A monitor with slug {slug} already exists"))
                    .with_field("slug", FieldErrorCode::AlreadyExists),
            );
        }

        let row = sqlx::query_as::<_, MonitorRow>(
            r#"
            INSERT INTO monitors (id, project_id, slug, name, config, is_muted, created_at)
            VALUES ($1, $2, $3, $4, $5, FALSE, $6)
            RETURNING id, project_id, slug, name, config, is_muted, created_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(project_id)
        .bind(&slug)
        .bind(&name)
        .bind(Json(&config))
        .bind(Utc::now())
        .fetch_one(pool)
        .await?;
        to_response(pool, row).await
    }

    /// Renames, reconfigures or mutes a monitor. A new schedule applies from
    /// the next check-in.
    pub async fn update(
        pool: &DbPool,
        project_id: i32,
        slug: &str,
        input: UpdateMonitor,
    ) -> AppResult<MonitorResponse> {
        let current = find_monitor(pool, project_id, slug).await?;
        let name = match input.name {
            Some(name) => validate_monitor_name(&name)?,
            None => current.name.clone(),
        };
        let config = match input.config {
            Some(config) => config
                .validate(Some(&current.config.0))
                .map_err(|e| e.with_field("config", FieldErrorCode::Invalid))?,
            None => current.config.0.clone(),
        };
        let is_muted = input.is_muted.unwrap_or(current.is_muted);

        let row = sqlx::query_as::<_, MonitorRow>(
            r#"
            UPDATE monitors SET name = $2, config = $3, is_muted = $4
            WHERE id = $1
            RETURNING id, project_id, slug, name, config, is_muted, created_at
            "#,
        )
        .bind(current.id)
        .bind(&name)
        .bind(Json(&config))
        .bind(is_muted)
        .fetch_one(pool)
        .await?;
        to_response(pool, row).await
    }

    /// Deletes a monitor with its environments and check-ins. Its issues stay.
    pub async fn delete(pool: &DbPool, project_id: i32, slug: &str) -> AppResult<()> {
        let done = sqlx::query("DELETE FROM monitors WHERE project_id = $1 AND slug = $2")
            .bind(project_id)
            .bind(slug)
            .execute(pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(not_found(slug));
        }
        Ok(())
    }
}
