//! Engine-specific upkeep: what each backend does to give space back, report
//! its size and let writers through. Every primitive has one `cfg` here so
//! callers do not branch on the backend.

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::Compaction;

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
        live_bytes,
        reclaimable_bytes: free * page_size,
        required_disk_bytes: 2 * live_bytes,
        free_disk_bytes: free_disk_bytes(std::path::Path::new(&file)),
    })
}

#[cfg(feature = "postgres")]
pub async fn compaction(_pool: &DbPool) -> AppResult<Compaction> {
    Ok(Compaction::default())
}

/// Rewrites the database in incremental mode with one `VACUUM`. It holds
/// the write lock throughout: digests wait and retry from their files,
/// while logs, spans and sessions arriving meanwhile fail once the busy
/// timeout runs out. Measured at about 3 s per GB of live data on NVMe.
#[cfg(feature = "sqlite")]
pub async fn compact(pool: &DbPool) -> AppResult<()> {
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

#[cfg(feature = "postgres")]
pub async fn compact(_pool: &DbPool) -> AppResult<()> {
    Ok(())
}

/// Postgres reclaims a deleted row's space with its own autovacuum.
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
