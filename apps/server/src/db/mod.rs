#[cfg(all(feature = "postgres", feature = "sqlite"))]
compile_error!("Features \"postgres\" and \"sqlite\" are mutually exclusive. Enable only one.");

#[cfg(not(any(feature = "postgres", feature = "sqlite")))]
compile_error!("Either feature \"postgres\" or \"sqlite\" must be enabled.");

pub mod dialect;
pub mod maintenance;

use crate::config::DatabaseConfig;
use sqlx::Connection as _;

#[cfg(feature = "postgres")]
use sqlx::postgres::{PgPool, PgPoolOptions};

#[cfg(feature = "sqlite")]
use sqlx::sqlite::{
    SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions,
    SqliteSynchronous,
};
#[cfg(feature = "sqlite")]
use std::str::FromStr;
#[cfg(feature = "sqlite")]
use std::time::Duration;

#[cfg(feature = "sqlite")]
/// How long a SQLite connection waits on the write lock before failing with
/// `SQLITE_BUSY`. This is the *whole* tolerance of every writer that is not
/// the digest (sourcemap assembly, the storage purge, bulk issue updates,
/// alert rules, the transaction/span/log processors): they get one shot at
/// the lock and no retry, so shortening it turns a slow holder into a 500.
/// 5s is the value SQLite's own guidance pairs with WAL.
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The active SQLx database backend (selected by feature flag).
#[cfg(feature = "postgres")]
pub type Db = sqlx::Postgres;

#[cfg(feature = "sqlite")]
pub type Db = sqlx::Sqlite;

/// Type alias for the database connection pool
#[cfg(feature = "postgres")]
pub type DbPool = PgPool;

#[cfg(feature = "sqlite")]
pub type DbPool = SqlitePool;

/// Creates a new database connection pool with the provided configuration
pub async fn create_pool(config: &DatabaseConfig) -> Result<DbPool, sqlx::Error> {
    log::info!("Connecting to database...");

    #[cfg(feature = "postgres")]
    let pool = {
        PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(config.acquire_timeout)
            .idle_timeout(Some(config.idle_timeout))
            .max_lifetime(Some(config.max_lifetime))
            .before_acquire(|conn, _meta| Box::pin(async move { Ok(!conn.is_in_transaction()) }))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("SET timezone = 'UTC'").execute(conn).await?;
                    Ok(())
                })
            })
            .connect(&config.url)
            .await?
    };

    #[cfg(feature = "sqlite")]
    let pool = {
        let is_in_memory = config.url.ends_with(":memory:") || config.url.contains("mode=memory");
        let opts = SqliteConnectOptions::from_str(&config.url)
            .map_err(|e| sqlx::Error::Configuration(e.into()))?
            .create_if_missing(true)
            // INCREMENTAL lets a storage cleanup hand the pages it freed back
            // to the filesystem (`maintenance::reclaim_free_space`); with
            // SQLite's default, NONE, the file never shrinks. It only takes
            // effect on a database created with it: an existing file keeps
            // its mode until a one-off `VACUUM`, see
            // apps/docs/content/usage/storage.mdx.
            .auto_vacuum(SqliteAutoVacuum::Incremental)
            // WAL + NORMAL is SQLite's own recommended pairing: the per-commit
            // WAL fsync is dropped, so write-lock holds get shorter. Commits
            // stay atomic and survive an application crash; only an OS crash
            // or power loss can roll back the newest ones, which is the right
            // trade for a stream of error events. Documented for operators in
            // apps/docs/content/configuration/database.mdx.
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            // Pool connections never checkpoint, so no request pays for the
            // whole log's checkpoint. The digest's connection owns checkpointing
            // (`connect_committer`, `ErrorProcessor::checkpoint_when_caught_up`).
            // The size limit truncates a reset log back to 64 MiB.
            .pragma("wal_autocheckpoint", "0")
            .pragma("journal_size_limit", "67108864")
            .busy_timeout(SQLITE_BUSY_TIMEOUT);
        let max_connections = if is_in_memory {
            1
        } else {
            config.max_connections
        };
        let min_connections = if is_in_memory {
            1
        } else {
            config.min_connections
        };
        SqlitePoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .acquire_timeout(config.acquire_timeout)
            .idle_timeout(Some(config.idle_timeout))
            .max_lifetime(Some(config.max_lifetime))
            // A connection sqlx still counts inside a transaction is closed,
            // never handed out: after `SQLITE_FULL` SQLite rolls back by
            // itself, sqlx's `ROLLBACK` fails and its depth stays at one, so
            // the next `BEGIN IMMEDIATE` would be refused.
            .before_acquire(|conn, _meta| Box::pin(async move { Ok(!conn.is_in_transaction()) }))
            .connect_with(opts)
            .await?
    };

    log::info!(
        "Database connection pool established (max: {}, min: {})",
        config.max_connections,
        config.min_connections
    );

    Ok(pool)
}

