//! SQL fragments where the two backends' dialects differ, so a query is
//! written once.

use chrono::{DateTime, Utc};
use sqlx::QueryBuilder;

use crate::db::Db;
use crate::error::{AppError, AppResult};

/// `column` falls in the last `hours`, as a SQL condition; true without a
/// window. `hours` is a parsed integer, never request text.
pub(crate) fn since(column: &str, hours: Option<i64>) -> String {
    let Some(hours) = hours else {
        return "1 = 1".to_string();
    };
    #[cfg(feature = "postgres")]
    return format!("{column} >= NOW() - INTERVAL '{hours} hours'");
    #[cfg(not(feature = "postgres"))]
    return format!("datetime({column}) >= datetime('now', '-{hours} hours')");
}

/// The bound a SQLite UTC column is compared with: `datetime()`'s value cut
/// to whole seconds, `T`-separated as sqlx writes a `DateTime<Utc>`.
#[cfg(not(feature = "postgres"))]
const UTC_SECONDS: &str = "strftime('%Y-%m-%dT%H:%M:%S'";

/// [`since`] for a column only ever written from Rust as a `DateTime<Utc>`,
/// compared as stored so an index on it serves the range on SQLite.
///
/// sqlx writes those as RFC 3339 (`SecondsFormat::AutoSi`, `+00:00`), whose
/// text sorts as time does. The bound is cut to whole seconds, as
/// `datetime()` cut both sides, so `>=` keeps exactly the same rows.
/// Never for a column a SQL default fills (`datetime('now')` is
/// space-separated).
pub(crate) fn since_utc(column: &str, hours: Option<i64>) -> String {
    let Some(hours) = hours else {
        return "1 = 1".to_string();
    };
    #[cfg(feature = "postgres")]
    return format!("{column} >= NOW() - INTERVAL '{hours} hours'");
    #[cfg(not(feature = "postgres"))]
    return format!("{column} >= {UTC_SECONDS}, 'now', '-{hours} hours')");
}

/// [`compare_time`] for a column [`since_utc`] can compare as stored. Only
/// `>=` and `<`: with the bound cut to whole seconds those two are the
/// comparisons `datetime()` made.
pub(crate) fn compare_utc(column: &str, op: &str, param: usize) -> String {
    debug_assert!(matches!(op, ">=" | "<"), "{op}");
    #[cfg(feature = "postgres")]
    return format!("{column} {op} ${param}");
    #[cfg(not(feature = "postgres"))]
    return format!("{column} {op} {UTC_SECONDS}, ${param})");
}

/// [`compare_utc`] for a [`QueryBuilder`], which numbers its own binds.
pub(crate) fn push_compare_utc(
    qb: &mut QueryBuilder<Db>,
    column: &str,
    op: &str,
    value: DateTime<Utc>,
) {
    debug_assert!(matches!(op, ">=" | "<"), "{op}");
    #[cfg(feature = "postgres")]
    qb.push(format!("{column} {op} "));
    #[cfg(not(feature = "postgres"))]
    qb.push(format!("{column} {op} {UTC_SECONDS}, "));
    qb.push_bind(value);
    #[cfg(not(feature = "postgres"))]
    qb.push(")");
}

/// [`since`] comparing the stored value as it is, so an index on `column`
/// still serves the range on SQLite. Only for a column stored the way
/// `datetime()` writes it (`YYYY-MM-DD HH:MM:SS`, e.g. `session_counts.bucket`):
/// a `T`-separated value sorts after every bound of the same day. Those
/// take [`since_utc`].
pub(crate) fn since_stored(column: &str, hours: Option<i64>) -> String {
    let Some(hours) = hours else {
        return "1 = 1".to_string();
    };
    #[cfg(feature = "postgres")]
    return format!("{column} >= NOW() - INTERVAL '{hours} hours'");
    #[cfg(not(feature = "postgres"))]
    return format!("{column} >= datetime('now', '-{hours} hours')");
}

/// A timestamp column as whole seconds since the epoch, as SQL, rounded
/// down on both backends (Postgres' `CAST` alone would round to nearest).
pub(crate) fn epoch(column: &str) -> String {
    #[cfg(feature = "postgres")]
    return format!("CAST(FLOOR(EXTRACT(EPOCH FROM {column})) AS BIGINT)");
    #[cfg(not(feature = "postgres"))]
    return format!("CAST(strftime('%s', {column}) AS INTEGER)");
}

/// The start of `column`'s bucket of `secs` seconds, as epoch seconds: read
/// it as `i64` and turn it into a time with [`bucket_time`].
pub(crate) fn bucket(column: &str, secs: i64) -> String {
    format!("({} / {secs} * {secs})", epoch(column))
}

/// A bucket read back from [`bucket`].
pub(crate) fn bucket_time(epoch_secs: i64) -> AppResult<DateTime<Utc>> {
    DateTime::from_timestamp(epoch_secs, 0)
        .ok_or_else(|| AppError::Internal(format!("bucket {epoch_secs} is out of range")))
}

/// `column` compared with the timestamp bound as `$param`. SQLite stores
/// timestamps as text in more than one format, so both sides go through
/// `datetime()` there.
pub(crate) fn compare_time(column: &str, op: &str, param: usize) -> String {
    #[cfg(feature = "postgres")]
    return format!("{column} {op} ${param}");
    #[cfg(not(feature = "postgres"))]
    return format!("datetime({column}) {op} datetime(${param})");
}

