use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Timelike, Utc};
use tokio::sync::Mutex;

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::session::{
    classify, is_valid_environment, is_valid_release, parse_ts, SessionAggregateItem,
    SessionAggregates, SessionOutcome, SessionStatus, SessionUpdate,
};

const OVERFLOW_RELEASE: &str = "<overflow>";

/// When an envelope's sessions arrived, and how far the device clock that
/// stamped them is off (Relay's `ClockDriftProcessor`).
#[derive(Debug, Clone, Copy)]
pub struct SessionClock {
    received_at: DateTime<Utc>,
    drift: chrono::Duration,
}

impl SessionClock {
    /// Relay's `MINIMUM_CLOCK_DRIFT`: less than this is not corrected.
    const MIN_DRIFT: chrono::Duration = chrono::Duration::minutes(55);
    /// Relay's `max_session_secs_in_past` and `max_secs_in_future` defaults.
    pub(crate) const MAX_AGE: chrono::Duration = chrono::Duration::days(5);
    const MAX_FUTURE: chrono::Duration = chrono::Duration::seconds(60);

    pub fn new(received_at: DateTime<Utc>, sent_at: Option<&str>) -> Self {
        let drift = sent_at
            .and_then(parse_ts)
            .map(|sent| received_at - sent)
            .filter(|drift| drift.abs() >= Self::MIN_DRIFT)
            .unwrap_or_default();
        Self { received_at, drift }
    }

    /// `raw` read, corrected for drift, and kept only inside Relay's window
    /// (`validate_session_timestamp`). Missing or unreadable is invalid.
    fn read(&self, raw: Option<&str>) -> Option<DateTime<Utc>> {
        self.correct(raw?).filter(|at| self.in_window(*at))
    }

    /// `raw` read and corrected for drift.
    fn correct(&self, raw: &str) -> Option<DateTime<Utc>> {
        Some(parse_ts(raw)? + self.drift)
    }

    fn in_window(&self, at: DateTime<Utc>) -> bool {
        let age = self.received_at - at;
        age <= Self::MAX_AGE && -age <= Self::MAX_FUTURE
    }
}

/// Key identifying a minute-bucketed session count row.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BucketKey {
    pub project_id: i32,
    pub release: String,
    pub environment: String,
    pub bucket: DateTime<Utc>,
}

#[derive(Debug, Default, Clone)]
pub struct Counters {
    pub total: i64,
    pub errored: i64,
    pub crashed: i64,
    pub abnormal: i64,
}

/// Key identifying a day-bucketed distinct-user row.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserKey {
    pub project_id: i32,
    pub release: String,
    pub environment: String,
    pub day: NaiveDate,
    pub did: String,
}

#[derive(Debug, Default)]
pub struct AggregatorState {
    /// `errored` is every errored session, fatal ones included (Relay's
    /// `session.error` set); Sentry's errored is that minus crashed and
    /// abnormal.
    pub counts: HashMap<BucketKey, Counters>,
    /// Map of UserKey → crashed; TRUE means this user had a crash this flush cycle.
    pub users: HashMap<UserKey, bool>,
    /// Sessions seen errored this flush cycle, by `(project, sid)`, with
    /// their bucket and exact start. Each adds one to its bucket's `errored`
    /// at the flush, unless `session_errors` already holds it.
    pub errored: HashMap<(i32, String), (BucketKey, DateTime<Utc>)>,
    /// Each project's releases in `counts`, so the cardinality cap needs no
    /// scan of every key per item.
    pub releases: HashMap<i32, HashSet<String>>,
}

pub struct SessionAggregator {
    pool: DbPool,
    state: Mutex<AggregatorState>,
    /// Serializes flushes, so the shutdown flush waits for an interval flush
    /// that already took its batch out of `state` and is still writing it.
    flushing: Mutex<()>,
    flush_interval_secs: u64,
    cardinality_cap: usize,
}

impl SessionAggregator {
    pub fn new(pool: DbPool, flush_interval_secs: u64, cardinality_cap: usize) -> Arc<Self> {
        Arc::new(Self {
            pool,
            state: Mutex::new(AggregatorState::default()),
            flushing: Mutex::new(()),
            flush_interval_secs,
            cardinality_cap,
        })
    }

    /// Ingest a single `session` envelope item.
    pub async fn ingest_session(
        &self,
        project_id: i32,
        update: &SessionUpdate,
        clock: SessionClock,
    ) {
        let mut state = self.state.lock().await;
        apply_update(&mut state, project_id, update, self.cardinality_cap, clock);
    }