/// Begins a transaction intended for writes.
///
/// On SQLite this issues `BEGIN IMMEDIATE` so the write lock is acquired up
/// front. A read-then-write transaction (the digest's
/// `SELECT MAX(digest_order)` → `INSERT`) must take the write lock at
/// `BEGIN`: a deferred `BEGIN` fails the read→write upgrade with
/// `SQLITE_BUSY_SNAPSHOT`, which `busy_timeout` does not retry. Taking
/// the lock up front also prevents duplicate `digest_order` values.
///
/// On PostgreSQL the default `BEGIN` is used (MVCC + advisory locks handle
/// concurrency; `IMMEDIATE` is not valid Postgres syntax).
///
/// Use ONLY for read-then-write transactions (a SELECT before the first
/// write). Write-first txs must keep a plain `pool.begin()`: IMMEDIATE
/// would hold the write lock across disk I/O and long loops for no benefit
/// — those sites carry a comment saying so.
pub async fn begin_write(pool: &DbPool) -> Result<sqlx::Transaction<'_, Db>, sqlx::Error> {
    #[cfg(feature = "sqlite")]
    {
        pool.begin_with("BEGIN IMMEDIATE").await
    }
    #[cfg(feature = "postgres")]
    {
        pool.begin().await
    }
}

/// The committer's automatic checkpoint, in pages of write-ahead log (~40
/// MiB at 4 KiB pages), while it is behind ingest: batches write megabytes,
/// and at 1 000 pages nearly every commit also checkpointed. Ingest's
/// free-disk floor counts this much log (`free_disk_floor`).
#[cfg(feature = "sqlite")]
pub const COMMITTER_CHECKPOINT_PAGES: u32 = 10_000;

/// The committer's automatic checkpoint once it has caught up with ingest:
/// none, each commit is checkpointed right after it instead
/// ([`checkpoint_caught_up`]).
#[cfg(feature = "sqlite")]
pub const CAUGHT_UP_CHECKPOINT_PAGES: u32 = 0;

/// How long [`checkpoint_caught_up`] waits for a reader on an older snapshot,
/// or for another writer, before it settles for what it could copy.
#[cfg(feature = "sqlite")]
const CAUGHT_UP_CHECKPOINT_WAIT: Duration = Duration::from_millis(5);

/// A `FULL` checkpoint on the digest's connection, right after a commit made
/// while the digest keeps up. A checkpoint ends in an fsync of the database
/// file that the spool fsyncs of arriving requests wait behind. A `PASSIVE`
/// one gives up on frames a busy reader still sees, so pages pile up into
/// rare, long syncs; waiting a few milliseconds for the readers syncs each
/// commit's few pages instead.
///
/// `FULL` takes the write lock while it copies, as an automatic checkpoint
/// at `COMMIT` does; it gives up after [`CAUGHT_UP_CHECKPOINT_WAIT`] on a
/// reader or another writer and leaves the rest to the next one. It changes
/// nothing about durability: the log is synced before the copy and the
/// database after it.
#[cfg(feature = "sqlite")]
pub async fn checkpoint_caught_up(conn: &mut sqlx::SqliteConnection) -> Result<(), sqlx::Error> {
    let busy_timeout = |wait: Duration| {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA busy_timeout = {}",
            wait.as_millis()
        )))
    };
    busy_timeout(CAUGHT_UP_CHECKPOINT_WAIT)
        .execute(&mut *conn)
        .await?;
    let checkpoint = sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(&mut *conn)
        .await;
    busy_timeout(SQLITE_BUSY_TIMEOUT)
        .execute(&mut *conn)
        .await?;
    checkpoint.map(drop)
}

