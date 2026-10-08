//! Storage stats & retention service.
//!
//! Surfaces how much data Rustrak is holding — row counts and on-disk weight,
//! globally and per project — so an admin can see what's accumulating and reclaim
//! space. Backs the Settings → Storage page.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use chrono::Utc;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{
    CleanupCounts, CleanupFilter, CleanupState, CleanupStatus, ProjectStorage, SourceMapGcResult,
    SourceMapStorage, StorageSummary,
};
use crate::services::sourcemap_store::SourceMapStore;

pub struct StorageService;

/// Rows a cleanup deletes per transaction.
///
/// Measured purging 750k of 1M ~5KB events (plus transactions, spans and logs)
/// with ingestion running alongside. Every commit rewrites the pages its rows
/// touch in the random-keyed indexes (`id`, `event_id`), so fewer, larger
/// batches are cheaper per row: on SQLite 2,000 rows took 17 min and 10,000
/// took 6.5, with no batch holding the write lock over ~1.6s (the busy timeout
/// is 5s); on Postgres 2,000 took 3 min and 10,000 took 2. Ingestion saw no
/// errors at either size.
///
/// ponytail: one DELETE over everything was faster still (64s on SQLite, 28s
/// on Postgres) but held the database for the whole purge: ingestion on
/// Postgres answered 500s while it ran. Batches trade that speed for never
/// blocking. Revisit if cleanups of tens of millions of rows become routine.
const CLEANUP_BATCH_SIZE: i64 = 10_000;

/// Rows per project and table the storage page's size estimate reads.
const SIZE_SAMPLE_ROWS: i64 = 200;

/// The size samples: `(payload bytes, rows)` over a project's newest rows,
/// `$1` the project and `$2` the sample size. Each walks the table's
/// `(project_id, ingested_at)` index; spans have no `ingested_at` and take
/// whichever rows their `project_id` index yields first.
/// `length(CAST(.. AS TEXT))` is the character length, the same measure on
/// both backends.
const EVENTS_SAMPLE: &str = "SELECT COALESCE(SUM(length(CAST(data AS TEXT))), 0), COUNT(*) \
     FROM (SELECT data FROM events WHERE project_id = $1 ORDER BY ingested_at DESC LIMIT $2) AS sample";
const TRANSACTIONS_SAMPLE: &str = "SELECT COALESCE(SUM(length(CAST(data AS TEXT))), 0), COUNT(*) \
     FROM (SELECT data FROM transactions WHERE project_id = $1 ORDER BY ingested_at DESC LIMIT $2) AS sample";
const SPANS_SAMPLE: &str = "SELECT COALESCE(SUM(length(CAST(data AS TEXT))), 0), COUNT(*) \
     FROM (SELECT data FROM spans WHERE project_id = $1 LIMIT $2) AS sample";
const LOGS_SAMPLE: &str = "SELECT COALESCE(SUM(COALESCE(length(CAST(body AS TEXT)), 0) \
     + COALESCE(length(CAST(attributes AS TEXT)), 0)), 0), COUNT(*) \
     FROM (SELECT body, attributes FROM logs WHERE project_id = $1 ORDER BY ingested_at DESC LIMIT $2) AS sample";

/// Gives waiting writers a turn between cleanup batches. SQLite's busy
/// handler retries a blocked writer at intervals of up to 100ms, so a pause
/// at least that long guarantees ingestion gets the lock between batches
/// instead of starving behind the cleanup. Postgres writers do not wait on
/// the cleanup, so it needs none.
async fn pause_between_batches() {
    #[cfg(feature = "sqlite")]
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

/// The instance's cleanup job: at most one runs at a time, detached from the
/// request that started it.
///
/// Running it inside the request was the other half of the timeout problem:
/// when the client gave up, Actix dropped the handler and the purge with it.
#[derive(Default)]
pub struct CleanupJob {
    status: Arc<Mutex<CleanupStatus>>,
}

/// The status is plain data, so a panic while it was locked leaves nothing
/// half-written worth refusing to read.
fn lock(status: &Mutex<CleanupStatus>) -> MutexGuard<'_, CleanupStatus> {
    status.lock().unwrap_or_else(PoisonError::into_inner)
}

impl CleanupJob {
    pub fn status(&self) -> CleanupStatus {
        lock(&self.status).clone()
    }

