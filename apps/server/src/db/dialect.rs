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

/// [`since`] comparing the stored value as it is, as the queries it replaced
/// did, so an index on `column` still serves the range on SQLite.
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

/// [`compare_time`] for a [`QueryBuilder`], which numbers its own binds.
pub(crate) fn push_compare_time(
    qb: &mut QueryBuilder<Db>,
    column: &str,
    op: &str,
    value: DateTime<Utc>,
) {
    #[cfg(feature = "postgres")]
    {
        qb.push(format!("{column} {op} "));
        qb.push_bind(value);
    }
    #[cfg(not(feature = "postgres"))]
    {
        qb.push(format!("datetime({column}) {op} datetime("));
        qb.push_bind(value);
        qb.push(")");
    }
}

/// A JSON column's member at `path`, as SQL that decodes to JSON on both
/// backends. `path` is trusted: names, never request text.
pub(crate) fn json_member(column: &str, path: &[&str]) -> String {
    #[cfg(feature = "postgres")]
    return path
        .iter()
        .fold(column.to_string(), |sql, key| format!("{sql} -> '{key}'"));
    #[cfg(not(feature = "postgres"))]
    return format!("json_extract({column}, '$.{}')", path.join("."));
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
