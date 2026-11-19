//! A span's description stored once per project (issue #558).
//!
//! On an ORM-backed app the SQL text is about half of a database span's row
//! and has a few hundred distinct values. A description seen again is kept
//! in `span_descriptions` and its spans refer to it; reads put the text
//! back ([`span_description!`]), so nothing above the database changes.
//! Sentry keeps the raw description too (`sentry.raw_description`), beside
//! the scrubbed one it groups by, which this does not replace.
//!
//! What stays in the span's own row:
//! - a description under [`MIN_LENGTH`], where a reference saves little;
//! - a description on its first sight, so that one nobody sends again (a URL
//!   with an id, SQL with its values inlined) never gets a row of its own;
//! - any new description of a project already at [`MAX_PER_PROJECT`].

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};

use chrono::{DateTime, TimeDelta, Utc};
use sha2::{Digest, Sha256};

use crate::db::{Db, DbPool};
use crate::digest::processors::span::SpanRow;
use crate::error::AppResult;

/// The shortest description stored apart. A reference is 8 bytes; under
/// this the saving does not pay for the second read.
pub const MIN_LENGTH: usize = 64;

/// The most descriptions stored apart for one project: what bounds the
/// table when an app's long descriptions repeat without ever settling. The
/// apps measured had 72 and 219.
pub const MAX_PER_PROJECT: i64 = 10_000;

/// How long an answer is trusted before the row is asked for again, which
/// also renews its `last_used_at`. Under [`UNUSED_AFTER`], so a row a span
/// is about to refer to is never old enough to remove.
const TRUST: TimeDelta = TimeDelta::days(1);

/// How long after its last use a row no span refers to is removed.
const UNUSED_AFTER: TimeDelta = TimeDelta::days(2);

/// Descriptions remembered per database before the memory starts over.
const MAX_REMEMBERED: usize = 200_000;

/// A span's `description` as a column of a query over `spans`: its own, or
/// the stored copy it refers to. A macro so it can sit in a string literal.
#[macro_export]
macro_rules! span_description {
    () => {
        "COALESCE(description, (SELECT d.text FROM span_descriptions d \
          WHERE d.id = spans.description_id)) AS description"
    };
}

/// A span id column of a query over `spans`, by name: its text, or the
/// number a 16-hex id is kept as, written back as those 16 characters.
#[cfg(feature = "postgres")]
#[macro_export]
macro_rules! span_hex_id {
    ($column:literal) => {
        concat!(
            "COALESCE(",
            $column,
            ", lpad(to_hex(",
            $column,
            "_int), 16, '0')) AS ",
            $column
        )
    };
}
#[cfg(not(feature = "postgres"))]
#[macro_export]
macro_rules! span_hex_id {
    ($column:literal) => {
        // printf reads a NULL as 0, hence the CASE.
        concat!(
            "COALESCE(",
            $column,
            ", CASE WHEN ",
            $column,
            "_int IS NOT NULL THEN printf('%016x', ",
            $column,
            "_int) END) AS ",
            $column
        )
    };
}

/// `platform`, `release` and `environment` as columns of a query over
/// `spans`: the span's own, or those of the transaction it came in, which
/// its spans do not repeat (see `span_row` in the transaction processor).
/// A transaction keeps `''` for one it was not sent; a span keeps NULL.
#[macro_export]
macro_rules! span_scope {
    () => {
        "COALESCE(platform, (SELECT NULLIF(t.platform, '') FROM transactions t \
          WHERE t.id = spans.transaction_id)) AS platform, \
         COALESCE(release, (SELECT NULLIF(t.release, '') FROM transactions t \
          WHERE t.id = spans.transaction_id)) AS release, \
         COALESCE(environment, (SELECT NULLIF(t.environment, '') FROM transactions t \
          WHERE t.id = spans.transaction_id)) AS environment"
    };
}

/// A span id as the number it is, when it is one written the usual way: 16
/// lowercase hex characters. Any other form (shorter, uppercase, not hex)
/// stays text, since the number would not give it back.
pub(crate) fn hex_id(id: &str) -> Option<i64> {
    let usual = id.len() == 16 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    // The bits of the u64, as the signed 64-bit integer both backends store.
    usual
        .then(|| u64::from_str_radix(id, 16).ok())
        .flatten()
        .map(|id| id as i64)
}

type Key = (i32, [u8; 16]);

#[derive(Clone, Copy)]
enum Seen {
    /// Once, and kept in its span's row.
    Once,
    /// Stored apart under this id, as of then.
    Stored(i64, DateTime<Utc>),
    /// Refused as of then: the project is at its limit.
    Refused(DateTime<Utc>),
}

type ConnectOptions = <<Db as sqlx::Database>::Connection as sqlx::Connection>::Options;

/// What each database's descriptions were last found to be. A database is
/// its pool, as in `db::payload`: one process owns it, so nothing else
/// stores a description, and the cleanup only removes rows this would ask
/// for again anyway.
static SEEN: LazyLock<Mutex<Vec<Remembered>>> = LazyLock::new(Default::default);

/// One database's descriptions.
type Remembered = (Weak<ConnectOptions>, HashMap<Key, Seen>);

