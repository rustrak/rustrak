//! Engine-specific upkeep: what each backend does to give space back, report
//! its size and let writers through. Every primitive has one `cfg` here so
//! callers do not branch on the backend.

use crate::db::DbPool;
#[cfg(feature = "postgres")]
use crate::error::AppError;
use crate::error::AppResult;
#[cfg(feature = "postgres")]
use crate::models::TableSize;
use crate::models::{CompactedTable, Compaction};

/// One step of handing freed pages back to the filesystem: 2,000 pages, 8 MiB
/// at SQLite's default 4 KiB page. Each step is its own write transaction, so
/// like a cleanup batch it holds the write lock briefly and ingestion gets a
/// turn between steps.
#[cfg(feature = "sqlite")]
const INCREMENTAL_VACUUM_STEP: &str = "PRAGMA incremental_vacuum(2000)";

/// Gives waiting writers a turn between cleanup batches. SQLite's busy
/// handler retries a blocked writer at intervals of up to 100ms, so a pause
/// at least that long guarantees ingestion gets the lock between batches
/// instead of starving behind the cleanup. Postgres writers do not wait on
/// the cleanup, so it needs none.
pub async fn yield_to_writers() {
    #[cfg(feature = "sqlite")]
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

/// Free bytes on the filesystem holding `file`, `None` when unreadable (an
/// in-memory database, or a platform without `statvfs`).
#[cfg(all(feature = "sqlite", unix))]
// `fsblkcnt_t` and `c_ulong` are u64 on Linux but u32 on macOS.
#[allow(clippy::unnecessary_cast)]
fn free_disk_bytes(file: &std::path::Path) -> Option<i64> {
    let stat = nix::sys::statvfs::statvfs(file.parent()?).ok()?;
    let bytes = (stat.blocks_available() as u64).checked_mul(stat.fragment_size() as u64)?;
    i64::try_from(bytes).ok()
}

#[cfg(all(feature = "sqlite", not(unix)))]
fn free_disk_bytes(_file: &std::path::Path) -> Option<i64> {
    None
}

/// Hands the pages a cleanup freed back to the filesystem, returning the
/// bytes the database file gave up: 0 when there was nothing to release,
/// or when a reader kept the closing checkpoint from shrinking the file.
///
/// SQLite keeps a deleted row's pages on its freelist and reuses them, so
/// after a cleanup the database stops growing but its file never shrinks.
/// A database created with `auto_vacuum = INCREMENTAL` (every database
/// `db::create_pool` creates) releases them with `incremental_vacuum`, a
/// bounded step at a time. A database created before keeps `NONE`, which
/// only a one-off `VACUUM` changes, and is left alone.
#[cfg(feature = "sqlite")]
pub async fn reclaim_free_space(pool: &DbPool) -> AppResult<i64> {
    let auto_vacuum: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(pool)
        .await?;
    // 0 = NONE, 1 = FULL (already shrinks on every commit), 2 = INCREMENTAL.
    if auto_vacuum != 2 {
        return Ok(0);
    }
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await?;
    let mut released = 0;
    loop {
        let free: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(pool)
            .await?;
        if free == 0 {
            break;
        }
        sqlx::query(INCREMENTAL_VACUUM_STEP).execute(pool).await?;
        let left: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(pool)
            .await?;
        if left >= free {
            break;
        }
        released += free - left;
        yield_to_writers().await;
    }
    // Under WAL the shrink is recorded in the log; the checkpoint applies
    // it to the database file. PASSIVE never waits: TRUNCATE or FULL
    // would hold every writer back for the busy timeout while a reader
    // sits on an older snapshot. A checkpoint that could not copy the
    // whole log leaves the shrink to a later one, so nothing has left the
    // file yet.
    let (_, logged, copied): (i64, i64, i64) = sqlx::query_as("PRAGMA wal_checkpoint(PASSIVE)")
        .fetch_one(pool)
        .await?;
    if copied < logged {
        return Ok(0);
    }
    Ok(released * page_size)
}

/// Whether the database file can give freed space back, and what
/// compacting it once would take. See [`Compaction`].
#[cfg(feature = "sqlite")]
pub async fn compaction(pool: &DbPool) -> AppResult<Compaction> {
    let auto_vacuum: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(pool)
        .await?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await?;
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await?;
    let free: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(pool)
        .await?;
    let file: String =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
            .fetch_one(pool)
            .await?;
    let live_bytes = (pages - free) * page_size;
    Ok(Compaction {
        needed: auto_vacuum == 0,
        file_bytes: pages * page_size,
        live_bytes: Some(live_bytes),
        reclaimable_bytes: Some(free * page_size),
        required_disk_bytes: 2 * live_bytes,
        free_disk_bytes: free_disk_bytes(std::path::Path::new(&file)),
        tables: Vec::new(),
    })
}

/// The tables retention deletes from: where the space a cleanup frees is,
/// and what a compaction rewrites.
#[cfg(feature = "postgres")]
const RETAINED_TABLES: [&str; 4] = ["events", "transactions", "spans", "logs"];

/// The [`RETAINED_TABLES`], smallest first: each table rewritten gives its
/// free space back before the next, larger one needs room for its copy.
#[cfg(feature = "postgres")]
async fn retained_tables(pool: &DbPool) -> AppResult<Vec<TableSize>> {
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT relname::text, pg_table_size(oid), pg_indexes_size(oid) FROM pg_class \
         WHERE oid = ANY($1::text[]::regclass[]) ORDER BY pg_total_relation_size(oid), relname",
    )
    .bind(&RETAINED_TABLES[..])
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(table, table_bytes, index_bytes)| TableSize {
            table,
            table_bytes,
            index_bytes,
        })
        .collect())
}

