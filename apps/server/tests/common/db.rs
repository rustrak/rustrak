//! Database test utilities
//!
//! Provides helpers for setting up test databases.
//! - Postgres: uses testcontainers to spin up a real PostgreSQL container
//! - SQLite: uses an isolated file database cloned from a migrated template

use rustrak::db::DbPool;

#[cfg(feature = "sqlite")]
use std::sync::Arc;

#[cfg(feature = "sqlite")]
use tempfile::TempDir;

// ── Postgres ─────────────────────────────────────────────────────────────────

#[cfg(feature = "postgres")]
use testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt};
#[cfg(feature = "postgres")]
use testcontainers_modules::postgres::Postgres;

#[cfg(feature = "postgres")]
pub struct TestDb {
    pub pool: DbPool,
}

/// One container per test binary, holding a migrated `template` database
/// every test copies. A container per test cost seconds each and, hundreds at
/// a time, Docker sometimes started one without publishing its port.
#[cfg(feature = "postgres")]
struct PgServer {
    /// `postgres://…@host:port`, without a database name.
    base_url: String,
    /// `CREATE DATABASE … TEMPLATE` refuses while another copy reads the
    /// template, so the copies take turns.
    copying: tokio::sync::Mutex<()>,
}

#[cfg(feature = "postgres")]
impl TestDb {
    pub async fn new() -> Self {
        use sqlx::Connection;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static SERVER: tokio::sync::OnceCell<PgServer> = tokio::sync::OnceCell::const_new();
        static NEXT: AtomicUsize = AtomicUsize::new(0);

        let server = SERVER.get_or_init(start_pg_server).await;
        let name = format!("test_{}", NEXT.fetch_add(1, Ordering::Relaxed));

        {
            let _turn = server.copying.lock().await;
            let mut admin = sqlx::PgConnection::connect(&format!("{}/postgres", server.base_url))
                .await
                .expect("Failed to connect to the test server");
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "CREATE DATABASE {name} TEMPLATE template"
            )))
            .execute(&mut admin)
            .await
            .expect("Failed to copy the template database");
            admin.close().await.ok();
        }

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&format!("{}/{name}", server.base_url))
            .await
            .expect("Failed to connect to test database");

        TestDb { pool }
    }
}

#[cfg(feature = "postgres")]
static PG_CONTAINER_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

#[cfg(feature = "postgres")]
extern "C" fn remove_pg_container() {
    if let Some(id) = PG_CONTAINER_ID.get() {
        let _ = std::process::Command::new("docker")
            .args(["rm", "--force", "--volumes", id])
            .output();
    }
}

#[cfg(feature = "postgres")]
async fn start_pg_server() -> PgServer {
    use sqlx::Connection;

    let container: ContainerAsync<Postgres> = Postgres::default()
        .with_tag("16-alpine")
        .with_cmd(["-c", "fsync=off", "-c", "max_connections=1000"])
        .start()
        .await
        .expect("Failed to start PostgreSQL container");
    let host = container.get_host().await.expect("Failed to get host");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("Failed to get port");
    // Shared by every test of this binary, so it is never dropped; it is
    // removed when the binary exits (testcontainers 0.27 has no reaper).
    if std::env::var("TESTCONTAINERS_COMMAND").as_deref() != Ok("keep") {
        let _ = PG_CONTAINER_ID.set(container.id().to_string());
        // SAFETY: registers a plain `extern "C"` function with no arguments.
        unsafe { libc::atexit(remove_pg_container) };
    }
    std::mem::forget(container);

    let base_url = format!("postgres://postgres:postgres@{host}:{port}");

    let mut admin = sqlx::PgConnection::connect(&format!("{base_url}/postgres"))
        .await
        .expect("Failed to connect to the test server");
    sqlx::query("CREATE DATABASE template")
        .execute(&mut admin)
        .await
        .expect("Failed to create the template database");
    admin.close().await.ok();

    let mut template = sqlx::PgConnection::connect(&format!("{base_url}/template"))
        .await
        .expect("Failed to connect to the template database");
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pgcrypto")
        .execute(&mut template)
        .await
        .expect("Failed to enable pgcrypto extension");
    sqlx::migrate!("./migrations/postgres")
        .run(&mut template)
        .await
        .expect("Failed to run migrations");
    template.close().await.ok();

    PgServer {
        base_url,
        copying: tokio::sync::Mutex::new(()),
    }
}

// ── SQLite ────────────────────────────────────────────────────────────────────

#[cfg(feature = "sqlite")]
pub struct TestDb {
    pub pool: DbPool,
    _directory: TempDir,
}

#[cfg(feature = "sqlite")]
impl TestDb {
    pub async fn new() -> Self {
        // Migrate one template once, then copy it for an isolated database.
        // This keeps parallel tests independent without paying migration setup
        // cost for every TestDb::new().
        static TEMPLATE: tokio::sync::OnceCell<Arc<TempDir>> = tokio::sync::OnceCell::const_new();
        let template = TEMPLATE
            .get_or_init(|| async {
                let directory =
                    tempfile::tempdir().expect("Failed to create SQLite template directory");
                let database_path = directory.path().join("template.sqlite");
                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect_with(
                        sqlx::sqlite::SqliteConnectOptions::new()
                            .filename(&database_path)
                            .create_if_missing(true),
                    )
                    .await
                    .expect("Failed to create SQLite template database");
                sqlx::migrate!("./migrations/sqlite")
                    .run(&pool)
                    .await
                    .expect("Failed to run SQLite migrations");
                pool.close().await;
                Arc::new(directory)
            })
            .await
            .clone();

        let directory = tempfile::tempdir().expect("Failed to create SQLite test directory");
        let database_path = directory.path().join("test.sqlite");
        std::fs::copy(template.path().join("template.sqlite"), &database_path)
            .expect("Failed to copy SQLite template database");

        // max_connections(1): SQLite only allows one writer at a time.
        // Using a single connection avoids SQLITE_LOCKED deadlocks when
        // multiple tokio tasks try to write concurrently in tests.
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&database_path)
                    .create_if_missing(false),
            )
            .await
            .expect("Failed to open copied SQLite database");

        TestDb {
            pool,
            _directory: directory,
        }
    }
}
