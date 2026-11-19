//! The plans of the span and transaction reads that once read a project's
//! every row (#557). SQLite only: Postgres plans by its statistics, which an
//! empty test table does not have. The SQL is the services' own, with the
//! dialect helpers written out.

#![cfg(feature = "sqlite")]

use crate::common::TestDb;

async fn plan(pool: &rustrak::db::DbPool, sql: &str) -> String {
    let rows: Vec<(i64, i64, i64, String)> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}")))
            .fetch_all(pool)
            .await
            .unwrap();
    rows.into_iter()
        .map(|r| r.3)
        .collect::<Vec<_>>()
        .join(" | ")
}

/// `since_utc` on SQLite.
fn since(column: &str) -> String {
    format!("{column} >= strftime('%Y-%m-%dT%H:%M:%S', 'now', '-24 hours')")
}

#[tokio::test]
async fn the_span_list_reads_its_page_in_order() {
    // SpanService::list_offset: newest first without sorting the project's
    // spans. Only the tie on `id` is sorted, a few rows at a time.
    let db = TestDb::new().await;
    for filter in ["", &format!(" AND {}", since("start_timestamp"))] {
        let got = plan(
            &db.pool,
            &format!(
                "SELECT id FROM spans WHERE project_id = $1{filter} \
                 ORDER BY start_timestamp DESC, id DESC LIMIT 20"
            ),
        )
        .await;
        assert!(got.contains("USING INDEX idx_spans_project_start"), "{got}");
        assert!(!got.contains("TEMP B-TREE FOR ORDER BY"), "{got}");
    }
}

#[tokio::test]
async fn the_spans_of_a_trace_are_read_by_trace() {
    // The same list narrowed to a trace: its spans are few, and with no
    // statistics SQLite would rather walk the project's spans in order than
    // sort them, so the service names the index (`SpanFilters::from`).
    let db = TestDb::new().await;
    for sql in [
        "SELECT id FROM spans INDEXED BY idx_spans_project_trace \
         WHERE project_id = $1 AND trace_id = $2 \
         ORDER BY start_timestamp DESC, id DESC LIMIT 20",
        "SELECT COUNT(*) FROM (SELECT 1 FROM spans INDEXED BY idx_spans_project_trace \
         WHERE project_id = $1 AND trace_id = $2 LIMIT 1001) AS hits",
    ] {
        let got = plan(&db.pool, sql).await;
        assert!(
            got.contains("idx_spans_project_trace (project_id=? AND trace_id=?)"),
            "{got}"
        );
    }
}

#[tokio::test]
async fn transaction_reads_are_answered_from_one_index() {
    let db = TestDb::new().await;
    let window = since("timestamp");
    let group = "transaction_name = $2 AND (op = $3 OR (op IS NULL AND $4 IS NULL))";
    let cases = [
        // TransactionService::stats and ::totals.
        format!(
            "SELECT transaction_name, op, COUNT(*), SUM(duration_ms), MAX(timestamp), \
             SUM(CASE WHEN status IS NOT NULL AND status <> 'ok' THEN 1 ELSE 0 END) \
             FROM transactions WHERE project_id = $1 AND duration_ms IS NOT NULL \
             AND ($2 IS NULL OR environment = $3) AND {window} \
             GROUP BY transaction_name, op"
        ),
        // ::group_durations, the SQLite percentiles of one endpoint.
        format!(
            "SELECT duration_ms FROM transactions WHERE project_id = $1 \
             AND duration_ms IS NOT NULL AND {window} AND {group}"
        ),
        // ::group_timeseries.
        format!(
            "SELECT timestamp, duration_ms, status FROM transactions WHERE project_id = $1 \
             AND {group} AND duration_ms IS NOT NULL AND ($5 IS NULL OR environment = $6) \
             AND {window}"
        ),
        // ::list_offset, the ids of an endpoint's slowest samples and their
        // count.
        format!(
            "SELECT id FROM transactions WHERE project_id = $1 AND transaction_name = $2 \
             AND op = $3 AND {window} \
             ORDER BY (duration_ms IS NULL), duration_ms DESC, id DESC LIMIT 10"
        ),
        format!(
            "SELECT COUNT(*) FROM (SELECT 1 FROM transactions WHERE project_id = $1 \
             AND transaction_name = $2 AND op = $3 AND {window} LIMIT 1001) AS hits"
        ),
    ];
    for sql in cases {
        let got = plan(&db.pool, &sql).await;
        assert!(
            got.contains("USING COVERING INDEX idx_transactions_group_window"),
            "{sql}\n  -> {got}"
        );
    }
}

/// Both were prefixes of idx_transactions_group_window.
#[tokio::test]
async fn the_indexes_it_replaces_are_gone() {
    let db = TestDb::new().await;
    for name in [
        "idx_transactions_group_duration",
        "idx_transactions_project_name",
    ] {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = $1")
                .bind(name)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(n, 0, "{name}");
    }
}