/// The digest's own connection, outside the pool and with every option of
/// the pool's, except that each commit on it is durable: SQLite
/// `synchronous=FULL` (a commit fsyncs the write-ahead log, which also covers
/// the `NORMAL` commits before it), Postgres `synchronous_commit=on`
/// whatever the server's default. The digest deletes an event's spool file
/// right after the commit that stored it, so that commit has to survive a
/// power loss.
pub async fn connect_committer(
    pool: &DbPool,
) -> Result<<Db as sqlx::Database>::Connection, sqlx::Error> {
    #[cfg(feature = "sqlite")]
    let options = pool
        .connect_options()
        .as_ref()
        .clone()
        .synchronous(SqliteSynchronous::Full)
        // The only connection that checkpoints (the pool's never do, see
        // `create_pool`); each batch sets its own threshold
        // (`checkpoint_pages`), and a timer checkpoints once the digest is
        // caught up, which covers the writes that never reach it.
        .pragma("wal_autocheckpoint", COMMITTER_CHECKPOINT_PAGES.to_string());
    #[cfg(feature = "postgres")]
    let options = pool
        .connect_options()
        .as_ref()
        .clone()
        .options([("synchronous_commit", "on"), ("timezone", "UTC")]);
    sqlx::ConnectOptions::connect(&options).await
}

/// Runs all pending database migrations
pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    log::info!("Running database migrations...");

    #[cfg(feature = "postgres")]
    sqlx::migrate!("./migrations/postgres").run(pool).await?;

    #[cfg(feature = "sqlite")]
    sqlx::migrate!("./migrations/sqlite").run(pool).await?;

    log::info!("Database migrations completed successfully");
    Ok(())
}

/// Performs a health check on the database connection
pub async fn health_check(pool: &DbPool) -> bool {
    sqlx::query("SELECT 1").execute(pool).await.is_ok()
}

/// Runs a full SQLite WAL checkpoint and reports whether it completed.
///
/// A commit is durably in the main database file once any `FULL` checkpoint
/// that *started* after the commit completes. Shutdown runs one last.
#[cfg(feature = "sqlite")]
pub async fn checkpoint_full(pool: &DbPool) -> Result<bool, sqlx::Error> {
    let busy: i64 = sqlx::query_scalar("PRAGMA wal_checkpoint(FULL)")
        .fetch_one(pool)
        .await?;
    Ok(busy == 0)
}

/// Which engine this binary was built for.
pub fn backend_name() -> &'static str {
    #[cfg(feature = "postgres")]
    {
        "postgres"
    }
    #[cfg(not(feature = "postgres"))]
    {
        "sqlite"
    }
}

/// The engine's own version, `major.minor` only.
pub async fn engine_version(pool: &DbPool) -> Result<String, sqlx::Error> {
    #[cfg(feature = "postgres")]
    let raw: String = sqlx::query_scalar("SHOW server_version")
        .fetch_one(pool)
        .await?;
    #[cfg(not(feature = "postgres"))]
    let raw: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(pool)
        .await?;
    Ok(major_minor(&raw))
}

/// `3.46.1` reads as `3.46`, `16.4 (Debian 16.4-1)` as `16.4`.
pub fn major_minor(raw: &str) -> String {
    raw.split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()
        .unwrap_or("")
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".")
}