fn seen_of<'a>(all: &'a mut Vec<Remembered>, pool: &DbPool) -> &'a mut HashMap<Key, Seen> {
    let options = Arc::downgrade(&pool.connect_options());
    all.retain(|(weak, _)| weak.strong_count() > 0);
    let at = match all.iter().position(|(weak, _)| weak.ptr_eq(&options)) {
        Some(at) => at,
        None => {
            all.push((options, HashMap::new()));
            all.len() - 1
        }
    };
    &mut all[at].1
}

/// Forgets every description of `pool`'s database, as a new process starts.
#[cfg(feature = "test-seams")]
pub fn forget(pool: &DbPool) {
    let mut all = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    seen_of(&mut all, pool).clear();
}

fn hash(text: &str) -> [u8; 16] {
    let mut hash = [0; 16];
    hash.copy_from_slice(&Sha256::digest(text.as_bytes())[..16]);
    hash
}

/// Makes each of `rows` refer to its description's stored copy where there
/// is one, leaving the rest as they are. Runs before the write's
/// transaction: a description is stored on its second sight only, in a
/// statement of its own.
pub(crate) async fn intern(pool: &DbPool, project_id: i32, rows: &mut [SpanRow]) -> AppResult<()> {
    let now = Utc::now();
    let fresh = |at: DateTime<Utc>| now - at < TRUST;
    // (row, hash) of the descriptions to ask the database about.
    let mut ask: Vec<(usize, [u8; 16])> = Vec::new();
    {
        let mut all = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
        let seen = seen_of(&mut all, pool);
        if seen.len() > MAX_REMEMBERED {
            seen.clear();
        }
        for (at, row) in rows.iter_mut().enumerate() {
            let Some(text) = row.description.as_deref() else {
                continue;
            };
            if text.len() < MIN_LENGTH {
                continue;
            }
            let hash = hash(text);
            match seen.get(&(project_id, hash)).copied() {
                None => {
                    seen.insert((project_id, hash), Seen::Once);
                }
                Some(Seen::Stored(id, at)) if fresh(at) => row.refer_to(id),
                Some(Seen::Refused(at)) if fresh(at) => {}
                Some(_) => ask.push((at, hash)),
            }
        }
    }

    let mut answers: HashMap<[u8; 16], Option<i64>> = HashMap::new();
    for (at, hash) in ask {
        let id = match answers.get(&hash) {
            Some(id) => *id,
            None => {
                let text = rows[at].description.as_deref().unwrap_or_default();
                let id = store(pool, project_id, &hash, text, now).await?;
                answers.insert(hash, id);
                let mut all = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
                seen_of(&mut all, pool).insert(
                    (project_id, hash),
                    id.map_or(Seen::Refused(now), |id| Seen::Stored(id, now)),
                );
                id
            }
        };
        if let Some(id) = id {
            rows[at].refer_to(id);
        }
    }
    Ok(())
}

/// The id of the stored copy of `text`, used as of `now`; `None` when it is
/// not stored and the project is at its limit.
async fn store(
    pool: &DbPool,
    project_id: i32,
    hash: &[u8; 16],
    text: &str,
    now: DateTime<Utc>,
) -> AppResult<Option<i64>> {
    let known: Option<i64> = sqlx::query_scalar(
        "UPDATE span_descriptions SET last_used_at = $1 \
         WHERE project_id = $2 AND hash = $3 RETURNING id",
    )
    .bind(now)
    .bind(project_id)
    .bind(hash.as_slice())
    .fetch_optional(pool)
    .await?;
    if known.is_some() {
        return Ok(known);
    }
    Ok(sqlx::query_scalar(
        "INSERT INTO span_descriptions (project_id, hash, text, last_used_at) \
         SELECT $1, $2, $3, $4 \
         WHERE (SELECT COUNT(*) FROM span_descriptions WHERE project_id = $5) < $6 \
         ON CONFLICT (project_id, hash) DO UPDATE SET last_used_at = excluded.last_used_at \
         RETURNING id",
    )
    .bind(project_id)
    .bind(hash.as_slice())
    .bind(text)
    .bind(now)
    .bind(project_id)
    .bind(MAX_PER_PROJECT)
    .fetch_optional(pool)
    .await?)
}

/// Removes the stored descriptions no span refers to and none was made to
/// refer to lately, and returns how many. Run with the retention cleanup,
/// which is what leaves them behind.
///
/// One pass over `spans`, and only when some row is old enough to go: no
/// index holds `description_id`, which would cost every span a few bytes
/// to make this, a daily job, faster.
pub(crate) async fn remove_unused(pool: &DbPool) -> AppResult<u64> {
    let before = Utc::now() - UNUSED_AFTER;
    let any: Option<i64> =
        sqlx::query_scalar("SELECT id FROM span_descriptions WHERE last_used_at < $1 LIMIT 1")
            .bind(before)
            .fetch_optional(pool)
            .await?;
    if any.is_none() {
        return Ok(0);
    }
    let removed = sqlx::query(
        "DELETE FROM span_descriptions WHERE last_used_at < $1 AND id NOT IN ( \
             SELECT DISTINCT description_id FROM spans WHERE description_id IS NOT NULL)",
    )
    .bind(before)
    .execute(pool)
    .await?
    .rows_affected();
    // Nothing remembered outlives the rows it names: [`TRUST`] already keeps
    // a remembered id from being this old, and this does not depend on it.
    if removed > 0 {
        let mut all = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
        seen_of(&mut all, pool).clear();
    }
    Ok(removed)
}