    /// Ingest a pre-aggregated `sessions` envelope item.
    pub async fn ingest_aggregates(
        &self,
        project_id: i32,
        agg: &SessionAggregates,
        clock: SessionClock,
    ) {
        let mut state = self.state.lock().await;
        apply_aggregates(&mut state, project_id, agg, self.cardinality_cap, clock);
    }

    /// Flush all in-memory counters to the DB via batched UPSERT.
    pub async fn flush(&self) -> AppResult<()> {
        let _flushing = self.flushing.lock().await;
        let (counts, users, errored) = {
            let mut state = self.state.lock().await;
            let counts = std::mem::take(&mut state.counts);
            let users = std::mem::take(&mut state.users);
            let errored = std::mem::take(&mut state.errored);
            state.releases.clear();
            (counts, users, errored)
        };

        if counts.is_empty() && users.is_empty() && errored.is_empty() {
            return Ok(());
        }

        let mut commit_started = false;
        let result = async {
            // Read-then-write (the projects lookup comes first): IMMEDIATE on
            // SQLite, or a commit elsewhere fails the upgrade with
            // SQLITE_BUSY, which busy_timeout does not retry (db::begin_write).
            let mut tx = crate::db::begin_write(&self.pool).await?;
            // A project deleted since its sessions arrived takes them with it:
            // written, they would fail every flush on its foreign key.
            let mut gone = HashSet::new();
            let projects: HashSet<i32> = (counts.keys().map(|k| k.project_id))
                .chain(users.keys().map(|k| k.project_id))
                .collect();
            for project_id in projects {
                let exists: bool =
                    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM projects WHERE id = $1)")
                        .bind(project_id)
                        .fetch_one(&mut *tx)
                        .await?;
                if !exists {
                    gone.insert(project_id);
                }
            }
            // A session is errored once, whichever flush first saw it so.
            let mut written = counts.clone();
            written.retain(|key, _| !gone.contains(&key.project_id));
            for ((project_id, sid), (key, started)) in &errored {
                if gone.contains(project_id) {
                    continue;
                }
                // The exact start, not the bucket: the id must last as long as
                // updates to its session are accepted.
                if insert_errored_session(*project_id, sid, *started, &mut *tx).await? {
                    written.entry(key.clone()).or_default().errored += 1;
                }
            }
            for (key, c) in &written {
                self.upsert_count(key, c, &mut *tx).await?;
            }
            // A session older than Relay's window gets no more updates.
            delete_expired_errored_sessions(Utc::now() - SessionClock::MAX_AGE, &mut *tx).await?;
            for (key, &crashed) in users.iter().filter(|(k, _)| !gone.contains(&k.project_id)) {
                self.upsert_user(key, crashed, &mut *tx).await?;
            }
            commit_started = true;
            tx.commit().await
        }
        .await;

        if let Err(error) = result {
            if !commit_started {
                let mut state = self.state.lock().await;
                merge_state(&mut state, counts, users);
                for (id, key) in errored {
                    state.errored.entry(id).or_insert(key);
                }
            } else {
                log::error!(
                    "session_aggregator: commit outcome unknown; counters will not be replayed"
                );
            }
            return Err(error.into());
        }