/// Vacuum makes a deleted row's space reusable inside its table but never
/// hands it to the filesystem, short of empty pages at the very end; only a
/// rewrite does. How much of a table is free space takes an extension
/// (`pgstattuple`) to tell, so none is claimed: the sizes are what can be
/// known.
#[cfg(feature = "postgres")]
pub async fn compaction(pool: &DbPool) -> AppResult<Compaction> {
    let tables = retained_tables(pool).await?;
    Ok(Compaction {
        needed: true,
        file_bytes: size_bytes(pool).await?,
        live_bytes: None,
        reclaimable_bytes: None,
        required_disk_bytes: tables
            .iter()
            .map(|t| t.table_bytes + t.index_bytes)
            .max()
            .unwrap_or(0),
        free_disk_bytes: None,
        tables,
    })
}

/// How long the compaction waits for a table's lock. Queued, its request
/// holds every later query on the table behind it, so it gives up soon
/// rather than stall ingestion behind a long reader.
#[cfg(feature = "postgres")]
const COMPACTION_LOCK_TIMEOUT_SECS: u64 = 5;

/// Rewrites each of the [`RETAINED_TABLES`] with `VACUUM FULL`, smallest
/// first, calling `on_table` after each. A table is locked (`ACCESS
/// EXCLUSIVE`) for its whole rewrite, a few seconds per GB it keeps, and
/// named in `rewriting` meanwhile: ingest turns away the items it would
/// write into it on the request path (see `pipeline::shed_by`), which would
/// otherwise wait on the lock until they held the whole pool. Digests and
/// dashboard reads of it wait. The old files go only once the new copy is
/// complete, so a run that fails or is cancelled leaves that table as it
/// was, and needs free disk for that copy.
///
/// It runs on a session of its own, outside any transaction (`VACUUM`
/// refuses one) and never handed back to the pool with its timeouts.
#[cfg(feature = "postgres")]
pub async fn compact(
    pool: &DbPool,
    rewriting: &std::sync::Mutex<Option<String>>,
    mut on_table: impl FnMut(CompactedTable),
) -> AppResult<()> {
    let set = |table: Option<&str>| {
        *rewriting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = table.map(str::to_string);
    };
    let mut conn = pool.acquire().await?.detach();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SET lock_timeout = '{COMPACTION_LOCK_TIMEOUT_SECS}s'"
    )))
    .execute(&mut conn)
    .await?;
    // A role's own statement_timeout would cancel a large table mid-copy.
    sqlx::query("SET statement_timeout = 0")
        .execute(&mut conn)
        .await?;
    for table in retained_tables(pool).await? {
        let started = std::time::Instant::now();
        // Named before the lock is asked for: queued, it already blocks.
        set(Some(&table.table));
        // The name is one of RETAINED_TABLES, read back from pg_class.
        let vacuumed = sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM FULL {}", table.table)))
            .execute(&mut conn)
            .await;
        set(None);
        vacuumed.map_err(|e| match e {
            sqlx::Error::Database(ref d) if d.code().as_deref() == Some("55P03") => {
                AppError::Conflict(format!(
                    "Could not lock {} within {COMPACTION_LOCK_TIMEOUT_SECS} s: another \
                         query was using it. It was left as it was; try again at a quieter time.",
                    table.table
                ))
            }
            e => e.into(),
        })?;
        let duration_ms = started.elapsed().as_millis() as i64;
        let bytes_after: i64 = sqlx::query_scalar("SELECT pg_total_relation_size($1::regclass)")
            .bind(&table.table)
            .fetch_one(&mut conn)
            .await?;
        on_table(CompactedTable {
            bytes_before: table.table_bytes + table.index_bytes,
            bytes_after,
            duration_ms,
            table: table.table,
        });
    }
    sqlx::Connection::close(conn).await?;
    Ok(())
}

