use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use uuid::Uuid;

use crate::config::RateLimitConfig;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::{Project, QuotaWindows};

pub struct RateLimitService;

/// Events accepted into the spool whose digest has not finished: counted
/// against the quota they will consume, for the installation and one project.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    pub installation: i64,
    pub project: i64,
}

/// The error events this process has accepted and not digested yet, so the
/// ingest check sees them before the digest counts them in the database and
/// a burst is refused at the door instead of dropped after a 200.
///
/// An event is added when it is spooled (or found in the spool at startup)
/// and removed when its digest is over: stored, a duplicate, dropped or
/// failed for good. A digest failure that keeps the file for replay keeps the
/// event here too. Keyed by event, so a duplicate delivery or a replay is
/// counted once. In memory because one process owns the database and its
/// spool.
#[derive(Default)]
pub struct PendingQuota {
    inner: Mutex<PendingEvents>,
}

#[derive(Default)]
struct PendingEvents {
    events: HashSet<(i32, Uuid)>,
    per_project: HashMap<i32, i64>,
}

impl PendingQuota {
    pub fn insert(&self, project_id: i32, event_id: Uuid) {
        let mut inner = self.inner.lock().unwrap();
        if inner.events.insert((project_id, event_id)) {
            *inner.per_project.entry(project_id).or_default() += 1;
        }
    }

    pub fn remove(&self, project_id: i32, event_id: Uuid) {
        let mut inner = self.inner.lock().unwrap();
        if inner.events.remove(&(project_id, event_id)) {
            if let Some(count) = inner.per_project.get_mut(&project_id) {
                *count -= 1;
                if *count == 0 {
                    inner.per_project.remove(&project_id);
                }
            }
            // A drained backlog gives its table back. Amortised: after a
            // shrink, a quarter of what is left must go before the next.
            if inner.events.len() < inner.events.capacity() / 4 {
                inner.events.shrink_to_fit();
            }
        }
    }

    /// What is pending for the installation and for `project_id`.
    pub fn counts(&self, project_id: i32) -> Pending {
        let inner = self.inner.lock().unwrap();
        Pending {
            installation: inner.events.len() as i64,
            project: inner.per_project.get(&project_id).copied().unwrap_or(0),
        }
    }
}

/// Seconds until a full window of `quota` rolls over, or `None` while both
/// the current minute and the current hour still have room. `pending` events
/// will be digested into the current windows, whichever window the stored
/// counters belong to.
fn seconds_while_full(
    quota: QuotaWindows,
    pending: i64,
    (minute_limit, hour_limit): (i64, i64),
    now: chrono::DateTime<Utc>,
) -> Option<u64> {
    let full = |window: i64, count: i64, limit: i64, length: i64| {
        let current = window == now.timestamp().div_euclid(length);
        let used = if current { count } else { 0 } + pending;
        ((current || pending > 0) && used >= limit)
            .then(|| (length - now.timestamp().rem_euclid(length)) as u64)
    };
    full(
        quota.quota_hour_window,
        quota.quota_hour_count,
        hour_limit,
        3600,
    )
    .or_else(|| {
        full(
            quota.quota_minute_window,
            quota.quota_minute_count,
            minute_limit,
            60,
        )
    })
}

/// The limits a project's events count against: the operator's
/// `MAX_EVENTS_PER_PROJECT_*`, tightened by the project's own where it has one.
fn project_limits(config: &RateLimitConfig, project: &Project) -> (i64, i64) {
    let tighten = |operator: i64, own: Option<i64>| own.map_or(operator, |own| own.min(operator));
    (
        tighten(
            config.max_events_per_project_per_minute,
            project.rate_limit_per_minute,
        ),
        tighten(
            config.max_events_per_project_per_hour,
            project.rate_limit_per_hour,
        ),
    )
}

