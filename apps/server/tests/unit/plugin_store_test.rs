//! A plugin's own SQLite file: forward-only migrations, a quota, a time limit,
//! and SQL that cannot reach outside the file.

use std::time::{Duration, Instant};

use rustrak::plugins::package::Migration;
use rustrak::plugins::store::{PluginDb, SqlError, Value};
use sha2::{Digest, Sha256};

fn migration(version: u32, sql: &str) -> Migration {
    Migration {
        version,
        name: format!("m{version}"),
        sql: sql.to_string(),
        checksum: hex::encode(Sha256::digest(sql)),
    }
}

fn open(dir: &tempfile::TempDir) -> PluginDb {
    PluginDb::open(&dir.path().join("data.sqlite"), 16).expect("open")
}

const FAR: Duration = Duration::from_secs(5);

#[test]
fn applies_migrations_once_and_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    let ms = [
        migration(1, "CREATE TABLE hits (path TEXT NOT NULL);"),
        migration(2, "ALTER TABLE hits ADD COLUMN at INTEGER;"),
    ];
    db.migrate(&ms).unwrap();
    db.migrate(&ms).unwrap(); // a second run applies nothing

    db.execute(
        "INSERT INTO hits (path, at) VALUES (?1, ?2)",
        &[Value::Text("/".into()), Value::Integer(1)],
        FAR,
    )
    .unwrap();
    let rows = db.query("SELECT path, at FROM hits", &[], FAR).unwrap();
    assert_eq!(rows.columns, ["path", "at"]);
    assert_eq!(rows.rows, [[Value::Text("/".into()), Value::Integer(1)]]);
}

#[test]
fn refuses_an_applied_migration_that_was_edited() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    db.migrate(&[migration(1, "CREATE TABLE a (x);")]).unwrap();
    let err = db
        .migrate(&[migration(1, "CREATE TABLE a (y);")])
        .unwrap_err();
    assert!(err.to_string().contains("0001"), "{err}");
}

#[test]
fn a_failing_migration_leaves_the_file_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    db.migrate(&[migration(1, "CREATE TABLE a (x);")]).unwrap();
    let err = db
        .migrate(&[
            migration(1, "CREATE TABLE a (x);"),
            migration(2, "CREATE TABLE b (x);"),
            migration(3, "THIS IS NOT SQL;"),
        ])
        .unwrap_err();
    assert!(err.to_string().contains("0003"), "{err}");
    // 0002 was applied in its own step, but the whole upgrade is undone.
    let tables = db
        .query(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'b'",
            &[],
            FAR,
        )
        .unwrap();
    assert!(
        tables.rows.is_empty(),
        "b should not exist after a failed upgrade"
    );
}

#[test]
fn plugin_sql_cannot_attach_another_file_or_change_pragmas() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    let other = dir.path().join("other.sqlite");
    let attach = format!("ATTACH DATABASE '{}' AS other", other.display());
    assert!(matches!(
        db.execute(&attach, &[], FAR),
        Err(SqlError::Other(_))
    ));
    assert!(!other.exists());
    assert!(db
        .execute("PRAGMA max_page_count = 1000000", &[], FAR)
        .is_err());
    assert!(db
        .execute("DELETE FROM _rustrak_migrations", &[], FAR)
        .is_err());
}

#[test]
fn a_full_quota_is_reported_as_such() {
    let dir = tempfile::tempdir().unwrap();
    let db = PluginDb::open(&dir.path().join("data.sqlite"), 1).unwrap(); // 1 MB
    db.migrate(&[migration(1, "CREATE TABLE blobs (b BLOB);")])
        .unwrap();
    let big = Value::Blob(vec![0u8; 256 * 1024]);
    let mut result = Ok(0);
    for _ in 0..10 {
        result = db.execute(
            "INSERT INTO blobs VALUES (?1)",
            std::slice::from_ref(&big),
            FAR,
        );
        if result.is_err() {
            break;
        }
    }
    assert!(matches!(result, Err(SqlError::DatabaseFull)), "{result:?}");
}