/// Rewrites the database in incremental mode with one `VACUUM`. It holds
/// the write lock throughout: digests wait and retry from their files,
/// while logs, spans and sessions arriving meanwhile fail once the busy
/// timeout runs out. Measured at about 3 s per GB of live data on NVMe.
#[cfg(feature = "sqlite")]
pub async fn compact(
    pool: &DbPool,
    _rewriting: &std::sync::Mutex<Option<String>>,
    _on_table: impl FnMut(CompactedTable),
) -> AppResult<()> {
    let mut conn = pool.acquire().await?;
    sqlx::query("PRAGMA auto_vacuum = INCREMENTAL")
        .execute(&mut *conn)
        .await?;
    sqlx::query("VACUUM").execute(&mut *conn).await?;
    // The log now holds a copy of the whole database. TRUNCATE gives that
    // back too; it may wait up to the busy timeout on a reader, which
    // next to the VACUUM itself is nothing. Best effort: the next
    // checkpoints apply it anyway, only the log file keeps its size.
    if let Err(error) = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *conn)
        .await
    {
        log::warn!("Compaction could not truncate the write-ahead log: {error}");
    }
    Ok(())
}

/// Postgres: autovacuum makes a deleted row's space reusable, which keeps a
/// table from growing under retention. A plain `VACUUM` here would hand back
/// only empty pages at a table's end, and retention empties its oldest pages,
/// at the start; giving the rest back takes [`compact`].
#[cfg(feature = "postgres")]
pub async fn reclaim_free_space(_pool: &DbPool) -> AppResult<i64> {
    Ok(0)
}

/// Whole-database size in bytes, reported by the backend. Best-effort: it's a
/// headline figure, not a per-row sum.
#[cfg(feature = "postgres")]
pub async fn size_bytes(pool: &DbPool) -> AppResult<i64> {
    let (size,): (i64,) = sqlx::query_as("SELECT pg_database_size(current_database())::BIGINT")
        .fetch_one(pool)
        .await?;
    Ok(size)
}

/// SQLite has no `pg_database_size`; the file size is `page_count * page_size`.
#[cfg(feature = "sqlite")]
pub async fn size_bytes(pool: &DbPool) -> AppResult<i64> {
    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await?;
    Ok(page_count * page_size)
}
