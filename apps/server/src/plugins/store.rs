//! A plugin's own SQLite file. The plugin writes SQL; the host keeps that SQL
//! inside the file (no ATTACH, no PRAGMA, no touching the migration ledger),
//! inside its quota and inside its time budget.

use std::path::Path;
use std::sync::{Mutex, TryLockError};
use std::time::{Duration, Instant};

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::types::{ToSqlOutput, ValueRef};
use rusqlite::{Connection, ErrorCode, ToSql};

use super::package::Migration;
use super::runtime::MEMORY_LIMIT;

const LEDGER: &str = "_rustrak_migrations";

/// A column value, the same shape as the WIT `sql.value`.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SqlError {
    #[error("constraint failed: {0}")]
    Constraint(String),
    #[error("the plugin's storage quota is full")]
    DatabaseFull,
    #[error("the database is busy")]
    Busy,
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct MigrationError(String);

impl From<rusqlite::Error> for SqlError {
    fn from(e: rusqlite::Error) -> Self {
        match e.sqlite_error_code() {
            Some(ErrorCode::ConstraintViolation) => SqlError::Constraint(e.to_string()),
            Some(ErrorCode::DiskFull) => SqlError::DatabaseFull,
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => SqlError::Busy,
            Some(ErrorCode::OperationInterrupted) => {
                SqlError::Other("interrupted: time limit reached".into())
            }
            _ => SqlError::Other(e.to_string()),
        }
    }
}

impl ToSql for Value {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(match self {
            Value::Null => ValueRef::Null,
            Value::Integer(i) => ValueRef::Integer(*i),
            Value::Real(f) => ValueRef::Real(*f),
            Value::Text(s) => ValueRef::Text(s.as_bytes()),
            Value::Blob(b) => ValueRef::Blob(b),
        }))
    }
}

impl From<ValueRef<'_>> for Value {
    fn from(v: ValueRef<'_>) -> Self {
        match v {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(i) => Value::Integer(i),
            ValueRef::Real(f) => Value::Real(f),
            ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
            ValueRef::Blob(b) => Value::Blob(b.to_vec()),
        }
    }
}

/// What plugin SQL may do: anything inside its own file, except reach
/// another file, change how the file is run, or rewrite the ledger.
fn fence(ctx: AuthContext<'_>) -> Authorization {
    match ctx.action {
        AuthAction::Attach { .. } | AuthAction::Detach { .. } | AuthAction::Pragma { .. } => {
            Authorization::Deny
        }
        AuthAction::Insert { table_name }
        | AuthAction::Delete { table_name }
        | AuthAction::Update { table_name, .. }
        | AuthAction::DropTable { table_name }
        | AuthAction::AlterTable { table_name, .. }
            if table_name.eq_ignore_ascii_case(LEDGER) =>
        {
            Authorization::Deny
        }
        _ => Authorization::Allow,
    }
}

/// The fence, for SQL the host wraps in its own transaction: the plugin's
/// `COMMIT` would otherwise end it halfway.
fn strict(ctx: AuthContext<'_>) -> Authorization {
    match ctx.action {
        AuthAction::Transaction { .. } | AuthAction::Savepoint { .. } => Authorization::Deny,
        _ => fence(ctx),
    }
}

/// How long the host's own work on a plugin's file may take.
const HOUSEKEEPING: Duration = Duration::from_secs(600);

/// A row older than `?1` (unix seconds): `created_at` as a number, or as
/// ISO text read as an instant ('T', 'Z' and offsets included).
const EXPIRED: &str = "((typeof(created_at) IN ('integer', 'real') AND created_at < ?1) \
     OR (typeof(created_at) = 'text' AND unixepoch(created_at) < ?1))";

/// One history table of a plugin and how much of it is past the window.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HistoryTable {
    pub table: String,
    pub rows: i64,
    pub expired: i64,
}

/// The plugin's tables with a `created_at` column, by name.
fn history_tables(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let names: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_schema WHERE type = 'table' \
             AND substr(name, 1, 7) <> 'sqlite_' AND substr(name, 1, 8) <> '_rustrak' ORDER BY name",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(names
        .into_iter()
        .filter(|t| !t.contains('"'))
        .filter(|t| {
            conn.prepare(&format!("SELECT created_at FROM \"{t}\" LIMIT 0"))
                .is_ok()
        })
        .collect())
}

