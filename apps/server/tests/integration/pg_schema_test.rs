//! PostgreSQL schema rules the queries rely on.

use crate::common::TestDb;

/// #329: sqlx binds a Rust string as `text`. Against a `CHAR(n)` column
/// Postgres casts the column, not the parameter, so the column's index cannot
/// be searched: every session lookup and every grouping lookup filtered whole
/// index ranges. No `CHAR(n)` column may come back.
#[actix_web::test]
async fn no_column_is_char() {
    let db = TestDb::new().await;
    let char_columns: Vec<String> = sqlx::query_scalar(
        "SELECT table_name || '.' || column_name FROM information_schema.columns \
         WHERE table_schema = 'public' AND data_type = 'character' ORDER BY 1",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert!(char_columns.is_empty(), "CHAR columns: {char_columns:?}");
}

/// The lookups run on every authenticated request and every event: with a
/// `text` parameter, as sqlx sends it, their unique index is searched.
#[actix_web::test]
async fn hash_lookups_search_their_index() {
    let db = TestDb::new().await;
    let mut conn = db.pool.acquire().await.unwrap();
    sqlx::query("SET enable_seqscan = off")
        .execute(&mut *conn)
        .await
        .unwrap();
    for (sql, column) in [
        (
            "EXPLAIN SELECT 1 FROM sessions WHERE id_hash = $1",
            "id_hash",
        ),
        (
            "EXPLAIN SELECT 1 FROM groupings WHERE project_id = 1 AND grouping_key_hash = $1",
            "grouping_key_hash",
        ),
        (
            "EXPLAIN SELECT 1 FROM source_file WHERE checksum = $1",
            "checksum",
        ),
    ] {
        let plan: Vec<String> = sqlx::query_scalar(sql)
            .bind("0".repeat(64))
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        let plan = plan.join("\n");
        let searched = plan
            .lines()
            .any(|line| line.contains("Index Cond") && line.contains(column));
        assert!(searched, "{sql}:\n{plan}");
    }
}
