//! The plans of the read paths over `events` that once scanned a project's
//! every event. SQLite only: Postgres plans by its statistics, which an
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

/// `since_utc` / `compare_utc` on SQLite.
const SINCE: &str = "ingested_at >= strftime('%Y-%m-%dT%H:%M:%S', 'now', '-24 hours')";

#[tokio::test]
async fn event_read_paths_are_answered_from_one_index() {
    let db = TestDb::new().await;
    let release_index = "USING COVERING INDEX idx_events_project_release";
    let stats_index = "USING COVERING INDEX idx_events_project_type_ingested";
    let cases = [
        // Deleting a grouping or an issue cascades to its events by
        // `grouping_id`, which had no index: one full scan per delete.
        (
            "SELECT id FROM events WHERE grouping_id = $1".to_string(),
            "USING INDEX idx_events_grouping".to_string(),
        ),
        // The issue list in a release (IssueService::list).
        (
            "SELECT id FROM issues WHERE project_id = $1 AND id IN (SELECT issue_id FROM events \
             WHERE project_id = $1 AND release = $2 AND environment = $3)"
                .to_string(),
            release_index.to_string(),
        ),
        (
            "SELECT id FROM issues WHERE project_id = $1 AND id IN (SELECT issue_id FROM events \
             WHERE project_id = $1 AND release = $2)"
                .to_string(),
            release_index.to_string(),
        ),
        // Project stats narrowed to a release (stats::ISSUE_IN_SCOPE).
        (
            "SELECT issue_id FROM events WHERE project_id = $1 AND release = $3 \
             AND ($2 IS NULL OR environment = $2)"
                .to_string(),
            release_index.to_string(),
        ),
        // The releases page (ReleaseService::summaries).
        (
            format!(
                "SELECT release, MIN(ingested_at), MAX(ingested_at), \
                 COUNT(CASE WHEN {SINCE} AND ($2 IS NULL OR environment = $2) THEN 1 END) \
                 FROM events WHERE project_id = $1 AND release <> '' GROUP BY release"
            ),
            release_index.to_string(),
        ),
        (
            "SELECT release, environment FROM events \
             WHERE project_id = $1 AND release <> '' AND environment <> '' \
             GROUP BY release, environment"
                .to_string(),
            release_index.to_string(),
        ),
        // `events/stats` (StatsService::timeseries).
        (
            format!(
                "SELECT (CAST(strftime('%s', ingested_at) AS INTEGER) / 3600 * 3600) AS bucket, \
                 COUNT(*), SUM(CASE WHEN COALESCE(level, '') = 'fatal' THEN 1 ELSE 0 END) \
                 FROM events WHERE project_id = $1 AND event_type = 'error' \
                 AND ($2 IS NULL OR environment = $2) AND ($3 IS NULL OR release = $3) \
                 AND {SINCE} GROUP BY 1 ORDER BY 1 ASC"
            ),
            format!("{stats_index} (project_id=? AND event_type=? AND ingested_at>?)"),
        ),
        // The headline counts (stats::count_events).
        (
            "SELECT COUNT(*) FROM events WHERE project_id = $1 \
             AND ($2 IS NULL OR environment = $2) AND ($3 IS NULL OR release = $3) \
             AND event_type = 'error' \
             AND ingested_at >= strftime('%Y-%m-%dT%H:%M:%S', $5)"
                .to_string(),
            format!("{stats_index} (project_id=? AND event_type=? AND ingested_at>?)"),
        ),
        // The projects list (stats::query_list_events).
        (
            "SELECT project_id, COUNT(*), COUNT(DISTINCT issue_id) FROM events \
             WHERE project_id IN (1, 2) AND event_type = 'error' \
             AND ingested_at >= strftime('%Y-%m-%dT%H:%M:%S', $1) GROUP BY 1"
                .to_string(),
            format!("{stats_index} (project_id=? AND event_type=? AND ingested_at>?)"),
        ),
        // An issue's series (IssueService::stats).
        (
            format!(
                "SELECT (CAST(strftime('%s', ingested_at) AS INTEGER) - 0) / 3600, COUNT(*) \
                 FROM events WHERE issue_id = $1 AND {SINCE} GROUP BY 1"
            ),
            "USING COVERING INDEX idx_events_issue_ingested (issue_id=? AND ingested_at>?)"
                .to_string(),
        ),
    ];
    for (sql, expected) in cases {
        let got = plan(&db.pool, &sql).await;
        assert!(
            got.contains(&expected),
            "{sql}\n  expected {expected}\n  got: {got}"
        );
    }
}

/// Indexes no query needs: each duplicates a UNIQUE constraint's own index,
/// is a prefix of another, or is read by nothing.
#[tokio::test]
async fn redundant_indexes_are_gone() {
    let db = TestDb::new().await;
    for name in [
        "idx_projects_slug",
        "idx_projects_sentry_key",
        "idx_users_email",
        "idx_auth_tokens_token",
        "idx_sfm_lookup",
        "idx_events_project_digested",
        "idx_events_event_type",
    ] {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = $1")
                .bind(name)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(n, 0, "{name} is still there");
    }
}