    /// Validates the window and starts the cleanup in the background,
    /// returning the `running` status. Refuses with `Conflict` while another
    /// cleanup is running.
    pub fn start(
        &self,
        pool: DbPool,
        older_than_days: i64,
        project_id: Option<i32>,
        filter: CleanupFilter,
    ) -> AppResult<CleanupStatus> {
        validate_retention(older_than_days)?;

        let started = {
            let mut status = lock(&self.status);
            if status.state == CleanupState::Running {
                return Err(AppError::Conflict(
                    "A storage cleanup is already running".to_string(),
                ));
            }
            *status = CleanupStatus {
                state: CleanupState::Running,
                started_at: Some(Utc::now()),
                ..Default::default()
            };
            status.clone()
        };

        let shared = Arc::clone(&self.status);
        tokio::spawn(async move {
            let progress = Arc::clone(&shared);
            // Its own task so a panic inside surfaces here as a JoinError
            // instead of leaving the job `running` forever.
            let outcome = tokio::spawn(async move {
                StorageService::execute_cleanup_in_batches(
                    &pool,
                    older_than_days,
                    project_id,
                    filter,
                    CLEANUP_BATCH_SIZE,
                    |counts| lock(&progress).removed = counts.clone(),
                )
                .await
            })
            .await;

            let mut status = lock(&shared);
            status.finished_at = Some(Utc::now());
            match outcome {
                Ok(Ok(counts)) => {
                    status.state = CleanupState::Completed;
                    status.removed = counts;
                }
                Ok(Err(error)) => {
                    log::error!("Storage cleanup failed: {error}");
                    status.state = CleanupState::Failed;
                    status.error = Some(error.to_string());
                }
                Err(error) => {
                    log::error!("Storage cleanup task stopped: {error}");
                    status.state = CleanupState::Failed;
                    status.error = Some("The cleanup stopped unexpectedly".to_string());
                }
            }
        });

        Ok(started)
    }
}

/// Smallest accepted retention window. A value below this would push the cutoff
/// to "now" or into the future, turning the cleanup into a full data wipe — so
/// it's rejected before any cutoff is computed. Mirrors the `min(1)` the MCP and
/// UI already enforce on the client side.
const MIN_RETENTION_DAYS: i64 = 1;

/// Rejects retention windows that would delete current/future data. Shared by the
/// preview and execute paths so both fail fast on the same contract.
fn validate_retention(older_than_days: i64) -> AppResult<()> {
    if older_than_days < MIN_RETENTION_DAYS {
        return Err(AppError::Validation(format!(
            "older_than_days must be at least {MIN_RETENTION_DAYS}"
        )));
    }
    Ok(())
}

impl StorageService {
    /// Dry-run for [`Self::gc_source_maps`]: counts the orphaned `source_file`
    /// rows and the bytes a GC would reclaim. Mutates nothing.
    pub async fn preview_source_map_gc(pool: &DbPool) -> AppResult<SourceMapGcResult> {
        // `size` is INT, so SUM(size) is BIGINT on Postgres (and integer on SQLite)
        // — both decode to i64. Casting size to BIGINT *before* SUM would make
        // Postgres return NUMERIC, which sqlx cannot decode into i64.
        let (files_removed, bytes_freed): (i64, i64) = sqlx::query_as(
            r#"
            SELECT
                COUNT(*),
                COALESCE(SUM(sf.size), 0)
            FROM source_file sf
            WHERE NOT EXISTS (
                SELECT 1 FROM source_file_metadata m WHERE m.file_id = sf.id
            )
            "#,
        )
        .fetch_one(pool)
        .await?;

        Ok(SourceMapGcResult {
            files_removed,
            bytes_freed,
        })
    }

    /// Garbage-collects orphaned source maps: `source_file` rows that no
    /// `source_file_metadata` references. Removes the DB row and unlinks the file
    /// from the CAS. Never touches referenced files.
    pub async fn gc_source_maps(
        pool: &DbPool,
        store: &dyn SourceMapStore,
    ) -> AppResult<SourceMapGcResult> {
        // Orphans: source_file rows with no metadata pointing at them. This is a
        // point-in-time snapshot — uploads dedup on `checksum`, so a concurrent
        // upload can re-reference one of these rows before we reach it below.
        let orphans: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT sf.checksum
            FROM source_file sf
            WHERE NOT EXISTS (
                SELECT 1 FROM source_file_metadata m WHERE m.file_id = sf.id
            )
            "#,
        )
        .fetch_all(pool)
        .await?;