/// Result when quota is exceeded
#[derive(Debug)]
pub struct QuotaExceeded {
    /// Seconds until the quota resets
    pub retry_after: u64,
    /// Which row was full: the installation or the project.
    pub scope: QuotaScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaScope {
    Installation,
    Project,
}

/// The quota rows a batch's events count against, read once in its
/// transaction (locked on Postgres; SQLite holds the one write lock), then
/// decided in memory and written back once.
#[derive(Debug)]
pub struct QuotaRows {
    pub installation: QuotaWindows,
    pub projects: HashMap<i32, ProjectQuotaRow>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ProjectQuotaRow {
    #[sqlx(flatten)]
    pub quota: QuotaWindows,
    pub rate_limit_per_minute: Option<i64>,
    pub rate_limit_per_hour: Option<i64>,
    /// Events admitted: counted as stored and digested.
    #[sqlx(skip)]
    pub admitted: i64,
    /// Events the quota dropped, whichever row was full.
    #[sqlx(skip)]
    pub limited: i64,
}

/// Reads and locks the installation's quota row and those of `project_ids`.
/// A project with no row is left out: its events are dropped as the
/// project's, as an `UPDATE` matching no row always dropped them.
pub async fn lock_quota_rows(
    conn: &mut <crate::db::Db as sqlx::Database>::Connection,
    project_ids: &[i32],
) -> AppResult<QuotaRows> {
    #[cfg(feature = "postgres")]
    const LOCK: &str = " FOR NO KEY UPDATE";
    #[cfg(feature = "sqlite")]
    const LOCK: &str = "";
    let installation: QuotaWindows = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT quota_minute_window, quota_minute_count, quota_hour_window, quota_hour_count \
         FROM installation WHERE id = 1{LOCK}"
    )))
    .fetch_one(&mut *conn)
    .await?;
    let mut select = sqlx::QueryBuilder::<crate::db::Db>::new(
        "SELECT id, quota_minute_window, quota_minute_count, quota_hour_window, quota_hour_count, \
         rate_limit_per_minute, rate_limit_per_hour FROM projects WHERE id IN (",
    );
    let mut ids = select.separated(", ");
    for id in project_ids {
        ids.push_bind(*id);
    }
    select.push(")");
    // Ascending, so two writers lock project rows in one order.
    #[cfg(feature = "postgres")]
    select.push(" ORDER BY id FOR NO KEY UPDATE");
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i32,
        #[sqlx(flatten)]
        row: ProjectQuotaRow,
    }
    let projects = select
        .build_query_as::<Row>()
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|r| (r.id, r.row))
        .collect();
    Ok(QuotaRows {
        installation,
        projects,
    })
}

/// Decides each event of a batch, in order, exactly as one conditional
/// `UPDATE` per row and event in its own transaction would: an event finding
/// the installation full consumes nothing; one finding its project full
/// consumes nothing either (that transaction rolled the installation's count
/// back); any other counts on both rows. `None` is admitted, `Some` names
/// the full row.
pub fn admit_batch(
    rows: &mut QuotaRows,
    config: &RateLimitConfig,
    now: chrono::DateTime<Utc>,
    events: impl IntoIterator<Item = i32>,
) -> Vec<Option<QuotaScope>> {
    let (minute, hour) = (
        now.timestamp().div_euclid(60),
        now.timestamp().div_euclid(3600),
    );
    let has_room = |q: &QuotaWindows, (minute_limit, hour_limit): (i64, i64)| {
        (q.quota_minute_window != minute || q.quota_minute_count < minute_limit)
            && (q.quota_hour_window != hour || q.quota_hour_count < hour_limit)
    };
    let consume = |q: &mut QuotaWindows| {
        q.quota_minute_count = if q.quota_minute_window == minute {
            q.quota_minute_count + 1
        } else {
            1
        };
        q.quota_minute_window = minute;
        q.quota_hour_count = if q.quota_hour_window == hour {
            q.quota_hour_count + 1
        } else {
            1
        };
        q.quota_hour_window = hour;
    };
    let installation_limits = (config.max_events_per_minute, config.max_events_per_hour);
    events
        .into_iter()
        .map(|project_id| {
            let project = rows.projects.get_mut(&project_id);
            let scope = if !has_room(&rows.installation, installation_limits) {
                Some(QuotaScope::Installation)
            } else {
                match &project {
                    Some(row) if has_room(&row.quota, row_limits(config, row)) => None,
                    _ => Some(QuotaScope::Project),
                }
            };
            if let Some(row) = project {
                match scope {
                    None => {
                        consume(&mut rows.installation);
                        consume(&mut row.quota);
                        row.admitted += 1;
                    }
                    Some(_) => row.limited += 1,
                }
            }
            scope
        })
        .collect()
}

