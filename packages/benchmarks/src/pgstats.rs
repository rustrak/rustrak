//! PostgreSQL-side metrics collection.
//!
//! Container CPU/memory tells you what the database *cost*; it does not tell you
//! what the engine actually *did*. For comparing major versions (16 vs 18) the
//! interesting signal is in the `pg_stat_*` views: buffer hit ratio, WAL volume,
//! checkpoint behaviour, and — new and much richer in 18 — per-context I/O.
//!
//! Views are snapshotted before and after a run and reported as deltas.
//!
//! ## Why `row_to_json` instead of typed structs
//!
//! The shape of these views is *not* stable across major versions. PG17 moved
//! checkpoint counters out of `pg_stat_bgwriter` into `pg_stat_checkpointer`;
//! PG18 removed the `wal_write`/`wal_sync` timing columns from `pg_stat_wal`
//! (that data now lives in `pg_stat_io`). Any hand-written struct would have to
//! be forked per version, and would silently drop whatever the newer engine
//! added — exactly the columns a version comparison exists to look at.
//!
//! Selecting `row_to_json(t)` instead means the collector captures whatever the
//! server offers, and deltas are computed generically over the numeric fields.
//! Columns present in only one version simply appear in only that snapshot.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use thiserror::Error;
use tokio_postgres::{Client, NoTls};

/// PostgreSQL stats collection errors
#[derive(Debug, Error)]
pub enum PgStatsError {
    #[error("PostgreSQL connection failed: {0}")]
    Connection(#[from] tokio_postgres::Error),
}

/// Identifying information about the server under test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgServerInfo {
    /// Full version string, e.g. "PostgreSQL 18.1 on aarch64-unknown-linux-musl…"
    pub version: String,
    /// Numeric version, e.g. 180001
    pub version_num: i32,
    /// Major version, e.g. 18
    pub major_version: i32,
    /// Selected settings that materially affect performance, captured so a
    /// result file is self-describing and two runs can be proven comparable.
    pub settings: BTreeMap<String, String>,
}

/// Settings worth recording alongside every run.
///
/// `io_method` and `io_workers` only exist on PG18+ (asynchronous I/O); they are
/// requested unconditionally and simply come back absent on older engines.
const TRACKED_SETTINGS: &[&str] = &[
    "shared_buffers",
    "work_mem",
    "maintenance_work_mem",
    "max_connections",
    "synchronous_commit",
    "wal_buffers",
    "wal_compression",
    "checkpoint_timeout",
    "checkpoint_completion_target",
    "max_wal_size",
    "effective_cache_size",
    "random_page_cost",
    "track_io_timing",
    "autovacuum",
    "max_parallel_workers",
    "max_parallel_workers_per_gather",
    "jit",
    // PG18+ asynchronous I/O
    "io_method",
    "io_workers",
    "io_combine_limit",
];