#[test]
fn a_runaway_query_is_interrupted_at_its_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    let started = Instant::now();
    let endless =
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c";
    let result = db.query(endless, &[], Duration::from_millis(100));
    assert!(result.is_err());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn batch_is_all_or_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    db.migrate(&[migration(1, "CREATE TABLE t (x INTEGER NOT NULL);")])
        .unwrap();
    let err = db.batch(
        &[
            ("INSERT INTO t VALUES (1)".into(), vec![]),
            ("INSERT INTO t VALUES (NULL)".into(), vec![]),
        ],
        FAR,
    );
    assert!(matches!(err, Err(SqlError::Constraint(_))), "{err:?}");
    let rows = db.query("SELECT count(*) FROM t", &[], FAR).unwrap();
    assert_eq!(rows.rows[0][0], Value::Integer(0));
}

// Retention: a table with `created_at` is history, cleared by Rustrak with
// the window the admin sets; the rest is never touched.

fn with_history(dir: &tempfile::TempDir) -> PluginDb {
    let db = open(dir);
    db.migrate(&[migration(
        1,
        "CREATE TABLE hits (path TEXT, created_at INTEGER NOT NULL);
         CREATE TABLE notes (body TEXT, created_at TEXT NOT NULL);
         CREATE TABLE daily (day TEXT PRIMARY KEY, hits INTEGER);",
    )])
    .unwrap();
    let now = 1_800_000_000_i64;
    for (sql, params) in [
        (
            "INSERT INTO hits VALUES ('/old', ?1)",
            vec![Value::Integer(now - 40 * 86_400)],
        ),
        (
            "INSERT INTO hits VALUES ('/new', ?1)",
            vec![Value::Integer(now - 86_400)],
        ),
        (
            "INSERT INTO notes VALUES ('old', '2026-01-01 10:00:00')",
            vec![],
        ),
        (
            "INSERT INTO notes VALUES ('new', '2027-01-14T10:00:00Z')",
            vec![],
        ),
        ("INSERT INTO daily VALUES ('2026-01-01', 9)", vec![]),
    ] {
        db.execute(sql, &params, FAR).unwrap();
    }
    db
}

#[test]
fn history_is_every_table_with_created_at() {
    let dir = tempfile::tempdir().unwrap();
    let db = with_history(&dir);
    // 2027-01-15 08:00 UTC, minus 30 days.
    let cutoff = 1_800_000_000 - 30 * 86_400;
    let history = db.history(Some(cutoff)).unwrap();
    let seen: Vec<_> = history
        .iter()
        .map(|t| (t.table.as_str(), t.rows, t.expired))
        .collect();
    assert_eq!(
        seen,
        [("hits", 2, 1), ("notes", 2, 1)],
        "daily has no created_at"
    );
    assert_eq!(db.history(None).unwrap()[0].expired, 0, "kept forever");
}

#[test]
fn forgetting_clears_only_rows_older_than_the_cutoff() {
    let dir = tempfile::tempdir().unwrap();
    let db = with_history(&dir);
    let removed = db.forget_before(1_800_000_000 - 30 * 86_400).unwrap();
    assert_eq!(removed, 2);
    let left = |sql: &str| match &db.query(sql, &[], FAR).unwrap().rows[0][0] {
        Value::Integer(n) => *n,
        other => panic!("{other:?}"),
    };
    assert_eq!(left("SELECT count(*) FROM hits"), 1);
    assert_eq!(left("SELECT count(*) FROM notes"), 1);
    assert_eq!(left("SELECT count(*) FROM daily"), 1);
}

#[test]
fn a_transaction_the_plugin_leaves_open_is_rolled_back() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir);
    db.migrate(&[migration(1, "CREATE TABLE hits (created_at INTEGER);")])
        .unwrap();
    db.execute("BEGIN", &[], FAR).unwrap();
    db.execute("INSERT INTO hits VALUES (1)", &[], FAR).unwrap();
    // A call is its own unit: the BEGIN ends with it, so the insert after
    // it is committed (another connection sees it) and the host's own
    // transactions still run.
    let other = open(&dir);
    let rows = other.query("SELECT count(*) FROM hits", &[], FAR).unwrap();
    assert_eq!(rows.rows, [[Value::Integer(1)]]);
    assert_eq!(db.forget_before(10).unwrap(), 1);
}