/// The limits a project row's events count against, as the conditional `UPDATE`
/// reads them: the operator's, tightened by the row's own.
fn row_limits(config: &RateLimitConfig, row: &ProjectQuotaRow) -> (i64, i64) {
    let tighten = |operator: i64, own: Option<i64>| own.map_or(operator, |own| own.min(operator));
    (
        tighten(
            config.max_events_per_project_per_minute,
            row.rate_limit_per_minute,
        ),
        tighten(
            config.max_events_per_project_per_hour,
            row.rate_limit_per_hour,
        ),
    )
}

/// Writes back what [`admit_batch`] decided: absolute windows (the rows are
/// locked, or the write lock held, since [`lock_quota_rows`]) and the event
/// counters, relative.
pub async fn write_quota_rows(
    conn: &mut <crate::db::Db as sqlx::Database>::Connection,
    rows: &QuotaRows,
) -> AppResult<()> {
    let admitted: i64 = rows.projects.values().map(|row| row.admitted).sum();
    if admitted > 0 {
        let q = rows.installation;
        sqlx::query(
            "UPDATE installation SET digested_event_count = digested_event_count + $1, \
             quota_minute_window = $2, quota_minute_count = $3, \
             quota_hour_window = $4, quota_hour_count = $5 WHERE id = 1",
        )
        .bind(admitted)
        .bind(q.quota_minute_window)
        .bind(q.quota_minute_count)
        .bind(q.quota_hour_window)
        .bind(q.quota_hour_count)
        .execute(&mut *conn)
        .await?;
    }
    let mut ids: Vec<&i32> = rows.projects.keys().collect();
    ids.sort();
    for id in ids {
        let row = &rows.projects[id];
        if row.admitted + row.limited == 0 {
            continue;
        }
        let q = row.quota;
        sqlx::query(
            "UPDATE projects SET stored_event_count = stored_event_count + $1, \
             digested_event_count = digested_event_count + $1, \
             rate_limited_event_count = rate_limited_event_count + $2, \
             quota_minute_window = $3, quota_minute_count = $4, \
             quota_hour_window = $5, quota_hour_count = $6 WHERE id = $7",
        )
        .bind(row.admitted)
        .bind(row.limited)
        .bind(q.quota_minute_window)
        .bind(q.quota_minute_count)
        .bind(q.quota_hour_window)
        .bind(q.quota_hour_count)
        .bind(*id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

impl QuotaExceeded {
    /// The `X-Sentry-Rate-Limits` value Relay would send for this limit:
    /// `retry_after:categories:scope`, with no categories because the quota
    /// counts every event alike. The installation is Relay's organization.
    pub fn sentry_rate_limits_header(&self) -> String {
        let scope = match self.scope {
            QuotaScope::Installation => "organization",
            QuotaScope::Project => "project",
        };
        format!("{}::{scope}", self.retry_after)
    }
}

impl RateLimitService {
    /// Checks if quota is exceeded for installation or project (call during ingest)
    /// Returns Some(QuotaExceeded) if rate limited, None if allowed
    ///
    /// `installation` and `project` are the rows the caller has just loaded
    /// (`ProjectService::get_with_installation_quota`, from the ingest
    /// extractor or the digest): they are read as the current quota state
    /// rather than fetched a second time. `pending` is what ingest has
    /// accepted and the digest not counted yet ([`PendingQuota`]); the digest
    /// passes none, its own event being one of them.
    pub fn check_quota(
        installation: &QuotaWindows,
        project: &Project,
        config: &RateLimitConfig,
        pending: Pending,
    ) -> Option<QuotaExceeded> {
        let now = Utc::now();

        let limits = (config.max_events_per_minute, config.max_events_per_hour);
        if let Some(retry_after) =
            seconds_while_full(*installation, pending.installation, limits, now)
        {
            return Some(QuotaExceeded {
                retry_after,
                scope: QuotaScope::Installation,
            });
        }

        let limits = project_limits(config, project);
        seconds_while_full(project.quota, pending.project, limits, now).map(|retry_after| {
            QuotaExceeded {
                retry_after,
                scope: QuotaScope::Project,
            }
        })
    }

    /// Counts one event against every quota window, or none of them, and
    /// counts it as stored on the installation and its project.
    ///
    /// [`lock_quota_rows`], [`admit_batch`] and [`write_quota_rows`] for one
    /// event: the rows are read under the lock, so two digests can never
    /// both take the last slot, the same all-or-nothing contract as Relay's
    /// `is_rate_limited.lua`. The project's own limits are read from its row
    /// there, not from `project`: a limit lowered after the digest loaded the
    /// row must still hold. Runs inside the caller's transaction; the scope
    /// names the row that was full.
    pub async fn try_consume(
        executor: &mut <crate::db::Db as sqlx::Database>::Connection,
        project: &Project,
        config: &RateLimitConfig,
        now: chrono::DateTime<Utc>,
    ) -> AppResult<Option<QuotaScope>> {
        let mut rows = lock_quota_rows(executor, &[project.id]).await?;
        let scope = admit_batch(&mut rows, config, now, [project.id])
            .pop()
            .flatten();
        write_quota_rows(executor, &rows).await?;
        Ok(scope)
    }

    /// Counts one event the quota dropped against the project it was sent to,
    /// whichever row was full. Runs after the digest rolled back, on its own.
    pub async fn record_rate_limited(pool: &DbPool, project_id: i32) -> AppResult<()> {
        sqlx::query(
            "UPDATE projects SET rate_limited_event_count = rate_limited_event_count + 1 WHERE id = $1",
        )
        .bind(project_id)
        .execute(pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drained backlog gives its memory back instead of keeping the table
    /// sized for its peak.
    #[test]
    fn a_drained_backlog_shrinks_the_table() {
        let pending = PendingQuota::default();
        let ids: Vec<Uuid> = (0..10_000).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            pending.insert(1, *id);
        }
        for id in &ids {
            pending.remove(1, *id);
        }
        assert!(pending.inner.lock().unwrap().events.capacity() < 100);
    }

    /// The conditional `UPDATE` each event once ran on one row: the WHERE says
    /// whether it has room, the SET what it becomes.
    fn consume_row(row: &mut QuotaWindows, minute: i64, hour: i64, limits: (i64, i64)) -> bool {
        let room = (row.quota_minute_window != minute || row.quota_minute_count < limits.0)
            && (row.quota_hour_window != hour || row.quota_hour_count < limits.1);
        if room {
            row.quota_minute_count = if row.quota_minute_window == minute {
                row.quota_minute_count + 1
            } else {
                1
            };
            row.quota_minute_window = minute;
            row.quota_hour_count = if row.quota_hour_window == hour {
                row.quota_hour_count + 1
            } else {
                1
            };
            row.quota_hour_window = hour;
        }
        room
    }

    proptest::proptest! {
        /// A batch admits exactly what one digest per event would have, in
        /// batch order: the installation first, untouched when the project
        /// then has no room, as the per-event transaction's rollback left it.
        #[test]
        fn admit_batch_matches_per_event_consume(
            seed_rows in proptest::collection::vec(
                (0i64..3, 0i64..6, 0i64..3, 0i64..8, proptest::option::of(0i64..5), proptest::option::of(0i64..7)),
                4,
            ),
            limits in (0i64..8, 0i64..12, 0i64..6, 0i64..9),
            events in proptest::collection::vec(1i32..=3, 0..30),
        ) {
            let now = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
            let (minute, hour) = (now.timestamp().div_euclid(60), now.timestamp().div_euclid(3600));
            // Window offsets 0..3 back from now: 0 is the current window.
            let windows = |(mw, mc, hw, hc, _, _): (i64, i64, i64, i64, Option<i64>, Option<i64>)| QuotaWindows {
                quota_minute_window: minute - mw,
                quota_minute_count: mc,
                quota_hour_window: hour - hw,
                quota_hour_count: hc,
            };
            let config = RateLimitConfig {
                max_events_per_minute: limits.0,
                max_events_per_hour: limits.1,
                max_events_per_project_per_minute: limits.2,
                max_events_per_project_per_hour: limits.3,
            };
            // Project 3 is gone: its row is not there.
            let mut rows = QuotaRows {
                installation: windows(seed_rows[0]),
                projects: (1..=2)
                    .map(|id| {
                        let seed = seed_rows[id as usize];
                        (id, ProjectQuotaRow {
                            quota: windows(seed),
                            rate_limit_per_minute: seed.4,
                            rate_limit_per_hour: seed.5,
                            admitted: 0,
                            limited: 0,
                        })
                    })
                    .collect(),
            };

            let mut installation = rows.installation;
            let mut projects: HashMap<i32, QuotaWindows> =
                rows.projects.iter().map(|(id, row)| (*id, row.quota)).collect();
            let mut expected = Vec::new();
            for &project_id in &events {
                let before = installation;
                if !consume_row(&mut installation, minute, hour, (limits.0, limits.1)) {
                    expected.push(Some(QuotaScope::Installation));
                    continue;
                }
                let consumed = rows.projects.get(&project_id).is_some_and(|row| {
                    let tighten = |operator: i64, own: Option<i64>| match own {
                        Some(own) if own < operator => own,
                        _ => operator,
                    };
                    let limits = (
                        tighten(limits.2, row.rate_limit_per_minute),
                        tighten(limits.3, row.rate_limit_per_hour),
                    );
                    consume_row(projects.get_mut(&project_id).unwrap(), minute, hour, limits)
                });
                if consumed {
                    expected.push(None);
                } else {
                    installation = before;
                    expected.push(Some(QuotaScope::Project));
                }
            }

            let admitted = admit_batch(&mut rows, &config, now, events.iter().copied());

            proptest::prop_assert_eq!(&admitted, &expected);
            proptest::prop_assert_eq!(format!("{:?}", rows.installation), format!("{installation:?}"));
            for (id, row) in &rows.projects {
                proptest::prop_assert_eq!(format!("{:?}", row.quota), format!("{:?}", projects[id]));
                let count = |scope: Option<QuotaScope>| {
                    events.iter().zip(&expected).filter(|(p, s)| *p == id && **s == scope).count() as i64
                };
                proptest::prop_assert_eq!(row.admitted, count(None));
                proptest::prop_assert_eq!(
                    row.limited,
                    count(Some(QuotaScope::Installation)) + count(Some(QuotaScope::Project))
                );
            }
        }
    }

    #[test]
    fn an_event_is_pending_once_however_often_it_is_added() {
        let pending = PendingQuota::default();
        let (event, other) = (Uuid::new_v4(), Uuid::new_v4());
        pending.insert(1, event);
        pending.insert(1, event);
        pending.insert(2, other);
        assert_eq!(
            pending.counts(1),
            Pending {
                installation: 2,
                project: 1
            }
        );

        pending.remove(1, event);
        pending.remove(1, event);
        pending.remove(3, Uuid::new_v4());
        assert_eq!(
            pending.counts(1),
            Pending {
                installation: 1,
                project: 0
            }
        );
    }
}