/// The `pg_stat_*` views sampled on every snapshot.
///
/// Each entry is (label, SQL). Every query must return at most one row of
/// aggregated counters. Views absent on a given major version are tolerated —
/// a failing query is recorded as absent rather than aborting the run.
fn snapshot_queries() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "database",
            "SELECT row_to_json(t) FROM (
                 SELECT * FROM pg_stat_database WHERE datname = current_database()
             ) t",
        ),
        (
            "bgwriter",
            "SELECT row_to_json(t) FROM (SELECT * FROM pg_stat_bgwriter) t",
        ),
        // PG17+ only — checkpoint counters moved out of pg_stat_bgwriter.
        (
            "checkpointer",
            "SELECT row_to_json(t) FROM (SELECT * FROM pg_stat_checkpointer) t",
        ),
        (
            "wal",
            "SELECT row_to_json(t) FROM (SELECT * FROM pg_stat_wal) t",
        ),
        // pg_stat_io exists in PG16+ but gains read/write-time detail and new
        // contexts in PG18. Aggregated across backend types so the shape is
        // comparable; per-context detail is captured separately below.
        (
            "io_total",
            "SELECT row_to_json(t) FROM (
                 SELECT sum(reads)::bigint          AS reads,
                        sum(writes)::bigint         AS writes,
                        sum(writebacks)::bigint     AS writebacks,
                        sum(extends)::bigint        AS extends,
                        sum(hits)::bigint           AS hits,
                        sum(evictions)::bigint      AS evictions,
                        sum(fsyncs)::bigint         AS fsyncs,
                        sum(read_time)::double precision  AS read_time,
                        sum(write_time)::double precision AS write_time,
                        sum(fsync_time)::double precision AS fsync_time
                 FROM pg_stat_io
             ) t",
        ),
        // Table-level activity for the hot ingestion tables.
        (
            "user_tables",
            "SELECT row_to_json(t) FROM (
                 SELECT sum(seq_scan)::bigint        AS seq_scan,
                        sum(seq_tup_read)::bigint    AS seq_tup_read,
                        sum(idx_scan)::bigint        AS idx_scan,
                        sum(idx_tup_fetch)::bigint   AS idx_tup_fetch,
                        sum(n_tup_ins)::bigint       AS n_tup_ins,
                        sum(n_tup_upd)::bigint       AS n_tup_upd,
                        sum(n_tup_del)::bigint       AS n_tup_del,
                        sum(n_tup_hot_upd)::bigint   AS n_tup_hot_upd,
                        sum(n_live_tup)::bigint      AS n_live_tup,
                        sum(n_dead_tup)::bigint      AS n_dead_tup,
                        sum(vacuum_count)::bigint    AS vacuum_count,
                        sum(autovacuum_count)::bigint AS autovacuum_count,
                        sum(analyze_count)::bigint   AS analyze_count,
                        sum(autoanalyze_count)::bigint AS autoanalyze_count
                 FROM pg_stat_user_tables
             ) t",
        ),
        (
            "statio_user_tables",
            "SELECT row_to_json(t) FROM (
                 SELECT sum(heap_blks_read)::bigint  AS heap_blks_read,
                        sum(heap_blks_hit)::bigint   AS heap_blks_hit,
                        sum(idx_blks_read)::bigint   AS idx_blks_read,
                        sum(idx_blks_hit)::bigint    AS idx_blks_hit,
                        sum(toast_blks_read)::bigint AS toast_blks_read,
                        sum(toast_blks_hit)::bigint  AS toast_blks_hit
                 FROM pg_statio_user_tables
             ) t",
        ),
        // WAL position, so total WAL generated can be derived independently of
        // pg_stat_wal (whose columns shift between versions).
        (
            "wal_lsn",
            "SELECT row_to_json(t) FROM (
                 SELECT pg_current_wal_lsn() - '0/0'::pg_lsn AS wal_bytes_total
             ) t",
        ),
    ]
}

/// A point-in-time capture of every sampled view.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PgSnapshot {
    /// view label -> the single JSON row that view returned
    pub views: BTreeMap<String, Value>,
}

/// Per-table size and row counts, captured once at the end of a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSize {
    pub table_name: String,
    pub row_estimate: i64,
    pub total_bytes: i64,
    pub table_bytes: i64,
    pub index_bytes: i64,
    pub toast_bytes: i64,
}

/// The deltas between two snapshots, plus derived ratios.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PgStatsDelta {
    /// view label -> { column -> delta }
    pub views: BTreeMap<String, BTreeMap<String, f64>>,
    /// Derived, human-meaningful figures
    pub derived: BTreeMap<String, f64>,
}

/// Everything the PostgreSQL side contributes to a benchmark result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgReport {
    pub server: PgServerInfo,
    pub delta: PgStatsDelta,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub table_sizes: Vec<TableSize>,
    /// Total bytes on disk for the benchmark database at end of run
    pub database_bytes: i64,
}

/// Collects PostgreSQL statistics across a benchmark run.
pub struct PgStatsCollector {
    client: Client,
    server: PgServerInfo,
    start: Option<PgSnapshot>,
}