pub struct PluginDb {
    // One connection for reads and writes; a reader pool when a
    // plugin's dashboards measurably wait on its writes.
    conn: Mutex<Connection>,
}

impl PluginDb {
    pub fn open(path: &Path, storage_mb: u32) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        // No single value bigger than a plugin instance could hold.
        conn.set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            MEMORY_LIMIT as i32,
        )?;
        let page_size: i64 = conn.pragma_query_value(None, "page_size", |r| r.get(0))?;
        let pages = i64::from(storage_mb) * 1024 * 1024 / page_size;
        conn.pragma_update(None, "max_page_count", pages)?;
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {LEDGER} (
                 version INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 checksum TEXT NOT NULL,
                 applied_at INTEGER NOT NULL
             );"
        ))?;
        conn.authorizer(Some(fence))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Applies the pending migrations, all in one transaction: an upgrade
    /// either lands whole or leaves the file as it was.
    pub fn migrate(&self, migrations: &[Migration]) -> Result<(), MigrationError> {
        self.migrate_within(migrations, HOUSEKEEPING)
    }

    fn migrate_within(
        &self,
        migrations: &[Migration],
        limit: Duration,
    ) -> Result<(), MigrationError> {
        let mut conn = self.conn.lock().expect("plugin db lock poisoned");
        let applied: Vec<(u32, String)> = {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT version, checksum FROM {LEDGER} ORDER BY version"
                ))
                .map_err(|e| MigrationError(e.to_string()))?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .and_then(|rows| rows.collect())
                .map_err(|e| MigrationError(e.to_string()))?
        };
        for m in migrations {
            if let Some((_, checksum)) = applied.iter().find(|(v, _)| *v == m.version) {
                if *checksum != m.checksum {
                    return Err(MigrationError(format!(
                        "migration {:04}_{} was changed after it was applied",
                        m.version, m.name
                    )));
                }
            }
        }
        let pending: Vec<&Migration> = migrations
            .iter()
            .filter(|m| !applied.iter().any(|(v, _)| *v == m.version))
            .collect();
        if pending.is_empty() {
            return Ok(());
        }

        let host = |e: rusqlite::Error| MigrationError(e.to_string());
        let deadline = Instant::now() + limit;
        conn.progress_handler(1_000, Some(move || Instant::now() > deadline))
            .map_err(host)?;
        let result = (|| {
            let tx = conn.transaction().map_err(host)?;
            for m in pending {
                tx.authorizer(Some(strict)).map_err(host)?;
                let applied = tx.execute_batch(&m.sql);
                tx.authorizer(Some(fence)).map_err(host)?;
                applied.map_err(|e| {
                    MigrationError(format!("migration {:04}_{}: {e}", m.version, m.name))
                })?;
                // The ledger is the host's: lift the fence for this one write.
                tx.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
                    .map_err(host)?;
                let recorded = tx.execute(
                    &format!("INSERT INTO {LEDGER} (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, unixepoch())"),
                    rusqlite::params![m.version, m.name, m.checksum],
                );
                tx.authorizer(Some(fence)).map_err(host)?;
                recorded.map_err(host)?;
            }
            tx.commit().map_err(host)
        })();
        conn.progress_handler(0, None::<fn() -> bool>)
            .map_err(host)?;
        result
    }

    pub fn execute(&self, sql: &str, params: &[Value], budget: Duration) -> Result<u64, SqlError> {
        self.with_deadline(Instant::now() + budget, |conn| {
            Ok(conn.execute(sql, rusqlite::params_from_iter(params))? as u64)
        })
    }

    pub fn query(&self, sql: &str, params: &[Value], budget: Duration) -> Result<Rows, SqlError> {
        self.with_deadline(Instant::now() + budget, |conn| {
            let mut stmt = conn.prepare(sql)?;
            let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
            let width = columns.len();
            let mut rows = Vec::new();
            // What the result will take in the plugin's memory, roughly: it
            // cannot hold more than its whole linear memory.
            let mut bytes = 0;
            let mut cursor = stmt.query(rusqlite::params_from_iter(params))?;
            while let Some(row) = cursor.next()? {
                let mut values = Vec::with_capacity(width);
                for i in 0..width {
                    let v = row.get_ref(i)?;
                    bytes += match v {
                        ValueRef::Text(b) | ValueRef::Blob(b) => b.len(),
                        _ => 8,
                    };
                    if bytes > MEMORY_LIMIT {
                        return Err(SqlError::Other(format!(
                            "the result is larger than the plugin's memory ({MEMORY_LIMIT} bytes)"
                        )));
                    }
                    values.push(Value::from(v));
                }
                rows.push(values);
            }
            Ok(Rows { columns, rows })
        })
    }

    /// Several statements in one transaction.
    pub fn batch(
        &self,
        statements: &[(String, Vec<Value>)],
        budget: Duration,
    ) -> Result<(), SqlError> {
        self.with_deadline(Instant::now() + budget, |conn| {
            let tx = conn.unchecked_transaction()?;
            tx.authorizer(Some(strict))?;
            let ran = statements.iter().try_for_each(|(sql, params)| {
                tx.execute(sql, rusqlite::params_from_iter(params))
                    .map(drop)
            });
            tx.authorizer(Some(fence))?;
            ran?;
            tx.commit()?;
            Ok(())
        })
    }

    /// The plugin's history: every table with a `created_at` column, unix
    /// seconds or ISO text. Rustrak's retention clears them; the rest of the
    /// plugin's data it never touches. `expired` counts the rows older than
    /// `cutoff` (unix seconds), none without one.
    pub fn history(&self, cutoff: Option<i64>) -> Result<Vec<HistoryTable>, SqlError> {
        self.with_deadline(Instant::now() + HOUSEKEEPING, |conn| {
            let mut out = Vec::new();
            for table in history_tables(conn)? {
                let (rows, expired) = conn.query_row(
                    &format!(
                        "SELECT count(*), coalesce(sum({}), 0) FROM \"{table}\"",
                        EXPIRED
                    ),
                    // No cutoff is the earliest instant: nothing is older.
                    rusqlite::params![cutoff.unwrap_or(i64::MIN)],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )?;
                out.push(HistoryTable {
                    table,
                    rows,
                    expired,
                });
            }
            Ok(out)
        })
    }

    /// Deletes every history row older than `cutoff` (unix seconds), in one
    /// transaction; how many went.
    pub fn forget_before(&self, cutoff: i64) -> Result<u64, SqlError> {
        self.with_deadline(Instant::now() + HOUSEKEEPING, |conn| {
            let tx = conn.unchecked_transaction()?;
            let mut removed = 0;
            for table in history_tables(&tx)? {
                removed += tx.execute(
                    &format!("DELETE FROM \"{table}\" WHERE {EXPIRED}"),
                    rusqlite::params![cutoff],
                )? as u64;
            }
            tx.commit()?;
            Ok(removed)
        })
    }

    fn with_deadline<T>(
        &self,
        deadline: Instant,
        f: impl FnOnce(&Connection) -> Result<T, SqlError>,
    ) -> Result<T, SqlError> {
        // The wait for the lock (housekeeping holds it for minutes) is part
        // of the budget: nothing interrupts a host call blocked in `lock()`.
        let conn = loop {
            match self.conn.try_lock() {
                Ok(conn) => break conn,
                Err(TryLockError::Poisoned(p)) => break p.into_inner(),
                Err(TryLockError::WouldBlock) if Instant::now() >= deadline => {
                    return Err(SqlError::Busy);
                }
                // Polls; a condvar if this ever shows up in a profile.
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        conn.progress_handler(1_000, Some(move || Instant::now() > deadline))?;
        let result = f(&conn);
        conn.progress_handler(0, None::<fn() -> bool>)?;
        // A plugin's `BEGIN` ends with its call: left open, it would hold
        // every later write, the host's included, in a transaction never
        // committed.
        if !conn.is_autocommit() {
            conn.execute_batch("ROLLBACK")?;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAR: Duration = Duration::from_secs(5);

    fn open(dir: &tempfile::TempDir) -> PluginDb {
        PluginDb::open(&dir.path().join("data.sqlite"), 64).expect("open")
    }

    fn migration(version: u32, sql: &str) -> Migration {
        Migration {
            version,
            name: format!("m{version}"),
            sql: sql.into(),
            checksum: format!("c{version}"),
        }
    }

    #[test]
    fn a_huge_blob_is_refused_before_it_is_built() {
        let dir = tempfile::tempdir().unwrap();
        // Not `expect_err`: a passing query would print a gigabyte.
        let Err(err) = open(&dir).query("SELECT zeroblob(1000000000)", &[], FAR) else {
            panic!("refused");
        };
        assert!(matches!(err, SqlError::Other(_)), "{err:?}");
    }

    #[test]
    fn rows_past_the_guest_memory_limit_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let Err(err) = open(&dir).query(
            "WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 40) \
             SELECT zeroblob(1048576) FROM c",
            &[],
            FAR,
        ) else {
            panic!("refused");
        };
        assert!(err.to_string().contains("memory"), "{err}");
    }

    #[test]
    fn a_deadline_spent_waiting_for_the_lock_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let guard = db.conn.lock().unwrap();
        std::thread::scope(|s| {
            let call = s.spawn(|| db.query("SELECT 1", &[], Duration::from_millis(50)));
            // Busy at its deadline, while the lock is still held: not once
            // housekeeping lets go minutes later.
            std::thread::sleep(Duration::from_millis(500));
            assert!(call.is_finished(), "the call waited past its deadline");
            drop(guard);
            assert_eq!(call.join().unwrap(), Err(SqlError::Busy));
        });
    }

    #[test]
    fn a_migration_cannot_commit_halfway() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let err = db
            .migrate(&[migration(1, "CREATE TABLE a (x); COMMIT; nonsense")])
            .unwrap_err();
        assert!(err.to_string().contains("0001"), "{err}");
        let left = db
            .query(
                "SELECT count(*) FROM sqlite_schema WHERE name = 'a'",
                &[],
                FAR,
            )
            .unwrap();
        assert_eq!(left.rows, [[Value::Integer(0)]]);
        // The fence is back: plugin SQL runs, and still cannot write the ledger.
        db.execute("CREATE TABLE b (x)", &[], FAR).unwrap();
        assert!(db
            .execute(&format!("DELETE FROM {LEDGER}"), &[], FAR)
            .is_err());
    }

    #[test]
    fn a_batch_cannot_commit_halfway() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        db.execute("CREATE TABLE a (x)", &[], FAR).unwrap();
        let stmts = [
            ("INSERT INTO a VALUES (1)".to_string(), vec![]),
            ("COMMIT".to_string(), vec![]),
            ("nonsense".to_string(), vec![]),
        ];
        assert!(db.batch(&stmts, FAR).is_err());
        let rows = db.query("SELECT count(*) FROM a", &[], FAR).unwrap();
        assert_eq!(rows.rows, [[Value::Integer(0)]]);
    }

    #[test]
    fn a_migration_that_never_ends_is_interrupted_and_rolled_back() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        let started = Instant::now();
        let err = db
            .migrate_within(
                &[migration(
                    1,
                    "CREATE TABLE a (x); \
                     WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c) \
                     SELECT count(*) FROM c;",
                )],
                Duration::from_millis(100),
            )
            .unwrap_err();
        assert!(started.elapsed() < FAR, "{err}");
        let left = db
            .query(
                "SELECT count(*) FROM sqlite_schema WHERE name = 'a'",
                &[],
                FAR,
            )
            .unwrap();
        assert_eq!(left.rows, [[Value::Integer(0)]]);
    }

    #[test]
    fn retention_reads_iso_text_as_an_instant() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir);
        db.execute("CREATE TABLE h (created_at)", &[], FAR).unwrap();
        // The cutoff, 1_700_000_000, is 2023-11-14 22:13:20 UTC.
        for at in [
            "2023-11-14T22:13:19Z",
            "2023-11-14 23:13:19+01:00",
            "2023-11-14T22:13:21Z",
            "2023-11-14T23:13:21+01:00",
        ] {
            db.execute("INSERT INTO h VALUES (?1)", &[Value::Text(at.into())], FAR)
                .unwrap();
        }
        assert_eq!(db.forget_before(1_700_000_000).unwrap(), 2);
        let left = db
            .query("SELECT created_at FROM h ORDER BY created_at", &[], FAR)
            .unwrap();
        assert_eq!(
            left.rows,
            [
                [Value::Text("2023-11-14T22:13:21Z".into())],
                [Value::Text("2023-11-14T23:13:21+01:00".into())],
            ]
        );
    }
}
