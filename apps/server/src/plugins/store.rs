//! A plugin's own SQLite file. The plugin writes SQL; the host keeps that SQL
//! inside the file (no ATTACH, no PRAGMA, no touching the migration ledger),
//! inside its quota and inside its time budget.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::types::{ToSqlOutput, ValueRef};
use rusqlite::{Connection, ErrorCode, ToSql};

use super::package::Migration;

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

/// How long the host's own work on a plugin's file may take.
const HOUSEKEEPING: Duration = Duration::from_secs(600);

/// A row older than `?1` (unix seconds): `created_at` as a number, or as
/// ISO text compared to the same instant.
const EXPIRED: &str = "((typeof(created_at) IN ('integer', 'real') AND created_at < ?1) \
     OR (typeof(created_at) = 'text' AND created_at < datetime(?1, 'unixepoch')))";

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
    // ponytail: one connection for reads and writes; a reader pool when a
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

        let tx = conn
            .transaction()
            .map_err(|e| MigrationError(e.to_string()))?;
        for m in pending {
            tx.execute_batch(&m.sql).map_err(|e| {
                MigrationError(format!("migration {:04}_{}: {e}", m.version, m.name))
            })?;
            // The ledger is the host's: lift the fence for this one write.
            let ledger = |e: rusqlite::Error| MigrationError(e.to_string());
            tx.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
                .map_err(ledger)?;
            let recorded = tx.execute(
                &format!("INSERT INTO {LEDGER} (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, unixepoch())"),
                rusqlite::params![m.version, m.name, m.checksum],
            );
            tx.authorizer(Some(fence)).map_err(ledger)?;
            recorded.map_err(ledger)?;
        }
        tx.commit().map_err(|e| MigrationError(e.to_string()))
    }

    pub fn execute(&self, sql: &str, params: &[Value], budget: Duration) -> Result<u64, SqlError> {
        self.with_deadline(budget, |conn| {
            Ok(conn.execute(sql, rusqlite::params_from_iter(params))? as u64)
        })
    }

    pub fn query(&self, sql: &str, params: &[Value], budget: Duration) -> Result<Rows, SqlError> {
        self.with_deadline(budget, |conn| {
            let mut stmt = conn.prepare(sql)?;
            let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
            let width = columns.len();
            let mut rows = Vec::new();
            let mut cursor = stmt.query(rusqlite::params_from_iter(params))?;
            while let Some(row) = cursor.next()? {
                rows.push(
                    (0..width)
                        .map(|i| row.get_ref(i).map(Value::from))
                        .collect::<Result<_, _>>()?,
                );
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
        self.with_deadline(budget, |conn| {
            let tx = conn.unchecked_transaction()?;
            for (sql, params) in statements {
                tx.execute(sql, rusqlite::params_from_iter(params))?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// The plugin's history: every table with a `created_at` column, unix
    /// seconds or ISO text. Rustrak's retention clears them; the rest of the
    /// plugin's data it never touches. `expired` counts the rows older than
    /// `cutoff` (unix seconds), none without one.
    pub fn history(&self, cutoff: Option<i64>) -> Result<Vec<HistoryTable>, SqlError> {
        self.with_deadline(HOUSEKEEPING, |conn| {
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
        self.with_deadline(HOUSEKEEPING, |conn| {
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
        budget: Duration,
        f: impl FnOnce(&Connection) -> Result<T, SqlError>,
    ) -> Result<T, SqlError> {
        let conn = self.conn.lock().expect("plugin db lock poisoned");
        let deadline = Instant::now() + budget;
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