impl PgStatsCollector {
    /// Connect to the benchmark database.
    ///
    /// `conn_str` is a standard libpq connection string, e.g.
    /// `postgres://bench:bench@localhost:55432/rustrak_bench`.
    pub async fn connect(conn_str: &str) -> Result<Self, PgStatsError> {
        let (client, connection) = tokio_postgres::connect(conn_str, NoTls).await?;

        // The connection future drives the protocol and must be polled for the
        // client to work; it completes when the client is dropped.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                log_warn(&format!("postgres connection error: {}", e));
            }
        });

        let server = Self::read_server_info(&client).await?;

        Ok(Self {
            client,
            server,
            start: None,
        })
    }

    /// Information about the connected server.
    pub fn server(&self) -> &PgServerInfo {
        &self.server
    }

    async fn read_server_info(client: &Client) -> Result<PgServerInfo, PgStatsError> {
        let row = client
            .query_one(
                "SELECT version(), current_setting('server_version_num')::int",
                &[],
            )
            .await?;
        let version: String = row.get(0);
        let version_num: i32 = row.get(1);

        let mut settings = BTreeMap::new();
        for name in TRACKED_SETTINGS {
            // current_setting(name, missing_ok := true) returns NULL rather than
            // erroring for GUCs that do not exist on this major version.
            if let Ok(row) = client
                .query_one("SELECT current_setting($1, true)", &[name])
                .await
            {
                let value: Option<String> = row.get(0);
                if let Some(value) = value {
                    settings.insert((*name).to_string(), value);
                }
            }
        }

        Ok(PgServerInfo {
            version,
            version_num,
            major_version: version_num / 10_000,
            settings,
        })
    }

    /// Take a snapshot of all sampled views.
    pub async fn snapshot(&self) -> PgSnapshot {
        let mut views = BTreeMap::new();

        for (label, sql) in snapshot_queries() {
            match self.client.query_opt(sql, &[]).await {
                Ok(Some(row)) => {
                    let value: Option<Value> = row.get(0);
                    if let Some(value) = value {
                        views.insert(label.to_string(), value);
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    // View absent on this major version (e.g. pg_stat_checkpointer
                    // before 17). Not an error — just nothing to record.
                }
            }
        }

        PgSnapshot { views }
    }

    /// Mark the start of a measured window.
    pub async fn begin(&mut self) {
        self.start = Some(self.snapshot().await);
    }

    /// Close the measured window and build the report.
    pub async fn finish(&self) -> PgReport {
        let end = self.snapshot().await;
        let start = self.start.clone().unwrap_or_default();

        let delta = diff_snapshots(&start, &end);
        let table_sizes = self.table_sizes().await.unwrap_or_default();
        let database_bytes = self.database_size().await.unwrap_or(0);

        PgReport {
            server: self.server.clone(),
            delta,
            table_sizes,
            database_bytes,
        }
    }

    /// Per-table sizes, largest first.
    pub async fn table_sizes(&self) -> Result<Vec<TableSize>, PgStatsError> {
        let rows = self
            .client
            .query(
                "SELECT c.relname::text,
                        c.reltuples::bigint,
                        pg_total_relation_size(c.oid)::bigint,
                        pg_table_size(c.oid)::bigint,
                        pg_indexes_size(c.oid)::bigint,
                        COALESCE(pg_total_relation_size(c.reltoastrelid), 0)::bigint
                 FROM pg_class c
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 WHERE c.relkind = 'r' AND n.nspname = 'public'
                 ORDER BY pg_total_relation_size(c.oid) DESC
                 LIMIT 15",
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| TableSize {
                table_name: row.get(0),
                row_estimate: row.get(1),
                total_bytes: row.get(2),
                table_bytes: row.get(3),
                index_bytes: row.get(4),
                toast_bytes: row.get(5),
            })
            .collect())
    }

    /// Total on-disk size of the benchmark database.
    pub async fn database_size(&self) -> Result<i64, PgStatsError> {
        let row = self
            .client
            .query_one("SELECT pg_database_size(current_database())::bigint", &[])
            .await?;
        Ok(row.get(0))
    }

    /// Number of rows currently in a table — used to observe digest progress.
    ///
    /// This is an exact `count(*)`, not a `reltuples` estimate: the drain
    /// scenario needs to know precisely when the backlog reaches zero, and
    /// `reltuples` is only refreshed by vacuum/analyze.
    pub async fn count_rows(
        &self,
        table: &str,
        project_id: Option<u32>,
    ) -> Result<i64, PgStatsError> {
        // `table` is not user input — it comes from a fixed set in the runner —
        // but it is still validated rather than interpolated blindly.
        let sql = match project_id {
            Some(pid) => format!(
                "SELECT count(*)::bigint FROM {} WHERE project_id = {}",
                sanitize_ident(table),
                pid
            ),
            None => format!("SELECT count(*)::bigint FROM {}", sanitize_ident(table)),
        };
        let row = self.client.query_one(sql.as_str(), &[]).await?;
        Ok(row.get(0))
    }

    /// Run ANALYZE so planner statistics are equivalent before read benchmarks.
    ///
    /// Without this the read comparison can measure "one engine happened to have
    /// autovacuumed" rather than the engine itself.
    pub async fn analyze(&self) -> Result<(), PgStatsError> {
        self.client.batch_execute("ANALYZE").await?;
        Ok(())
    }

    /// Force a checkpoint so buffered writes are flushed before measuring.
    pub async fn checkpoint(&self) -> Result<(), PgStatsError> {
        self.client.batch_execute("CHECKPOINT").await?;
        Ok(())
    }
}