        Ok(())
    }

    async fn upsert_count<'e, E>(
        &self,
        key: &BucketKey,
        c: &Counters,
        executor: E,
    ) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'e, Database = crate::db::Db>,
    {
        #[cfg(feature = "postgres")]
        sqlx::query(
            r#"
            INSERT INTO session_counts (project_id, release, environment, bucket, total, errored, crashed, abnormal)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (project_id, release, environment, bucket)
            DO UPDATE SET
                total    = session_counts.total    + EXCLUDED.total,
                errored  = session_counts.errored  + EXCLUDED.errored,
                crashed  = session_counts.crashed  + EXCLUDED.crashed,
                abnormal = session_counts.abnormal + EXCLUDED.abnormal
            "#,
        )
        .bind(key.project_id)
        .bind(&key.release)
        .bind(&key.environment)
        .bind(key.bucket)
        .bind(c.total)
        .bind(c.errored)
        .bind(c.crashed)
        .bind(c.abnormal)
        .execute(executor)
        .await?;

        #[cfg(not(feature = "postgres"))]
        sqlx::query(
            r#"
            INSERT INTO session_counts (project_id, release, environment, bucket, total, errored, crashed, abnormal)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT (project_id, release, environment, bucket)
            DO UPDATE SET
                total    = session_counts.total    + excluded.total,
                errored  = session_counts.errored  + excluded.errored,
                crashed  = session_counts.crashed  + excluded.crashed,
                abnormal = session_counts.abnormal + excluded.abnormal
            "#,
        )
        .bind(key.project_id)
        .bind(&key.release)
        .bind(&key.environment)
        .bind(key.bucket.naive_utc().to_string())
        .bind(c.total)
        .bind(c.errored)
        .bind(c.crashed)
        .bind(c.abnormal)
        .execute(executor)
        .await?;

        Ok(())
    }

    async fn upsert_user<'e, E>(
        &self,
        key: &UserKey,
        crashed: bool,
        executor: E,
    ) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'e, Database = crate::db::Db>,
    {
        #[cfg(feature = "postgres")]
        sqlx::query(
            r#"
            INSERT INTO session_users (project_id, release, environment, day, did, crashed)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (project_id, release, environment, day, did)
            DO UPDATE SET crashed = session_users.crashed OR EXCLUDED.crashed
            "#,
        )
        .bind(key.project_id)
        .bind(&key.release)
        .bind(&key.environment)
        .bind(key.day)
        .bind(&key.did)
        .bind(crashed)
        .execute(executor)
        .await?;

        #[cfg(not(feature = "postgres"))]
        sqlx::query(
            r#"
            INSERT INTO session_users (project_id, release, environment, day, did, crashed)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT (project_id, release, environment, day, did)
            DO UPDATE SET crashed = MAX(session_users.crashed, excluded.crashed)
            "#,
        )
        .bind(key.project_id)
        .bind(&key.release)
        .bind(&key.environment)
        .bind(key.day.to_string())
        .bind(&key.did)
        .bind(if crashed { 1i64 } else { 0i64 })
        .execute(executor)
        .await?;

        Ok(())
    }

    /// The interval loop that flushes the counters.
    pub async fn run(self: Arc<Self>) {
        log::info!("session_aggregator: started");
        let interval_secs = self.flush_interval_secs;
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        interval.tick().await; // consume the immediate first tick
        loop {
            interval.tick().await;
            if let Err(e) = self.flush().await {
                log::error!("session_aggregator: flush error: {:?}", e);
            }
        }
    }
}

fn merge_state(
    state: &mut AggregatorState,
    counts: HashMap<BucketKey, Counters>,
    users: HashMap<UserKey, bool>,
) {
    for (key, counters) in counts {
        state
            .releases
            .entry(key.project_id)
            .or_default()
            .insert(key.release.clone());
        let current = state.counts.entry(key).or_default();
        current.total += counters.total;
        current.errored += counters.errored;
        current.crashed += counters.crashed;
        current.abnormal += counters.abnormal;
    }
    for (key, crashed) in users {
        state
            .users
            .entry(key)
            .and_modify(|current| *current |= crashed)
            .or_insert(crashed);
    }
}

pub fn truncate_to_minute(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt.with_second(0)
        .and_then(|d| d.with_nanosecond(0))
        .unwrap_or(dt)
}

/// Apply cardinality cap: if the new release would exceed the per-project cap, use OVERFLOW_RELEASE.
pub fn apply_cardinality_cap(
    state: &mut AggregatorState,
    project_id: i32,
    release: String,
    cap: usize,
) -> String {
    let releases = state.releases.entry(project_id).or_default();
    if releases.contains(&release) {
        return release;
    }
    if releases.len() >= cap {
        return OVERFLOW_RELEASE.to_string();
    }
    releases.insert(release.clone());
    release
}

/// Records `sid` as errored; false when `session_errors` already holds it.
async fn insert_errored_session<'e, E>(
    project_id: i32,
    sid: &str,
    started: DateTime<Utc>,
    executor: E,
) -> Result<bool, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = crate::db::Db>,
{
    #[cfg(not(feature = "postgres"))]
    let started = started.naive_utc().to_string();
    let done = sqlx::query(
        "INSERT INTO session_errors (project_id, sid, started) VALUES ($1, $2, $3) \
         ON CONFLICT (project_id, sid) DO NOTHING",
    )
    .bind(project_id)
    .bind(sid)
    .bind(started)
    .execute(executor)
    .await?;
    Ok(done.rows_affected() == 1)
}

async fn delete_expired_errored_sessions<'e, E>(
    before: DateTime<Utc>,
    executor: E,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = crate::db::Db>,
{
    #[cfg(not(feature = "postgres"))]
    let before = before.naive_utc().to_string();
    sqlx::query("DELETE FROM session_errors WHERE started < $1")
        .bind(before)
        .execute(executor)
        .await?;
    Ok(())
}