        let mut files_removed = 0_i64;
        let mut bytes_freed = 0_i64;

        for (checksum,) in orphans {
            // Re-check orphan status atomically at delete time: the row is only
            // removed if it's STILL unreferenced, closing the window where a
            // concurrent upload re-attached metadata to it. `RETURNING` hands back
            // the path/size of the row we actually deleted (None if it's no longer
            // an orphan), so we never unlink a file that's back in use.
            let deleted: Option<(String, i64)> = sqlx::query_as(
                r#"
                DELETE FROM source_file
                WHERE checksum = $1
                  AND NOT EXISTS (
                      SELECT 1 FROM source_file_metadata m WHERE m.file_id = source_file.id
                  )
                RETURNING storage_path, CAST(size AS BIGINT)
                "#,
            )
            .bind(&checksum)
            .fetch_optional(pool)
            .await?;

            if let Some((storage_path, size)) = deleted {
                // The DB row is gone; now unlink the file. The store's delete is
                // idempotent (a missing file is not an error), so a crash here just
                // leaves a stray file that a rerun re-reaps.
                let _ = store.delete(&storage_path).await;
                files_removed += 1;
                bytes_freed += size;
            }
        }

        Ok(SourceMapGcResult {
            files_removed,
            bytes_freed,
        })
    }

    /// Dry-run: counts the rows a cleanup of data older than `older_than_days`
    /// would remove (optionally scoped to one project). Mutates nothing.
    pub async fn preview_cleanup(
        pool: &DbPool,
        older_than_days: i64,
        project_id: Option<i32>,
        filter: CleanupFilter,
    ) -> AppResult<CleanupCounts> {
        validate_retention(older_than_days)?;
        let cutoff = Utc::now() - chrono::Duration::days(older_than_days);
        Self::count_cleanup(pool, cutoff, project_id, filter).await
    }

    /// Executes a cleanup: deletes data older than `older_than_days` (optionally
    /// scoped to one project) and removes the issues it leaves with zero events.
    /// Returns the same shape as the preview. See
    /// [`Self::execute_cleanup_in_batches`] for how the work is split.
    pub async fn execute_cleanup(
        pool: &DbPool,
        older_than_days: i64,
        project_id: Option<i32>,
        filter: CleanupFilter,
    ) -> AppResult<CleanupCounts> {
        Self::execute_cleanup_in_batches(
            pool,
            older_than_days,
            project_id,
            filter,
            CLEANUP_BATCH_SIZE,
            |_| {},
        )
        .await
    }

    /// The cleanup, deleting at most `batch_size` rows per transaction and
    /// calling `on_batch` with the running totals after each one.
    ///
    /// One transaction over the whole purge was what made it unusable on a
    /// large instance: it outlived every HTTP timeout and rolled back, and on
    /// SQLite it held the write lock long enough to stall ingestion. Short
    /// batches commit as they go, so an interrupted run keeps what it deleted
    /// and the next run picks up the rest.
    ///
    /// Batches walk one project at a time so every lookup is a range scan of
    /// a `(project_id, ingested_at)` index.
    pub async fn execute_cleanup_in_batches(
        pool: &DbPool,
        older_than_days: i64,
        project_id: Option<i32>,
        filter: CleanupFilter,
        batch_size: i64,
        mut on_batch: impl FnMut(&CleanupCounts),
    ) -> AppResult<CleanupCounts> {
        validate_retention(older_than_days)?;
        let cutoff = Utc::now() - chrono::Duration::days(older_than_days);
        let projects: Vec<i32> =
            sqlx::query_scalar("SELECT id FROM projects WHERE ($1 IS NULL OR id = $2) ORDER BY id")
                .bind(project_id)
                .bind(project_id)
                .fetch_all(pool)
                .await?;

        let mut counts = CleanupCounts::default();
        Self::purge(
            pool,
            cutoff,
            &projects,
            filter,
            batch_size,
            &mut counts,
            &mut on_batch,
        )
        .await?;
        Ok(counts)
    }

    async fn purge(
        pool: &DbPool,
        cutoff: chrono::DateTime<Utc>,
        projects: &[i32],
        filter: CleanupFilter,
        batch_size: i64,
        counts: &mut CleanupCounts,
        on_batch: &mut impl FnMut(&CleanupCounts),
    ) -> AppResult<()> {
        for &project_id in projects {
            if filter.include_transactions {
                while let Some((transactions, spans)) =
                    Self::delete_transaction_batch(pool, project_id, cutoff, batch_size).await?
                {
                    counts.transactions += transactions;
                    counts.spans += spans;
                    on_batch(counts);
                    pause_between_batches().await;
                }
            }

            if filter.include_logs {
                loop {
                    // Logs carry no issue_id and drive no counter: one statement.
                    let logs = sqlx::query(
                        "DELETE FROM logs WHERE id IN ( \
                             SELECT id FROM logs \
                             WHERE project_id = $1 AND ingested_at < $2 LIMIT $3)",
                    )
                    .bind(project_id)
                    .bind(cutoff)
                    .bind(batch_size)
                    .execute(pool)
                    .await?
                    .rows_affected() as i64;
                    if logs == 0 {
                        break;
                    }
                    counts.logs += logs;
                    on_batch(counts);
                    pause_between_batches().await;
                }
            }

            // Events (and the issue bookkeeping they drive) only when in scope:
            // a transaction- or log-only purge leaves error history, its issues
            // and the denormalized counters untouched.
            if filter.include_events {
                while let Some((events, issues_removed)) =
                    Self::delete_event_batch(pool, project_id, cutoff, batch_size).await?
                {
                    counts.events += events;
                    counts.issues_removed += issues_removed;
                    on_batch(counts);
                    pause_between_batches().await;
                }
            }
        }
        Ok(())
    }

    /// Deletes up to `limit` old transactions of one project with their spans.
    /// Returns `(transactions, spans)` removed, or `None` once none are left.
    ///
    /// The batch is pinned in a temp table so the span delete and the
    /// transaction delete act on exactly the same rows. Spans are deleted
    /// explicitly rather than left to the cascade so they can be counted.
    async fn delete_transaction_batch(
        pool: &DbPool,
        project_id: i32,
        cutoff: chrono::DateTime<Utc>,
        limit: i64,
    ) -> AppResult<Option<(i64, i64)>> {
        // Read-then-write: IMMEDIATE on SQLite (see db::begin_write).
        let mut tx = crate::db::begin_write(pool).await?;
        // `WHERE 1 = 0` copies the column types, which differ per backend.
        sqlx::query("CREATE TEMP TABLE cleanup_batch AS SELECT id FROM transactions WHERE 1 = 0")
            .execute(&mut *tx)
            .await?;
        let picked = sqlx::query(
            "INSERT INTO cleanup_batch \
             SELECT id FROM transactions WHERE project_id = $1 AND ingested_at < $2 LIMIT $3",
        )
        .bind(project_id)
        .bind(cutoff)
        .bind(limit)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if picked == 0 {
            // Rolling back also drops the temp table.
            tx.rollback().await?;
            return Ok(None);
        }

        let spans =
            sqlx::query("DELETE FROM spans WHERE transaction_id IN (SELECT id FROM cleanup_batch)")
                .execute(&mut *tx)
                .await?
                .rows_affected() as i64;
        let transactions =
            sqlx::query("DELETE FROM transactions WHERE id IN (SELECT id FROM cleanup_batch)")
                .execute(&mut *tx)
                .await?
                .rows_affected() as i64;
        sqlx::query("DROP TABLE cleanup_batch")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some((transactions, spans)))
    }

    /// Deletes up to `limit` old events of one project, keeps their issues'
    /// counters in step, and removes the issues this batch left empty.
    /// Returns `(events, issues_removed)`, or `None` once none are left.
    ///
    /// `None` is decided by what the batch picked, not by what it deleted: on
    /// Postgres a user deleting an issue mid-batch takes the picked events
    /// with it, and a batch that deleted nothing is not the last one.
    ///
    /// Only issues this batch touched can be removed: an issue that was
    /// already empty is not the cleanup's to delete.
    async fn delete_event_batch(
        pool: &DbPool,
        project_id: i32,
        cutoff: chrono::DateTime<Utc>,
        limit: i64,
    ) -> AppResult<Option<(i64, i64)>> {
        // Read-then-write: IMMEDIATE on SQLite (see db::begin_write).
        let mut tx = crate::db::begin_write(pool).await?;
        sqlx::query(
            "CREATE TEMP TABLE cleanup_batch AS SELECT id, issue_id FROM events WHERE 1 = 0",
        )
        .execute(&mut *tx)
        .await?;
        let picked = sqlx::query(
            "INSERT INTO cleanup_batch \
             SELECT id, issue_id FROM events WHERE project_id = $1 AND ingested_at < $2 LIMIT $3",
        )
        .bind(project_id)
        .bind(cutoff)
        .bind(limit)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if picked == 0 {
            tx.rollback().await?;
            return Ok(None);
        }

        // Subtract from each issue the number of its events this batch removes. Rows
        // with no issue (legacy transaction rows, other non-error types) never
        // incremented a counter, so they are left out.
        sqlx::query(
            r#"
            UPDATE issues SET
                stored_event_count = stored_event_count - b.n,
                digested_event_count = digested_event_count - b.n
            FROM (
                SELECT issue_id, COUNT(*) AS n FROM cleanup_batch
                WHERE issue_id IS NOT NULL GROUP BY issue_id
            ) AS b
            WHERE issues.id = b.issue_id
            "#,
        )
        .execute(&mut *tx)
        .await?;

        // The project gives up exactly what its issues just gave up, in the
        // same commit, so a run cut short between batches leaves it right. A
        // delta costs one batch, where summing the issues would scan every
        // issue of the project under SQLite's write lock. It is also safe
        // against ingestion: on Postgres a sum read from the statement's
        // snapshot misses an in-flight `+ 1` and writes over it once the row
        // lock is released, while `count - n` re-reads the committed value.
        sqlx::query(
            r#"
            UPDATE projects SET
                stored_event_count = stored_event_count - b.n,
                digested_event_count = digested_event_count - b.n
            FROM (
                SELECT COUNT(*) AS n FROM cleanup_batch c
                JOIN issues i ON i.id = c.issue_id
            ) AS b
            WHERE projects.id = $1
            "#,
        )
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

        let events = sqlx::query("DELETE FROM events WHERE id IN (SELECT id FROM cleanup_batch)")
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;

        // No ghost shells: an issue this batch emptied goes with its events.
        let issues_removed = sqlx::query(
            "DELETE FROM issues WHERE id IN (SELECT issue_id FROM cleanup_batch) \
             AND NOT EXISTS (SELECT 1 FROM events e WHERE e.issue_id = issues.id)",
        )
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;

        sqlx::query("DROP TABLE cleanup_batch")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some((events, issues_removed)))
    }

    /// Counts what a cleanup at `cutoff` would remove. Retention is keyed on
    /// `ingested_at` (server receipt time, not client-controlled). Spans are
    /// counted through their parent transaction since they cascade. An issue is
    /// "removed" when it has events but none survive the cutoff.
    ///
    /// The scope is written as `project_id IN (...)` rather than
    /// `$n IS NULL OR project_id = $m`: the planner cannot use an index for the
    /// latter, while the former is a range scan of each project's
    /// `(project_id, ingested_at)` index instead of a pass over every row.
    async fn count_cleanup(
        pool: &DbPool,
        cutoff: chrono::DateTime<Utc>,
        project_id: Option<i32>,
        filter: CleanupFilter,
    ) -> AppResult<CleanupCounts> {
        let (events, transactions, spans, issues_removed, logs): (i64, i64, i64, i64, i64) =
            sqlx::query_as(
                r#"
            SELECT
                (SELECT COUNT(*) FROM events e
                    WHERE e.ingested_at < $1
                      AND e.project_id IN (SELECT id FROM projects WHERE $2 IS NULL OR id = $3)),
                (SELECT COUNT(*) FROM transactions t
                    WHERE t.ingested_at < $4
                      AND t.project_id IN (SELECT id FROM projects WHERE $5 IS NULL OR id = $6)),
                (SELECT COUNT(*) FROM spans s
                    JOIN transactions t2 ON s.transaction_id = t2.id
                    WHERE t2.ingested_at < $7
                      AND t2.project_id IN (SELECT id FROM projects WHERE $8 IS NULL OR id = $9)),
                (SELECT COUNT(*) FROM issues i
                    WHERE ($10 IS NULL OR i.project_id = $11)
                      AND EXISTS (SELECT 1 FROM events e2 WHERE e2.issue_id = i.id)
                      AND NOT EXISTS (
                          SELECT 1 FROM events e3 WHERE e3.issue_id = i.id AND e3.ingested_at >= $12
                      )),
                (SELECT COUNT(*) FROM logs l
                    WHERE l.ingested_at < $13
                      AND l.project_id IN (SELECT id FROM projects WHERE $14 IS NULL OR id = $15))
            "#,
            )
            .bind(cutoff)
            .bind(project_id)
            .bind(project_id)
            .bind(cutoff)
            .bind(project_id)
            .bind(project_id)
            .bind(cutoff)
            .bind(project_id)
            .bind(project_id)
            .bind(project_id)
            .bind(project_id)
            .bind(cutoff)
            .bind(cutoff)
            .bind(project_id)
            .bind(project_id)
            .fetch_one(pool)
            .await?;

        // Mask out categories the filter excludes. `spans` follow their parent
        // transaction, and an issue can only be emptied when its events are in
        // scope — so both track their governing flag, not a separate one.
        Ok(CleanupCounts {
            events: if filter.include_events { events } else { 0 },
            transactions: if filter.include_transactions {
                transactions
            } else {
                0
            },
            spans: if filter.include_transactions {
                spans
            } else {
                0
            },
            logs: if filter.include_logs { logs } else { 0 },
            issues_removed: if filter.include_events {
                issues_removed
            } else {
                0
            },
        })
    }

    /// Per-project storage breakdown (one row per project, including empty ones).
    ///
    /// Row counts are exact: correlated `COUNT(*)`s answered from each table's
    /// `project_id` index. `estimated_bytes` is the JSON payload length the
    /// project holds, estimated from a sample of its newest rows per table (see
    /// [`Self::estimate_bytes`]); summing every payload meant reading the whole
    /// database on each load of the storage page.
    pub async fn by_project(pool: &DbPool) -> AppResult<Vec<ProjectStorage>> {
        // (id, name, events, transactions, spans, logs, source_maps)
        type ProjectCountsRow = (i32, String, i64, i64, i64, i64, i64);
        let rows: Vec<ProjectCountsRow> = sqlx::query_as(
            r#"
            SELECT
                p.id,
                p.name,
                (SELECT COUNT(*) FROM events e        WHERE e.project_id = p.id) AS events_count,
                (SELECT COUNT(*) FROM transactions t  WHERE t.project_id = p.id) AS transactions_count,
                (SELECT COUNT(*) FROM spans s         WHERE s.project_id = p.id) AS spans_count,
                (SELECT COUNT(*) FROM logs lg         WHERE lg.project_id = p.id) AS logs_count,
                (SELECT COUNT(*) FROM source_file_metadata m WHERE m.project_id = p.id) AS source_maps_count
            FROM projects p
            ORDER BY p.id
            "#,
        )
        .fetch_all(pool)
        .await?;

        let mut breakdown = Vec::with_capacity(rows.len());
        for (
            project_id,
            project_name,
            events_count,
            transactions_count,
            spans_count,
            logs_count,
            source_maps_count,
        ) in rows
        {
            let estimated_bytes =
                Self::estimate_bytes(pool, EVENTS_SAMPLE, project_id, events_count).await?
                    + Self::estimate_bytes(
                        pool,
                        TRANSACTIONS_SAMPLE,
                        project_id,
                        transactions_count,
                    )
                    .await?
                    + Self::estimate_bytes(pool, SPANS_SAMPLE, project_id, spans_count).await?
                    + Self::estimate_bytes(pool, LOGS_SAMPLE, project_id, logs_count).await?;
            breakdown.push(ProjectStorage {
                project_id,
                project_name,
                events_count,
                transactions_count,
                spans_count,
                logs_count,
                source_maps_count,
                estimated_bytes,
            });
        }
        Ok(breakdown)
    }

    /// Payload bytes `count` rows hold, estimated as the bytes of a sample of
    /// the project's newest rows scaled by `count`. Exact whenever the project
    /// has no more rows than the sample.
    ///
    /// ponytail: the newest rows stand in for all of them, so a project whose
    /// payloads changed size over time is off by that drift. Store each row's
    /// size at ingest if an exact figure is ever needed.
    async fn estimate_bytes(
        pool: &DbPool,
        sample_sql: &'static str,
        project_id: i32,
        count: i64,
    ) -> AppResult<i64> {
        if count == 0 {
            return Ok(0);
        }
        let (bytes, sampled): (i64, i64) = sqlx::query_as(sample_sql)
            .bind(project_id)
            .bind(SIZE_SAMPLE_ROWS)
            .fetch_one(pool)
            .await?;
        Ok(scale_sample(bytes, sampled, count))
    }

    /// Instance-wide storage summary (row counts + DB size + source-map weight).
    pub async fn global_summary(pool: &DbPool) -> AppResult<StorageSummary> {
        let (events_count, transactions_count, spans_count, logs_count): (i64, i64, i64, i64) =
            sqlx::query_as(
                r#"
            SELECT
                (SELECT COUNT(*) FROM events)       AS events_count,
                (SELECT COUNT(*) FROM transactions) AS transactions_count,
                (SELECT COUNT(*) FROM spans)        AS spans_count,
                (SELECT COUNT(*) FROM logs)         AS logs_count
            "#,
            )
            .fetch_one(pool)
            .await?;

        Ok(StorageSummary {
            total_db_size_bytes: Self::db_size_bytes(pool).await?,
            events_count,
            transactions_count,
            spans_count,
            logs_count,
            source_maps: Self::source_map_storage(pool).await?,
        })
    }

    /// Whole-database size in bytes, reported by the backend. Best-effort: it's a
    /// headline figure, not a per-row sum.
    #[cfg(feature = "postgres")]
    pub(crate) async fn db_size_bytes(pool: &DbPool) -> AppResult<i64> {
        let (size,): (i64,) = sqlx::query_as("SELECT pg_database_size(current_database())::BIGINT")
            .fetch_one(pool)
            .await?;
        Ok(size)
    }

    /// SQLite has no `pg_database_size`; the file size is `page_count * page_size`.
    #[cfg(feature = "sqlite")]
    pub(crate) async fn db_size_bytes(pool: &DbPool) -> AppResult<i64> {
        let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
            .fetch_one(pool)
            .await?;
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
            .fetch_one(pool)
            .await?;
        Ok(page_count * page_size)
    }

    /// Exact source-map storage weight, summed from `chunk.size` + `source_file.size`.
    ///
    /// Scalar subqueries + `COALESCE(SUM(...), 0)` keep it portable across Postgres
    /// and SQLite and well-defined on an empty database (zeros, never NULL).
    pub async fn source_map_storage(pool: &DbPool) -> AppResult<SourceMapStorage> {
        let (chunk_bytes, source_file_bytes, file_count): (i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
                (SELECT COALESCE(SUM(size), 0) FROM chunk)       AS chunk_bytes,
                (SELECT COALESCE(SUM(size), 0) FROM source_file) AS source_file_bytes,
                (SELECT COUNT(*) FROM source_file)               AS file_count
            "#,
        )
        .fetch_one(pool)
        .await?;

        Ok(SourceMapStorage {
            chunk_bytes,
            source_file_bytes,
            total_bytes: chunk_bytes + source_file_bytes,
            file_count,
        })
    }
}

/// Scales a sample's `bytes` over `sampled` rows to `count` rows.
///
/// A sample short of the limit already holds every row the project has, so
/// its bytes are the exact figure. `count` was read in an earlier query, and
/// a cleanup deleting rows in between would otherwise scale what is left by
/// the count from before it.
fn scale_sample(bytes: i64, sampled: i64, count: i64) -> i64 {
    if sampled < SIZE_SAMPLE_ROWS {
        return bytes;
    }
    (i128::from(bytes) * i128::from(count) / i128::from(sampled)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sample_is_exact_whatever_the_stale_count() {
        // 100 counted, 90 deleted before the sample: the 10 left are the answer.
        assert_eq!(scale_sample(1_000, 10, 100), 1_000);
        assert_eq!(scale_sample(0, 0, 5), 0);
    }

    #[test]
    fn full_sample_scales_to_the_count() {
        assert_eq!(scale_sample(2_000, SIZE_SAMPLE_ROWS, 450), 4_500);
    }
}