/// Reject anything that is not a plain lowercase identifier.
fn sanitize_ident(ident: &str) -> String {
    ident
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

/// Extract the numeric fields of a JSON object as f64.
///
/// PostgreSQL renders `bigint` counters as JSON numbers and `interval`/`text`
/// columns as strings; only the numeric ones can be differenced, so the rest are
/// dropped. Timestamps (`stats_reset`) fall out here too, which is correct — a
/// delta of a reset time is meaningless.
fn numeric_fields(value: &Value) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    if let Value::Object(map) = value {
        collect_numeric(map, &mut out);
    }
    out
}

fn collect_numeric(map: &Map<String, Value>, out: &mut BTreeMap<String, f64>) {
    for (key, value) in map {
        match value {
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    out.insert(key.clone(), f);
                }
            }
            // Numeric/bigint values can arrive as strings when they exceed the
            // range JSON numbers are guaranteed to carry (pg_lsn arithmetic in
            // particular yields `numeric`).
            Value::String(s) => {
                if let Ok(f) = s.parse::<f64>() {
                    out.insert(key.clone(), f);
                }
            }
            _ => {}
        }
    }
}

/// Compute end-minus-start for every numeric column of every view.
pub fn diff_snapshots(start: &PgSnapshot, end: &PgSnapshot) -> PgStatsDelta {
    let mut views: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();

    for (label, end_value) in &end.views {
        let end_fields = numeric_fields(end_value);
        let start_fields = start
            .views
            .get(label)
            .map(numeric_fields)
            .unwrap_or_default();

        let mut deltas = BTreeMap::new();
        for (key, end_num) in end_fields {
            // A column missing from the start snapshot means stats were reset
            // mid-run or the view appeared late; treat the start as zero rather
            // than dropping the column entirely.
            let start_num = start_fields.get(&key).copied().unwrap_or(0.0);
            deltas.insert(key, end_num - start_num);
        }

        if !deltas.is_empty() {
            views.insert(label.clone(), deltas);
        }
    }

    let derived = derive_ratios(&views);

    PgStatsDelta { views, derived }
}

/// Turn raw counters into the figures a reader actually compares.
fn derive_ratios(views: &BTreeMap<String, BTreeMap<String, f64>>) -> BTreeMap<String, f64> {
    let mut derived = BTreeMap::new();

    let get = |view: &str, key: &str| -> Option<f64> { views.get(view)?.get(key).copied() };

    // Shared-buffer hit ratio: the single most telling number for whether extra
    // work reached the disk.
    if let (Some(hit), Some(read)) = (get("database", "blks_hit"), get("database", "blks_read")) {
        let total = hit + read;
        if total > 0.0 {
            derived.insert("cache_hit_ratio".to_string(), hit / total * 100.0);
        }
    }

    // WAL generated over the window, derived from LSN advance so it is
    // comparable across versions regardless of pg_stat_wal's column churn.
    if let Some(wal_bytes) = get("wal_lsn", "wal_bytes_total") {
        derived.insert("wal_bytes".to_string(), wal_bytes);
        derived.insert("wal_mb".to_string(), wal_bytes / (1024.0 * 1024.0));
    }

    // Transaction throughput over the window.
    if let (Some(commits), Some(rollbacks)) = (
        get("database", "xact_commit"),
        get("database", "xact_rollback"),
    ) {
        derived.insert("transactions".to_string(), commits + rollbacks);
        if commits + rollbacks > 0.0 {
            derived.insert(
                "rollback_ratio".to_string(),
                rollbacks / (commits + rollbacks) * 100.0,
            );
        }
    }

    // Time the engine spent blocked on I/O (requires track_io_timing=on).
    if let (Some(read_time), Some(write_time)) = (
        get("database", "blk_read_time"),
        get("database", "blk_write_time"),
    ) {
        derived.insert("blk_io_time_ms".to_string(), read_time + write_time);
    }

    // Index vs sequential access mix — a planner/statistics difference between
    // versions shows up here before it shows up in latency.
    if let (Some(seq), Some(idx)) = (
        get("user_tables", "seq_scan"),
        get("user_tables", "idx_scan"),
    ) {
        let total = seq + idx;
        if total > 0.0 {
            derived.insert("idx_scan_ratio".to_string(), idx / total * 100.0);
        }
    }

    // Temp file spill: work_mem pressure, and a common source of version drift.
    if let Some(temp_bytes) = get("database", "temp_bytes") {
        derived.insert("temp_mb".to_string(), temp_bytes / (1024.0 * 1024.0));
    }

    derived
}