/// Apply one `session` update into state (pure, no I/O).
pub fn apply_update(
    state: &mut AggregatorState,
    project_id: i32,
    update: &SessionUpdate,
    cardinality_cap: usize,
    clock: SessionClock,
) {
    let attrs = match &update.attrs {
        Some(a) => a,
        None => {
            log::warn!("session item missing attrs, dropping");
            return;
        }
    };
    let release = match &attrs.release {
        Some(r) if !r.is_empty() && is_valid_release(r) => r.clone(),
        _ => {
            log::warn!("session item without a valid release, dropping");
            return;
        }
    };
    let environment = attrs
        .environment
        .clone()
        .filter(|e| is_valid_environment(e))
        .unwrap_or_default();

    let Some(started) = clock.read(update.started.as_deref()) else {
        log::debug!("session outside the accepted time window, dropping");
        return;
    };
    // Relay raises an earlier `timestamp` to `started`, then checks it too.
    if let Some(raw) = update.timestamp.as_deref() {
        if !clock
            .correct(raw)
            .is_some_and(|ts| clock.in_window(ts.max(started)))
        {
            log::debug!("session update stamped outside the accepted window, dropping");
            return;
        }
    }
    let bucket = truncate_to_minute(started);

    let release = apply_cardinality_cap(state, project_id, release, cardinality_cap);

    let key = BucketKey {
        project_id,
        release: release.clone(),
        environment: environment.clone(),
        bucket,
    };

    if update.init {
        state.counts.entry(key.clone()).or_default().total += 1;
    }

    let status = update.status.as_ref().cloned().unwrap_or(SessionStatus::Ok);
    let mut crashed = false;
    if status.is_terminal() {
        let entry = state.counts.entry(key.clone()).or_default();
        match classify(&status, update.errors) {
            SessionOutcome::Crashed => {
                entry.crashed += 1;
                crashed = true;
            }
            SessionOutcome::Abnormal => entry.abnormal += 1,
            SessionOutcome::Errored | SessionOutcome::Healthy => {}
        }
    }
    // Relay's `all_errors`: errors on any update, or any status but ok and
    // exited, marks the session errored once, by its id.
    if update.errors > 0 || !matches!(status, SessionStatus::Ok | SessionStatus::Exited) {
        // Relay gives a session without an id a new one.
        let sid = update
            .sid
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        // Its bucket takes a slot of the cardinality cap now; the flush
        // creates it anyway.
        state.counts.entry(key.clone()).or_default();
        state
            .errored
            .entry((project_id, sid))
            .or_insert_with(|| (key.clone(), started));
    }
    record_user(state, &key, update.did.as_deref(), crashed);
}

/// Counts `did` among the users of `key`'s day, as Relay does on every update
/// (metrics_extraction/sessions/mod.rs). Relay's `nil_to_none`: an empty or
/// nil-UUID `did` is no user.
fn record_user(state: &mut AggregatorState, key: &BucketKey, did: Option<&str>, crashed: bool) {
    let Some(did) = did.filter(|d| !d.is_empty()) else {
        return;
    };
    if uuid::Uuid::parse_str(did).is_ok_and(|id| id.is_nil()) {
        return;
    }
    let ukey = UserKey {
        project_id: key.project_id,
        release: key.release.clone(),
        environment: key.environment.clone(),
        day: key.bucket.date_naive(),
        did: did.to_string(),
    };
    *state.users.entry(ukey).or_insert(false) |= crashed;
}

/// Apply a pre-aggregated `sessions` item into state (pure, no I/O).
pub fn apply_aggregates(
    state: &mut AggregatorState,
    project_id: i32,
    agg: &SessionAggregates,
    cardinality_cap: usize,
    clock: SessionClock,
) {
    let attrs = match &agg.attrs {
        Some(a) => a,
        None => {
            log::warn!("sessions item missing attrs, dropping");
            return;
        }
    };
    let release = match &attrs.release {
        Some(r) if !r.is_empty() && is_valid_release(r) => r.clone(),
        _ => {
            log::warn!("sessions item without a valid release, dropping");
            return;
        }
    };
    let environment = attrs
        .environment
        .clone()
        .filter(|e| is_valid_environment(e))
        .unwrap_or_default();
    // Nothing in the window keeps nothing, so it takes no release slot.
    if !agg
        .aggregates
        .iter()
        .any(|item| clock.read(item.started.as_deref()).is_some())
    {
        return;
    }
    let release = apply_cardinality_cap(state, project_id, release, cardinality_cap);

    for item in &agg.aggregates {
        apply_aggregate_item(state, project_id, &release, &environment, item, clock);
    }
}

