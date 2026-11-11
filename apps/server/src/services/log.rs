use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::db::dialect::{epoch, like_pattern, since};
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::{LogResponse, LogVolumePoint};
use crate::services::scope::bucket_hours;

pub struct LogService;

/// What narrows the log list and its volume. A `None` field matches every
/// row.
#[derive(Debug, Default, Clone)]
pub struct LogFilters {
    /// Exact level (trace/debug/info/warn/error/fatal).
    pub level: Option<String>,
    pub trace_id: Option<String>,
    /// Keeps the logs whose message contains it, ignoring case.
    pub query: Option<String>,
    pub period_hours: Option<i64>,
}

impl LogFilters {
    /// The condition after `project_id = $1`, binding `$2..=$7`: level, trace
    /// id and the message pattern, each twice. Static SQL with positional
    /// binds, so it reads the same on Postgres and SQLite.
    fn condition(&self) -> String {
        format!(
            "($2 IS NULL OR level = $3) AND ($4 IS NULL OR trace_id = $5) \
             AND ($6 IS NULL OR LOWER(body) LIKE $7 ESCAPE '\\') AND {}",
            since("timestamp", self.period_hours)
        )
    }
}

/// Binds `$1..=$7`: the project, then [`LogFilters::condition`]'s pairs.
macro_rules! bind_filters {
    ($query:expr, $project_id:expr, $filters:expr, $like:expr) => {
        $query
            .bind($project_id)
            .bind($filters.level.as_deref())
            .bind($filters.level.as_deref())
            .bind($filters.trace_id.as_deref())
            .bind($filters.trace_id.as_deref())
            .bind($like.as_deref())
            .bind($like.as_deref())
    };
}

impl LogService {
    /// Lists logs for a project with offset-based pagination (newest first by
    /// log timestamp), narrowed by `filters`. Returns (rows, total).
    pub async fn list_offset(
        pool: &DbPool,
        project_id: i32,
        page: i64,
        per_page: i64,
        filters: &LogFilters,
    ) -> AppResult<(Vec<LogResponse>, i64)> {
        // Clamp at the service boundary so a direct caller passing page <= 0
        // can't produce a negative SQL OFFSET (the HTTP route already guards it).
        let page = page.max(1);
        let offset = crate::pagination::offset(page, per_page);
        let like = like_pattern(filters.query.as_deref());
        let condition = filters.condition();

        let count_sql = format!("SELECT COUNT(*) FROM logs WHERE project_id = $1 AND {condition}");
        let total_count: (i64,) = bind_filters!(
            sqlx::query_as(sqlx::AssertSqlSafe(&*count_sql)),
            project_id,
            filters,
            like
        )
        .fetch_one(pool)
        .await?;

        let sql = format!(
            "SELECT id, trace_id, span_id, level, severity_number, body, \
                    attributes, timestamp, ingested_at \
             FROM logs WHERE project_id = $1 AND {condition} \
             ORDER BY timestamp DESC, id DESC LIMIT $8 OFFSET $9"
        );
        let rows = bind_filters!(
            sqlx::query(sqlx::AssertSqlSafe(&*sql)),
            project_id,
            filters,
            like
        )
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((rows.iter().map(log_response).collect(), total_count.0))
    }

    /// One of a project's logs by id; `None` if it has no such log.
    pub async fn get(pool: &DbPool, project_id: i32, id: Uuid) -> AppResult<Option<LogResponse>> {
        let row = sqlx::query(
            "SELECT id, trace_id, span_id, level, severity_number, body, \
                    attributes, timestamp, ingested_at \
             FROM logs WHERE project_id = $1 AND id = $2",
        )
        .bind(project_id)
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(row.as_ref().map(log_response))
    }

    /// How many logs `filters` matches, level by level, bucket by bucket with
    /// no gaps. The window is the period, or without one everything since the
    /// oldest matching log; empty when nothing matches at all.
    pub async fn volume(
        pool: &DbPool,
        project_id: i32,
        filters: &LogFilters,
    ) -> AppResult<Vec<LogVolumePoint>> {
        let like = like_pattern(filters.query.as_deref());
        let condition = filters.condition();
        let epoch = epoch("timestamp");
        let now = Utc::now().timestamp();

        let start = match filters.period_hours {
            Some(hours) => now - hours * 3600,
            None => {
                let sql =
                    format!("SELECT MIN({epoch}) FROM logs WHERE project_id = $1 AND {condition}");
                let (oldest,): (Option<i64>,) = bind_filters!(
                    sqlx::query_as(sqlx::AssertSqlSafe(&*sql)),
                    project_id,
                    filters,
                    like
                )
                .fetch_one(pool)
                .await?;
                match oldest {
                    Some(oldest) => oldest.min(now),
                    None => return Ok(Vec::new()),
                }
            }
        };

        let interval = volume_bucket_hours((now - start) / 3600) * 3600;
        let sql = format!(
            "SELECT ({epoch} / {interval}) * {interval} AS at, level, COUNT(*) \
             FROM logs WHERE project_id = $1 AND {condition} GROUP BY at, level"
        );
        let rows: Vec<(i64, String, i64)> = bind_filters!(
            sqlx::query_as(sqlx::AssertSqlSafe(&*sql)),
            project_id,
            filters,
            like
        )
        .fetch_all(pool)
        .await?;

        let align = |ts: i64| ts.div_euclid(interval) * interval;
        let mut buckets: BTreeMap<i64, LogVolumePoint> = BTreeMap::new();
        let mut at = align(start);
        while at <= align(now) {
            buckets.insert(at, LogVolumePoint::empty(at));
            at += interval;
        }
        for (at, level, count) in rows {
            // A clock running ahead still counts, in the current bucket, as
            // the list counts it.
            if let Some(point) = buckets.get_mut(&at.min(align(now))) {
                point.add(&level, count);
            }
        }
        Ok(buckets.into_values().collect())
    }
}

fn log_response(row: &<crate::db::Db as sqlx::Database>::Row) -> LogResponse {
    let timestamp: DateTime<Utc> = row.get("timestamp");
    let ingested_at: DateTime<Utc> = row.get("ingested_at");
    LogResponse {
        id: row.get("id"),
        trace_id: row.get("trace_id"),
        span_id: row.get("span_id"),
        level: row.get("level"),
        severity_number: row.get("severity_number"),
        body: row.get("body"),
        attributes: row.get("attributes"),
        timestamp,
        ingested_at,
    }
}

/// [`bucket_hours`] for the dashboard's windows; past them, as many whole
/// days as keep it near sixty buckets.
fn volume_bucket_hours(span_hours: i64) -> i64 {
    if span_hours <= 90 * 24 {
        bucket_hours(span_hours)
    } else {
        (span_hours + 60 * 24 - 1) / (60 * 24) * 24
    }
}

#[cfg(test)]
mod tests {
    use super::volume_bucket_hours;

    #[test]
    fn volume_buckets_stay_near_sixty_past_ninety_days() {
        assert_eq!(volume_bucket_hours(24), 1);
        assert_eq!(volume_bucket_hours(90 * 24), 24);
        assert_eq!(volume_bucket_hours(600 * 24), 240);
        assert_eq!(volume_bucket_hours(601 * 24), 264);
    }
}