fn log_warn(msg: &str) {
    eprintln!("warning: {}", msg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numeric_fields_keeps_numbers_and_numeric_strings() {
        let value = json!({
            "blks_hit": 100,
            "blks_read": 5,
            "datname": "rustrak_bench",
            "stats_reset": "2026-07-20T00:00:00Z",
            "wal_bytes_total": "123456789012345",
        });

        let fields = numeric_fields(&value);

        assert_eq!(fields.get("blks_hit"), Some(&100.0));
        assert_eq!(fields.get("blks_read"), Some(&5.0));
        assert_eq!(fields.get("wal_bytes_total"), Some(&123456789012345.0));
        // Non-numeric strings are dropped, not coerced to zero.
        assert!(!fields.contains_key("datname"));
        assert!(!fields.contains_key("stats_reset"));
    }

    #[test]
    fn diff_computes_per_column_deltas() {
        let start = PgSnapshot {
            views: [(
                "database".to_string(),
                json!({ "blks_hit": 100, "blks_read": 10 }),
            )]
            .into_iter()
            .collect(),
        };
        let end = PgSnapshot {
            views: [(
                "database".to_string(),
                json!({ "blks_hit": 900, "blks_read": 110 }),
            )]
            .into_iter()
            .collect(),
        };

        let delta = diff_snapshots(&start, &end);

        let db = delta.views.get("database").unwrap();
        assert_eq!(db.get("blks_hit"), Some(&800.0));
        assert_eq!(db.get("blks_read"), Some(&100.0));
        // 800 / 900 = 88.9%
        let ratio = delta.derived.get("cache_hit_ratio").unwrap();
        assert!((ratio - 88.888).abs() < 0.01, "got {}", ratio);
    }

    #[test]
    fn diff_treats_columns_new_in_end_as_starting_from_zero() {
        // Mirrors a column that only exists on the newer major version.
        let start = PgSnapshot {
            views: [("wal".to_string(), json!({ "wal_records": 10 }))]
                .into_iter()
                .collect(),
        };
        let end = PgSnapshot {
            views: [(
                "wal".to_string(),
                json!({ "wal_records": 50, "wal_buffers_full": 7 }),
            )]
            .into_iter()
            .collect(),
        };

        let delta = diff_snapshots(&start, &end);

        let wal = delta.views.get("wal").unwrap();
        assert_eq!(wal.get("wal_records"), Some(&40.0));
        assert_eq!(wal.get("wal_buffers_full"), Some(&7.0));
    }

    #[test]
    fn sanitize_ident_strips_injection_attempts() {
        assert_eq!(sanitize_ident("events"), "events");
        assert_eq!(
            sanitize_ident("events; DROP TABLE users"),
            "eventsDROPTABLEusers"
        );
        assert_eq!(sanitize_ident("issue_activity"), "issue_activity");
    }

    #[test]
    fn derive_ratios_handles_empty_input() {
        let derived = derive_ratios(&BTreeMap::new());
        assert!(derived.is_empty());
    }
}