/// Apply a pre-aggregated item into state (pure, no I/O).
pub fn apply_aggregate_item(
    state: &mut AggregatorState,
    project_id: i32,
    release: &str,
    environment: &str,
    item: &SessionAggregateItem,
    clock: SessionClock,
) {
    let Some(started) = clock.read(item.started.as_deref()) else {
        log::debug!("session bucket outside the accepted time window, dropping");
        return;
    };
    let bucket = truncate_to_minute(started);

    let key = BucketKey {
        project_id,
        release: release.to_string(),
        environment: environment.to_string(),
        bucket,
    };
    record_user(state, &key, item.did.as_deref(), item.crashed > 0);
    let entry = state.counts.entry(key).or_default();
    entry.total += item.exited + item.errored + item.crashed + item.abnormal + item.unhandled;
    // Relay's `all_errors` for a bucket: errored, unhandled and the fatal
    // ones. Protocol 1.6.0 `unhandled`: the process survived, so it is not a
    // crash.
    entry.errored += item.errored + item.unhandled + item.crashed + item.abnormal;
    entry.crashed += item.crashed;
    entry.abnormal += item.abnormal;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::{SessionAttributes, SessionStatus};

    fn make_update(
        init: bool,
        status: SessionStatus,
        errors: i64,
        did: Option<&str>,
    ) -> SessionUpdate {
        SessionUpdate {
            sid: Some("sid-1".to_string()),
            did: did.map(|s| s.to_string()),
            seq: Some(0),
            init,
            started: Some("2026-06-10T10:00:00.000Z".to_string()),
            timestamp: None,
            duration: None,
            status: Some(status),
            errors,
            attrs: Some(SessionAttributes {
                release: Some("1.0.0".to_string()),
                environment: Some("production".to_string()),
            }),
        }
    }

    /// Received a few minutes after `make_update`'s `started`, by a device
    /// whose clock is right.
    fn clock() -> SessionClock {
        SessionClock::new(at("2026-06-10T10:05:00Z"), None)
    }

    fn at(s: &str) -> DateTime<Utc> {
        parse_ts(s).unwrap()
    }

    fn apply_update(state: &mut AggregatorState, project_id: i32, update: &SessionUpdate) {
        super::apply_update(state, project_id, update, 10_000, clock());
    }

    fn apply_aggregate_item(
        state: &mut AggregatorState,
        project_id: i32,
        release: &str,
        environment: &str,
        item: &SessionAggregateItem,
    ) {
        super::apply_aggregate_item(state, project_id, release, environment, item, clock());
    }

    #[test]
    fn truncate_removes_seconds() {
        use chrono::TimeZone;
        let dt = Utc.with_ymd_and_hms(2026, 6, 10, 10, 5, 42).unwrap();
        let t = truncate_to_minute(dt);
        assert_eq!(t.second(), 0);
        assert_eq!(t.minute(), 5);
    }

    #[test]
    fn init_increments_total() {
        let mut state = AggregatorState::default();
        apply_update(
            &mut state,
            1,
            &make_update(true, SessionStatus::Ok, 0, None),
        );
        let c = state.counts.values().next().unwrap();
        assert_eq!(c.total, 1);
        assert_eq!(c.crashed, 0);
    }

    #[test]
    fn heartbeat_no_double_count() {
        let mut state = AggregatorState::default();
        apply_update(
            &mut state,
            1,
            &make_update(true, SessionStatus::Ok, 0, None),
        );
        apply_update(
            &mut state,
            1,
            &make_update(false, SessionStatus::Ok, 0, None),
        );
        let c = state.counts.values().next().unwrap();
        assert_eq!(c.total, 1, "heartbeat must not increment total");
    }

    #[test]
    fn crash_increments_crashed_and_marks_user() {
        let mut state = AggregatorState::default();
        apply_update(
            &mut state,
            1,
            &make_update(true, SessionStatus::Ok, 0, Some("u1")),
        );
        apply_update(
            &mut state,
            1,
            &make_update(false, SessionStatus::Crashed, 0, Some("u1")),
        );
        let c = state.counts.values().next().unwrap();
        assert_eq!(c.crashed, 1);
        assert_eq!(c.total, 1);
        let crashed = *state.users.values().next().unwrap();
        assert!(crashed);
    }

    #[test]
    fn errored_by_errors_count() {
        let mut state = AggregatorState::default();
        apply_update(
            &mut state,
            1,
            &make_update(true, SessionStatus::Ok, 0, None),
        );
        apply_update(
            &mut state,
            1,
            &make_update(false, SessionStatus::Exited, 3, None),
        );
        // Counted at the flush, once per session id.
        assert_eq!(state.errored.len(), 1);
        assert_eq!(state.counts.values().next().unwrap().crashed, 0);
    }

    #[test]
    fn pre_aggregated_sums_correctly() {
        let mut state = AggregatorState::default();
        let item = SessionAggregateItem {
            started: Some("2026-06-10T10:00:00.000Z".to_string()),
            did: None,
            exited: 5,
            errored: 2,
            crashed: 1,
            abnormal: 0,
            unhandled: 0,
        };
        apply_aggregate_item(&mut state, 1, "2.0.0", "staging", &item);
        let c = state.counts.values().next().unwrap();
        assert_eq!(c.total, 8); // 5+2+1+0
        assert_eq!(
            c.errored, 3,
            "every errored session, the crashed one included"
        );
        assert_eq!(c.crashed, 1);
    }

    fn users(state: &AggregatorState) -> Vec<(String, bool)> {
        let mut users: Vec<_> = state
            .users
            .iter()
            .map(|(k, crashed)| (k.did.clone(), *crashed))
            .collect();
        users.sort();
        users
    }

    /// Relay records the user on every update, not only the first
    /// (metrics_extraction/sessions/mod.rs), so a user set after the session
    /// started is counted.
    #[test]
    fn the_user_is_recorded_on_any_update() {
        let mut state = AggregatorState::default();
        apply_update(
            &mut state,
            1,
            &make_update(true, SessionStatus::Ok, 0, None),
        );
        apply_update(
            &mut state,
            1,
            &make_update(false, SessionStatus::Ok, 0, Some("u1")),
        );
        assert_eq!(users(&state), vec![("u1".to_string(), false)]);
    }

    /// Relay's `nil_to_none`: a nil UUID, hyphenated or not, is no user.
    #[test]
    fn a_nil_uuid_is_no_user() {
        let mut state = AggregatorState::default();
        for did in [
            "00000000-0000-0000-0000-000000000000",
            "00000000000000000000000000000000",
        ] {
            apply_update(
                &mut state,
                1,
                &make_update(true, SessionStatus::Crashed, 0, Some(did)),
            );
        }
        assert!(users(&state).is_empty());
    }

    fn bucket_of(state: &AggregatorState) -> Vec<DateTime<Utc>> {
        state.counts.keys().map(|k| k.bucket).collect()
    }

    fn started_at(started: &str) -> SessionUpdate {
        SessionUpdate {
            started: Some(started.to_string()),
            ..make_update(true, SessionStatus::Ok, 0, None)
        }
    }

    /// Relay's `ClockDriftProcessor`: a device clock 55 minutes or more off
    /// (`received_at - sent_at`) is corrected on `started` before bucketing.
    #[test]
    fn a_drifted_device_clock_is_corrected() {
        let received = at("2026-06-10T13:00:00Z");
        let mut state = AggregatorState::default();
        let clock = SessionClock::new(received, Some("2026-06-10T10:00:00Z"));
        super::apply_update(
            &mut state,
            1,
            &started_at("2026-06-10T09:50:00Z"),
            10,
            clock,
        );
        assert_eq!(bucket_of(&state), vec![at("2026-06-10T12:50:00Z")]);

        // Under 55 minutes is not drift.
        let mut state = AggregatorState::default();
        let clock = SessionClock::new(received, Some("2026-06-10T12:10:00Z"));
        super::apply_update(
            &mut state,
            1,
            &started_at("2026-06-10T12:00:00Z"),
            10,
            clock,
        );
        assert_eq!(bucket_of(&state), vec![at("2026-06-10T12:00:00Z")]);
    }

    /// Relay drops a session that started more than 5 days before it arrived
    /// or more than a minute after, and one with no `started` at all.
    #[test]
    fn a_session_outside_relays_window_is_dropped() {
        let received = at("2026-06-10T10:00:00Z");
        let clock = SessionClock::new(received, None);
        for started in [
            Some("2026-06-05T09:59:00Z"),
            Some("2026-06-10T10:01:01Z"),
            Some("not a date"),
            None,
        ] {
            let mut state = AggregatorState::default();
            let update = SessionUpdate {
                started: started.map(str::to_string),
                ..make_update(true, SessionStatus::Ok, 0, None)
            };
            super::apply_update(&mut state, 1, &update, 10, clock);
            assert!(state.counts.is_empty(), "started {started:?} was kept");
        }
        let mut state = AggregatorState::default();
        super::apply_update(
            &mut state,
            1,
            &started_at("2026-06-05T10:01:00Z"),
            10,
            clock,
        );
        assert_eq!(state.counts.len(), 1, "4 days 23 hours old is kept");

        // Each pre-aggregated bucket on its own.
        let mut state = AggregatorState::default();
        let agg: SessionAggregates = serde_json::from_value(serde_json::json!({
            "attrs": { "release": "1.0.0" },
            "aggregates": [
                { "started": "2026-06-01T10:00:00Z", "exited": 1 },
                { "started": "2026-06-10T09:00:00Z", "exited": 1 }
            ]
        }))
        .unwrap();
        super::apply_aggregates(&mut state, 1, &agg, 10, clock);
        assert_eq!(bucket_of(&state), vec![at("2026-06-10T09:00:00Z")]);
    }

    /// Relay checks the update's `timestamp` against the same window, after
    /// raising it to `started` if it is earlier (`normalize_update`).
    #[test]
    fn an_update_stamped_outside_the_window_is_dropped() {
        let clock = SessionClock::new(at("2026-06-10T10:00:00Z"), None);
        let stamped = |timestamp: &str| SessionUpdate {
            timestamp: Some(timestamp.to_string()),
            ..started_at("2026-06-10T09:50:00Z")
        };
        for (timestamp, kept) in [
            ("2026-06-10T10:02:00Z", false),
            ("not a date", false),
            ("2026-06-10T09:55:00Z", true),
            // Earlier than `started`: raised to it, so kept.
            ("2026-06-01T00:00:00Z", true),
        ] {
            let mut state = AggregatorState::default();
            super::apply_update(&mut state, 1, &stamped(timestamp), 10, clock);
            assert_eq!(!state.counts.is_empty(), kept, "timestamp {timestamp}");
        }
    }

    /// A release reached only through an errored `ok` update still takes a
    /// slot of the cardinality cap, so the deferred error cannot add a bucket
    /// past it at the flush.
    #[test]
    fn a_deferred_error_counts_toward_the_release_cap() {
        let mut state = AggregatorState::default();
        for (sid, release) in [("s1", "1.0.0"), ("s2", "2.0.0")] {
            let update = SessionUpdate {
                sid: Some(sid.to_string()),
                attrs: Some(SessionAttributes {
                    release: Some(release.to_string()),
                    environment: None,
                }),
                ..make_update(false, SessionStatus::Ok, 1, None)
            };
            super::apply_update(&mut state, 1, &update, 1, clock());
        }
        let mut releases: Vec<&str> = state
            .errored
            .values()
            .map(|(key, _)| key.release.as_str())
            .collect();
        releases.sort();
        assert_eq!(releases, vec!["1.0.0", OVERFLOW_RELEASE]);
    }

    fn with_attrs(release: &str, environment: &str) -> (SessionUpdate, SessionAggregates) {
        let update = SessionUpdate {
            attrs: Some(SessionAttributes {
                release: Some(release.to_string()),
                environment: Some(environment.to_string()),
            }),
            ..make_update(true, SessionStatus::Ok, 0, None)
        };
        let agg = serde_json::from_value(serde_json::json!({
            "attrs": { "release": release, "environment": environment },
            "aggregates": [{ "started": "2026-06-10T10:00:00Z", "exited": 1 }]
        }))
        .unwrap();
        (update, agg)
    }

    /// A sessions item whose buckets all fall outside the accepted window
    /// keeps nothing, so it must not take a release slot from the cap.
    #[test]
    fn an_aggregate_with_nothing_in_the_window_takes_no_release_slot() {
        let stale: SessionAggregates = serde_json::from_value(serde_json::json!({
            "attrs": { "release": "stale", "environment": "production" },
            "aggregates": [{ "started": "2020-01-01T00:00:00Z", "exited": 1 }]
        }))
        .unwrap();
        let (_, fresh) = with_attrs("fresh", "production");
        let mut state = AggregatorState::default();
        super::apply_aggregates(&mut state, 1, &stale, 1, clock());
        super::apply_aggregates(&mut state, 1, &fresh, 1, clock());
        let releases: Vec<&str> = state.counts.keys().map(|k| k.release.as_str()).collect();
        assert_eq!(releases, ["fresh"]);
    }

    /// Relay drops a session whose release `validate_release` refuses
    /// (processing/sessions/process.rs:200): over 200 bytes, `.`, `..`,
    /// `latest`, or a `/`, `\` or control character.
    #[test]
    fn a_release_relay_refuses_drops_the_session() {
        let long = "a".repeat(201);
        for release in [
            long.as_str(),
            "latest",
            "LATEST",
            ".",
            "..",
            "a/b",
            "a\\b",
            "a\tb",
            "a\u{7f}",
        ] {
            let (update, agg) = with_attrs(release, "production");
            let mut state = AggregatorState::default();
            super::apply_update(&mut state, 1, &update, 10, clock());
            super::apply_aggregates(&mut state, 1, &agg, 10, clock());
            assert!(state.counts.is_empty(), "release {release:?} was kept");
        }
        let (update, _) = with_attrs(&"a".repeat(200), "production");
        let mut state = AggregatorState::default();
        super::apply_update(&mut state, 1, &update, 10, clock());
        assert_eq!(state.counts.len(), 1, "200 bytes is kept");
    }

    /// Relay keeps the session and drops only an environment
    /// `validate_environment` refuses: over 64 bytes, `.`, `..`, `none`, or a
    /// `/`, `\` or control character.
    #[test]
    fn an_environment_relay_refuses_is_dropped_from_the_session() {
        let long = "e".repeat(65);
        for environment in [long.as_str(), "none", "None", ".", "..", "a/b", "a\nb"] {
            let (update, agg) = with_attrs("1.0.0", environment);
            let mut state = AggregatorState::default();
            super::apply_update(&mut state, 1, &update, 10, clock());
            super::apply_aggregates(&mut state, 1, &agg, 10, clock());
            let environments: Vec<&str> = state
                .counts
                .keys()
                .map(|k| k.environment.as_str())
                .collect();
            assert_eq!(environments, vec![""], "environment {environment:?}");
        }
    }

    /// Relay reads `did` on a pre-aggregated bucket too.
    #[test]
    fn a_pre_aggregated_bucket_records_its_user() {
        let mut state = AggregatorState::default();
        let item: SessionAggregateItem = serde_json::from_value(serde_json::json!({
            "started": "2026-06-10T10:00:00.000Z", "did": "u1", "exited": 1, "crashed": 1
        }))
        .unwrap();
        apply_aggregate_item(&mut state, 1, "2.0.0", "staging", &item);
        assert_eq!(users(&state), vec![("u1".to_string(), true)]);
    }

    #[test]
    fn unhandled_update_counts_as_errored() {
        let mut state = AggregatorState::default();
        apply_update(
            &mut state,
            1,
            &make_update(true, SessionStatus::Ok, 0, None),
        );
        apply_update(
            &mut state,
            1,
            &make_update(false, SessionStatus::Unhandled, 1, None),
        );
        assert_eq!(state.errored.len(), 1);
        assert_eq!(state.counts.values().next().unwrap().crashed, 0);
    }

    #[test]
    fn pre_aggregated_unhandled_counts_as_errored() {
        let mut state = AggregatorState::default();
        let item = SessionAggregateItem {
            started: Some("2026-06-10T10:00:00.000Z".to_string()),
            did: None,
            exited: 5,
            errored: 1,
            crashed: 0,
            abnormal: 0,
            unhandled: 2,
        };
        apply_aggregate_item(&mut state, 1, "2.0.0", "staging", &item);
        let c = state.counts.values().next().unwrap();
        assert_eq!(c.total, 8); // 5+1+0+0+2
        assert_eq!(c.errored, 3);
        assert_eq!(c.crashed, 0);
    }

    #[test]
    fn cardinality_cap_overflows() {
        let mut state = AggregatorState::default();
        for i in 0..3usize {
            apply_cardinality_cap(&mut state, 1, format!("v{i}"), 3);
        }
        // Cap = 3; next release should overflow
        let result = apply_cardinality_cap(&mut state, 1, "v99".to_string(), 3);
        assert_eq!(result, OVERFLOW_RELEASE);
    }

    #[test]
    fn cardinality_cap_allows_existing_release() {
        let mut state = AggregatorState::default();
        apply_cardinality_cap(&mut state, 1, "v1.0.0".to_string(), 1);
        let result = apply_cardinality_cap(&mut state, 1, "v1.0.0".to_string(), 1);
        assert_eq!(result, "v1.0.0");
    }

    #[test]
    fn failed_flush_merges_counts_and_crash_flags_back_into_state() {
        let key = BucketKey {
            project_id: 1,
            release: "v1".to_string(),
            environment: "prod".to_string(),
            bucket: Utc::now(),
        };
        let user = UserKey {
            project_id: 1,
            release: "v1".to_string(),
            environment: "prod".to_string(),
            day: key.bucket.date_naive(),
            did: "did".to_string(),
        };
        let mut state = AggregatorState {
            counts: HashMap::from([(
                key.clone(),
                Counters {
                    total: 1,
                    ..Default::default()
                },
            )]),
            users: HashMap::from([(user.clone(), false)]),
            errored: HashMap::new(),
            releases: HashMap::new(),
        };
        merge_state(
            &mut state,
            HashMap::from([(
                key.clone(),
                Counters {
                    total: 2,
                    ..Default::default()
                },
            )]),
            HashMap::from([(user.clone(), true)]),
        );
        assert_eq!(state.counts[&key].total, 3);
        assert!(state.users[&user]);
        assert!(state.releases[&1].contains("v1"), "the cap still counts it");
    }
}