/// A JSON column's member at `path`, as SQL that decodes to JSON on both
/// backends. `path` is trusted: names, never request text.
pub(crate) fn json_member(column: &str, path: &[&str]) -> String {
    #[cfg(feature = "postgres")]
    return path
        .iter()
        .fold(column.to_string(), |sql, key| format!("{sql} -> '{key}'"));
    // `->`, not `json_extract`: a string member stays JSON text instead of
    // turning into a bare SQL string that no longer decodes as JSON.
    #[cfg(not(feature = "postgres"))]
    return format!("{column} -> '$.{}'", path.join("."));
}

/// [`since`] for a `DATE` column.
pub(crate) fn since_day(column: &str, hours: Option<i64>) -> String {
    let Some(hours) = hours else {
        return "1 = 1".to_string();
    };
    #[cfg(feature = "postgres")]
    return format!("{column} >= (NOW() - INTERVAL '{hours} hours')::date");
    #[cfg(not(feature = "postgres"))]
    return format!("{column} >= date('now', '-{hours} hours')");
}

/// A search as a case-insensitive `LIKE ... ESCAPE '\'` pattern, its own
/// `%`, `_` and `\` taken literally. `None` for a blank search.
///
/// Lowered by the same rules as the column's `LOWER()`: Unicode on Postgres,
/// ASCII only on SQLite, where `É` stays `É` on both sides and still matches.
// ponytail: on SQLite `échec` does not find `Échec`; that takes the ICU
// extension's `LOWER()`.
pub(crate) fn like_pattern(query: Option<&str>) -> Option<String> {
    let query = query?.trim();
    #[cfg(feature = "postgres")]
    let query = query.to_lowercase();
    #[cfg(not(feature = "postgres"))]
    let query = query.to_ascii_lowercase();
    if query.is_empty() {
        return None;
    }
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    Some(format!("%{escaped}%"))
}

#[cfg(all(test, not(feature = "postgres")))]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use sqlx::SqlitePool;

    /// Instants at every precision sqlx writes a `DateTime<Utc>` with
    /// (`SecondsFormat::AutoSi`: 0, 3, 6 or 9 fraction digits), across a
    /// day boundary.
    fn instants(base: DateTime<Utc>) -> Vec<DateTime<Utc>> {
        let nanos = [
            0,
            1,
            999,
            1_000,
            500_000,
            999_999,
            1_000_000,
            123_000_000,
            500_000_000,
            999_000_000,
            999_999_999,
        ];
        (-2..3)
            .flat_map(|s| nanos.map(|ns| base + Duration::seconds(s) + Duration::nanoseconds(ns)))
            .collect()
    }

    /// A table of `values`, each stored the way the app stores it: bound.
    async fn table(values: &[DateTime<Utc>]) -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE t (v TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        for v in values {
            sqlx::query("INSERT INTO t (v) VALUES ($1)")
                .bind(v)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    async fn count(pool: &SqlitePool, condition: &str, bound: Option<DateTime<Utc>>) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM t WHERE {condition}");
        let query = sqlx::query_as::<_, (i64,)>(sqlx::AssertSqlSafe(sql));
        let query = match bound {
            Some(b) => query.bind(b),
            None => query,
        };
        query.fetch_one(pool).await.unwrap().0
    }

    #[tokio::test]
    async fn stored_text_sorts_chronologically() {
        let base = Utc.with_ymd_and_hms(2026, 3, 9, 23, 59, 59).unwrap();
        let mut values = instants(base);
        let pool = table(&values).await;
        let stored: Vec<(DateTime<Utc>,)> = sqlx::query_as("SELECT v FROM t ORDER BY v")
            .fetch_all(&pool)
            .await
            .unwrap();
        values.sort();
        assert_eq!(stored.into_iter().map(|r| r.0).collect::<Vec<_>>(), values);
    }

    #[tokio::test]
    async fn compare_utc_agrees_with_compare_time() {
        let base = Utc.with_ymd_and_hms(2026, 3, 9, 23, 59, 59).unwrap();
        let values = instants(base);
        let pool = table(&values).await;
        for bound in &values {
            for op in [">=", "<"] {
                assert_eq!(
                    count(&pool, &compare_utc("v", op, 1), Some(*bound)).await,
                    count(&pool, &compare_time("v", op, 1), Some(*bound)).await,
                    "v {op} {bound}"
                );
            }
        }
    }

    #[tokio::test]
    async fn since_utc_agrees_with_since() {
        // Whole seconds away from the edge, so the two queries' `now` can
        // differ by a few milliseconds without changing either answer.
        let edge = Utc::now() - Duration::hours(24);
        let values: Vec<_> = [-90_000, -3_600, -2, 2, 3_600]
            .into_iter()
            .flat_map(|s| instants(edge + Duration::seconds(s * 3)))
            .collect();
        let pool = table(&values).await;
        let old = count(&pool, &since("v", Some(24)), None).await;
        assert_eq!(count(&pool, &since_utc("v", Some(24)), None).await, old);
        assert_eq!(old, 2 * 5 * 11, "the two groups after the edge");
    }
}
