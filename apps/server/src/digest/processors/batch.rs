//! The digest's write: one transaction for a batch of prepared events, and
//! the fold that turns a batch's events into one write per issue.

use super::event::{alert_type_for_lifecycle, PreparedEvent};
use super::feedback;
use crate::config::RateLimitConfig;
use crate::error::{AppError, AppResult};
use crate::models::{AlertType, Grouping, Issue, STATUS_IGNORED, STATUS_RESOLVED};
use crate::protocol::clamp;
use crate::services::event::{EventLink, EventRow};
use crate::services::rate_limit::{admit_batch, lock_quota_rows, write_quota_rows, QuotaScope};
use crate::services::{DenormalizedFields, IssueService};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

pub(crate) type DbConn = <crate::db::Db as sqlx::Database>::Connection;

/// What one event of a committed batch came to.
#[derive(Debug)]
pub(crate) enum EventOutcome {
    Inserted {
        issue_id: Uuid,
        lifecycle: Option<AlertType>,
        muted: bool,
        issue_event_count: Option<i32>,
        regressed: bool,
    },
    /// A row with its identity was already there: the replay path.
    AlreadyStored {
        issue_id: Option<Uuid>,
        alert_type: Option<AlertType>,
        issue_event_count: Option<i32>,
    },
    RateLimited(QuotaScope),
    /// A second copy of an event earlier in the same batch.
    DuplicateInBatch,
}

/// A committed batch: one outcome per event, in batch order, and every
/// issue it touched as it stands after the commit.
pub(crate) struct BatchOutcome {
    pub events: Vec<EventOutcome>,
    pub issues: HashMap<Uuid, Issue>,
}

/// One issue's state while a batch's events are folded into it: the row the
/// transaction read, then each event applied in batch order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IssueFold {
    pub id: Uuid,
    pub status: String,
    pub status_details: String,
    pub last_release: String,
    pub first_release: String,
    pub calculated_type: String,
    pub calculated_value: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub last_frame_filename: String,
    pub last_frame_module: String,
    pub last_frame_function: String,
    pub level: Option<String>,
    pub culprit: String,
    pub digested_event_count: i32,
    /// Events folded in, which the counters grow by.
    pub added: i32,
    /// Whether an event of this batch reopened it.
    pub regressed: bool,
}

impl From<PrevIssue> for IssueFold {
    fn from(prev: PrevIssue) -> Self {
        Self {
            id: prev.id,
            status: prev.status,
            status_details: prev.status_details,
            last_release: prev.last_release,
            first_release: prev.first_release,
            calculated_type: prev.calculated_type,
            calculated_value: prev.calculated_value,
            first_seen: prev.first_seen,
            last_seen: prev.last_seen,
            last_frame_filename: prev.last_frame_filename,
            last_frame_module: prev.last_frame_module,
            last_frame_function: prev.last_frame_function,
            level: prev.level,
            culprit: String::new(),
            digested_event_count: prev.digested_event_count,
            added: 0,
            regressed: false,
        }
    }
}

impl From<&Issue> for IssueFold {
    fn from(issue: &Issue) -> Self {
        Self {
            id: issue.id,
            status: issue.status.clone(),
            status_details: issue.status_details.clone(),
            last_release: issue.last_release.clone(),
            first_release: issue.first_release.clone(),
            calculated_type: issue.calculated_type.clone(),
            calculated_value: issue.calculated_value.clone(),
            first_seen: issue.first_seen,
            last_seen: issue.last_seen,
            last_frame_filename: issue.last_frame_filename.clone(),
            last_frame_module: issue.last_frame_module.clone(),
            last_frame_function: issue.last_frame_function.clone(),
            level: issue.level.clone(),
            culprit: issue.culprit.clone(),
            digested_event_count: issue.digested_event_count,
            added: 0,
            regressed: false,
        }
    }
}

/// What folding one event decided for it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Folded {
    pub regressed: bool,
    /// The count this event brought the issue to; none while it is muted.
    pub issue_event_count: Option<i32>,
    pub muted: bool,
}

/// Applies one event to its issue, by the rules each event's own update
/// always followed.
pub(crate) fn fold_event(
    state: &mut IssueFold,
    timestamp: DateTime<Utc>,
    denormalized: &DenormalizedFields,
    level: Option<&str>,
) -> Folded {
    // Detect regression: a new event for an already-resolved issue must
    // reopen it as `regressed` (mirrors Sentry's lifecycle) — unless the
    // issue was "resolved in next release" and this event is still from the
    // same release (no new deploy yet), in which case it stays resolved.
    let in_next_release = serde_json::from_str::<serde_json::Value>(&state.status_details)
        .ok()
        .and_then(|v| v.get("in_next_release").and_then(|b| b.as_bool()))
        .unwrap_or(false);
    // An event with no release metadata can't prove it's from a newer
    // release, so treat it as the same release rather than triggering a
    // spurious regression reopen.
    let same_release =
        denormalized.release.is_empty() || denormalized.release == state.last_release;
    let regressed = state.status == STATUS_RESOLVED && !(in_next_release && same_release);
    if regressed {
        state.status = crate::models::STATUS_UNRESOLVED.to_string();
        state.status_details = "{}".to_string();
        state.regressed = true;
    }

    let (calculated_type, calculated_value) = updated_title(
        (&state.calculated_type, &state.calculated_value),
        (
            &denormalized.calculated_type,
            &denormalized.calculated_value,
        ),
    );
    (state.calculated_type, state.calculated_value) =
        (calculated_type.to_string(), calculated_value.to_string());
    // An event can arrive older than the issue (clock skew across hosts, a
    // replayed envelope), so each bound moves only in its own direction.
    state.first_seen = state.first_seen.min(timestamp);
    state.last_seen = state.last_seen.max(timestamp);
    // SDKs omit `level` when it is their default, so an absent one keeps
    // whatever the issue already had rather than clearing it.
    if let Some(level) = level {
        state.level = Some(level.to_string());
    }
    state.culprit = denormalized.culprit.clone();
    // Sentry merges the incoming metadata over the existing bag, so a field
    // the new event does not carry keeps the value it had.
    let keep_if_blank = |incoming: &str, existing: &mut String| {
        if !incoming.is_empty() {
            *existing = incoming.to_string();
        }
    };
    keep_if_blank(
        &denormalized.last_frame_filename,
        &mut state.last_frame_filename,
    );
    keep_if_blank(
        &denormalized.last_frame_module,
        &mut state.last_frame_module,
    );
    keep_if_blank(
        &denormalized.last_frame_function,
        &mut state.last_frame_function,
    );
    keep_if_blank(&denormalized.release, &mut state.last_release);
    if state.first_release.is_empty() {
        state.first_release = denormalized.release.clone();
    }

    state.digested_event_count += 1;
    state.added += 1;
    let muted = state.status == STATUS_IGNORED;
    Folded {
        regressed,
        // Kept only when the event could fire new-issue rules: a muted one
        // replays its lifecycle alert alone, whatever the issue is by then.
        issue_event_count: (!muted).then_some(state.digested_event_count),
        muted,
    }
}

/// One transaction for `batch`: every row its events write, or none.
///
/// On SQLite `BEGIN IMMEDIATE` takes the write lock up front (the caller
/// already holds the in-process write turn); on Postgres a per-project
/// advisory lock serializes issue creation, then the issue rows the batch
/// touches are locked as they are read.
pub(crate) async fn commit_batch_once(
    conn: &mut DbConn,
    batch: &[PreparedEvent],
    cfg: &RateLimitConfig,
) -> AppResult<BatchOutcome> {
    #[cfg(feature = "sqlite")]
    let mut tx = sqlx::Connection::begin_with(&mut *conn, "BEGIN IMMEDIATE").await?;
    #[cfg(feature = "postgres")]
    let mut tx = sqlx::Connection::begin(&mut *conn).await?;

    let result = write_batch(&mut tx, batch, cfg).await;
    crate::failpoint!("digest.mid_transaction");

    // sqlx keeps a connection's write buffer at the largest statement it
    // ever sent and the pool never shrinks it, so ten connections could each
    // keep a maximum batch's worth until the idle timeout. Only past 256 KiB:
    // shrinking after every batch would reallocate on every digest.
    #[cfg(feature = "postgres")]
    if batch.iter().map(|e| e.data_bytes).sum::<usize>() > 256 * 1024 {
        sqlx::Connection::shrink_buffers(&mut *tx);
    }

    match result {
        Ok(outcome) => {
            tx.commit().await?;
            crate::failpoint!("digest.after_commit_before_release");
            Ok(outcome)
        }
        Err(e) => {
            tx.rollback().await?;
            Err(e)
        }
    }
}

/// [`commit_batch_once`], rolled back: what the database would make of
/// `batch`, leaving nothing behind.
pub(crate) async fn try_batch(
    conn: &mut DbConn,
    batch: &[PreparedEvent],
    cfg: &RateLimitConfig,
) -> AppResult<BatchOutcome> {
    #[cfg(feature = "sqlite")]
    let mut tx = sqlx::Connection::begin_with(&mut *conn, "BEGIN IMMEDIATE").await?;
    #[cfg(feature = "postgres")]
    let mut tx = sqlx::Connection::begin(&mut *conn).await?;
    let result = write_batch(&mut tx, batch, cfg).await;
    tx.rollback().await?;
    result
}

/// Where a grouping key leads.
#[derive(Clone, Copy)]
struct Group {
    grouping_id: i32,
    issue_id: Uuid,
}

/// A batch's writes, inside its transaction, in the order the statements
/// run: dedupe, groups, quota, the fold, then one write per table.
async fn write_batch(
    conn: &mut DbConn,
    batch: &[PreparedEvent],
    cfg: &RateLimitConfig,
) -> AppResult<BatchOutcome> {
    let mut outcomes: Vec<Option<EventOutcome>> = batch.iter().map(|_| None).collect();

    // The same spool file twice is one event: the first copy stands for it.
    let mut first_of: HashMap<(i32, Uuid), usize> = HashMap::new();
    for (i, e) in batch.iter().enumerate() {
        match first_of.entry((e.r.project_id, e.r.event_id)) {
            std::collections::hash_map::Entry::Occupied(_) => {
                outcomes[i] = Some(EventOutcome::DuplicateInBatch);
            }
            std::collections::hash_map::Entry::Vacant(first) => {
                first.insert(i);
            }
        }
    }
    let mut fresh: Vec<usize> = (0..batch.len())
        .filter(|i| outcomes[*i].is_none())
        .collect();
    let mut projects: Vec<i32> = fresh.iter().map(|i| batch[*i].r.project_id).collect();
    projects.sort_unstable();
    projects.dedup();
    let of_project = |p: i32, fresh: &[usize]| -> Vec<usize> {
        fresh
            .iter()
            .copied()
            .filter(|i| batch[*i].r.project_id == p)
            .collect()
    };

    // Ascending, so two writers take the project locks in one order.
    #[cfg(feature = "postgres")]
    for p in &projects {
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(*p as i64)
            .execute(&mut *conn)
            .await?;
    }

    // Events already stored are replays: they write nothing.
    for &p in &projects {
        let ids: Vec<Uuid> = of_project(p, &fresh)
            .iter()
            .map(|i| batch[*i].r.event_id)
            .collect();
        for stored in find_stored(conn, p, &ids).await? {
            if let Some(&i) = first_of.get(&(p, stored.event_id)) {
                outcomes[i] = Some(EventOutcome::AlreadyStored {
                    issue_id: stored.issue_id,
                    alert_type: stored.alert_type,
                    issue_event_count: stored.issue_event_count,
                });
            }
        }
    }
    fresh.retain(|i| outcomes[*i].is_none());

    // Every grouping the batch's keys name, and its issue as the fold
    // starts from it (locked on Postgres). Then, for the keys nothing is
    // under, the keys an older release would have produced.
    let mut groups: HashMap<(i32, String), Group> = HashMap::new();
    let mut legacy: HashMap<(i32, String), GroupingWithIssue> = HashMap::new();
    let mut folds: HashMap<Uuid, IssueFold> = HashMap::new();
    for &p in &projects {
        let events = of_project(p, &fresh);
        let mut hashes: Vec<&String> = events
            .iter()
            .map(|i| &batch[*i].grouping_key_hash)
            .collect();
        hashes.sort_unstable();
        hashes.dedup();
        if hashes.is_empty() {
            continue;
        }
        for row in find_groups(conn, p, &hashes).await? {
            let group = Group {
                grouping_id: row.grouping.id,
                issue_id: row.grouping.issue_id,
            };
            groups.insert((p, row.grouping.grouping_key_hash.clone()), group);
            folds
                .entry(group.issue_id)
                .or_insert_with(|| IssueFold::from(row.issue));
        }
        let mut misses: Vec<&String> = events
            .iter()
            .map(|i| &batch[*i])
            .filter(|e| {
                e.legacy_grouping_key_hash != e.grouping_key_hash
                    && !groups.contains_key(&(p, e.grouping_key_hash.clone()))
            })
            .map(|e| &e.legacy_grouping_key_hash)
            .collect();
        misses.sort_unstable();
        misses.dedup();
        if !misses.is_empty() {
            for row in find_groups(conn, p, &misses).await? {
                legacy.insert((p, row.grouping.grouping_key_hash.clone()), row);
            }
        }
    }

    // The quota, decided once for the batch in batch order.
    let mut quota = None;
    let mut admitted = Vec::new();
    if !fresh.is_empty() {
        let mut rows = lock_quota_rows(conn, &projects).await?;
        let scopes = admit_batch(
            &mut rows,
            cfg,
            Utc::now(),
            fresh.iter().map(|i| batch[*i].r.project_id),
        );
        for (&i, scope) in fresh.iter().zip(scopes) {
            match scope {
                Some(scope) => outcomes[i] = Some(EventOutcome::RateLimited(scope)),
                None => admitted.push(i),
            }
        }
        quota = Some(rows);
    }

    // Each admitted event's issue, in batch order: found, claimed from an
    // older key, or created by it. Then the event folds into it.
    let mut created: HashMap<Uuid, Issue> = HashMap::new();
    let mut links: Vec<(usize, EventLink)> = Vec::with_capacity(admitted.len());
    for &i in &admitted {
        let e = &batch[i];
        let p = e.r.project_id;
        let key = (p, e.grouping_key_hash.clone());
        let mut is_new = false;
        let group = match groups.get(&key) {
            Some(group) => *group,
            None => {
                let legacy_key = (p, e.legacy_grouping_key_hash.clone());
                let claimed = if e.legacy_grouping_key_hash == e.grouping_key_hash {
                    None
                } else if let Some(group) = groups.get(&legacy_key) {
                    Some(group.issue_id)
                } else if let Some(row) = legacy.remove(&legacy_key) {
                    let issue_id = row.grouping.issue_id;
                    // Another key of this batch may fall back to it too.
                    groups.insert(
                        legacy_key,
                        Group {
                            grouping_id: row.grouping.id,
                            issue_id,
                        },
                    );
                    folds
                        .entry(issue_id)
                        .or_insert_with(|| IssueFold::from(row.issue));
                    Some(issue_id)
                } else {
                    None
                };
                let group = match claimed {
                    // Nothing under the current key: the issue predates a
                    // change to how the key is built. Claim it, and record the
                    // current key so later events resolve directly.
                    Some(issue_id) => {
                        let grouping = insert_grouping(
                            conn,
                            p,
                            issue_id,
                            &e.grouping_key,
                            &e.grouping_key_hash,
                        )
                        .await?;
                        Group {
                            grouping_id: grouping.id,
                            issue_id,
                        }
                    }
                    None => {
                        let (issue, grouping) = create_group(conn, e).await?;
                        is_new = true;
                        let group = Group {
                            grouping_id: grouping.id,
                            issue_id: issue.id,
                        };
                        folds.insert(issue.id, IssueFold::from(&issue));
                        created.insert(issue.id, issue);
                        group
                    }
                };
                groups.insert(key, group);
                group
            }
        };

        let folded = if is_new {
            // The issue was created from this event, counter and all.
            Folded {
                regressed: false,
                issue_event_count: Some(1),
                muted: false,
            }
        } else {
            let state = folds
                .get_mut(&group.issue_id)
                .ok_or_else(|| AppError::Internal("an issue the batch did not read".into()))?;
            fold_event(state, e.metadata.ingested_at, &e.denormalized, e.level)
        };
        // No digest_order to compute: events order within an issue by
        // (timestamp, id) (see idx_events_issue_timestamp).
        let lifecycle = alert_type_for_lifecycle(is_new, folded.regressed, e.feedback.is_some());
        links.push((
            i,
            EventLink {
                id: Uuid::new_v4(),
                issue_id: group.issue_id,
                grouping_id: group.grouping_id,
                alert_type: lifecycle,
                issue_event_count: folded.issue_event_count,
            },
        ));
        outcomes[i] = Some(EventOutcome::Inserted {
            issue_id: group.issue_id,
            lifecycle,
            muted: folded.muted,
            issue_event_count: folded.issue_event_count,
            regressed: folded.regressed,
        });
    }

    if !links.is_empty() {
        let mut inserted =
            EventRow::insert_all(conn, links.iter().map(|(i, link)| (&batch[*i].row, link)))
                .await?;
        let mut expected: Vec<(i32, Uuid)> = links
            .iter()
            .map(|(i, _)| (batch[*i].r.project_id, batch[*i].r.event_id))
            .collect();
        inserted.sort_unstable();
        expected.sort_unstable();
        // Impossible under the locks the dedupe ran in; checked, so every
        // count below is of rows actually inserted.
        if inserted != expected {
            return Err(AppError::Internal(
                "events present that the dedupe did not see".to_string(),
            ));
        }
    }

    // Each touched issue once, in id order. One created by its only event
    // is already as that event left it.
    let mut issues: HashMap<Uuid, Issue> = HashMap::new();
    let mut touched: Vec<&IssueFold> = folds.values().filter(|f| f.added > 0).collect();
    touched.sort_unstable_by_key(|f| f.id);
    for fold in &touched {
        let issue = write_issue(conn, fold).await?;
        issues.insert(issue.id, issue);
    }
    for (id, issue) in created {
        issues.entry(id).or_insert(issue);
    }

    upsert_environments(conn, &environments(batch, &links)).await?;

    // An event claims the user reports that arrived before it; a feedback
    // records itself against the issue of the event it is about.
    for &p in &projects {
        let claimed: Vec<Uuid> = links
            .iter()
            .map(|(i, _)| &batch[*i])
            .filter(|e| e.r.project_id == p && e.feedback.is_none())
            .map(|e| e.r.event_id)
            .collect();
        claim_user_reports(conn, p, &claimed).await?;
    }
    for (i, link) in &links {
        link_feedback(conn, &batch[*i], link.issue_id).await?;
        insert_attachments(conn, &batch[*i], link.id).await?;
    }
    for fold in touched.iter().filter(|f| f.regressed) {
        regression_activity(conn, fold.id).await?;
    }
    if let Some(rows) = &quota {
        write_quota_rows(conn, rows).await?;
    }

    let events = outcomes
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| AppError::Internal("an event the batch did not decide".into()))?;
    Ok(BatchOutcome { events, issues })
}

/// The admitted events per issue and environment: where each issue has been
/// seen, summed as one row each.
fn environments<'a>(
    batch: &'a [PreparedEvent],
    links: &[(usize, EventLink)],
) -> Vec<EnvironmentRow<'a>> {
    let mut rows: Vec<EnvironmentRow<'a>> = Vec::new();
    let mut at: HashMap<(Uuid, &str), usize> = HashMap::new();
    for (i, link) in links {
        let e = &batch[*i];
        if e.environment.is_empty() {
            continue;
        }
        let seen = e.metadata.ingested_at;
        let release = e.denormalized.release.as_str();
        match at.get(&(link.issue_id, e.environment.as_str())) {
            Some(&row) => {
                let row = &mut rows[row];
                row.first_seen = row.first_seen.min(seen);
                row.last_seen = row.last_seen.max(seen);
                if row.first_release.is_empty() {
                    row.first_release = release;
                }
                row.event_count += 1;
            }
            None => {
                at.insert((link.issue_id, e.environment.as_str()), rows.len());
                rows.push(EnvironmentRow {
                    issue_id: link.issue_id,
                    project_id: e.r.project_id,
                    environment: &e.environment,
                    first_seen: seen,
                    last_seen: seen,
                    first_release: release,
                    event_count: 1,
                });
            }
        }
    }
    rows
}

/// The columns of an already stored event its replay needs.
#[derive(sqlx::FromRow)]
struct StoredEvent {
    event_id: Uuid,
    issue_id: Option<Uuid>,
    alert_type: Option<AlertType>,
    issue_event_count: Option<i32>,
}

/// The events of `project_id` among `event_ids` that are already stored.
async fn find_stored(
    conn: &mut DbConn,
    project_id: i32,
    event_ids: &[Uuid],
) -> AppResult<Vec<StoredEvent>> {
    let mut select = sqlx::QueryBuilder::<crate::db::Db>::new(
        "SELECT event_id, issue_id, alert_type, issue_event_count FROM events WHERE project_id = ",
    );
    select.push_bind(project_id).push(" AND event_id IN (");
    let mut ids = select.separated(", ");
    for id in event_ids {
        ids.push_bind(*id);
    }
    select.push(")");
    Ok(select
        .build_query_as::<StoredEvent>()
        .fetch_all(&mut *conn)
        .await?)
}

/// The groupings of `project_id` under `hashes`, each with the issue
/// columns the fold starts from.
async fn find_groups(
    conn: &mut DbConn,
    project_id: i32,
    hashes: &[&String],
) -> AppResult<Vec<GroupingWithIssue>> {
    // The grouping and the issue columns the update path reads, in one round
    // trip: on SQLite every statement is a hand-off to the connection's
    // worker thread, and this one runs under the write lock.
    let mut select = sqlx::QueryBuilder::<crate::db::Db>::new(
        "SELECT g.id, g.project_id, g.issue_id, g.grouping_key, g.grouping_key_hash, \
                g.created_at, i.status, i.status_details, i.last_release, i.first_release, \
                i.calculated_type, i.calculated_value, i.first_seen, i.last_seen, \
                i.last_frame_filename, i.last_frame_module, i.last_frame_function, i.level, \
                i.digested_event_count \
         FROM groupings g JOIN issues i ON i.id = g.issue_id \
         WHERE g.project_id = ",
    );
    select
        .push_bind(project_id)
        .push(" AND g.grouping_key_hash IN (");
    let mut list = select.separated(", ");
    for hash in hashes {
        list.push_bind(hash.as_str());
    }
    select.push(")");
    // The issue rows are locked as they are read, in id order (the lock
    // sits above the sort), so an API write either committed before this
    // read and is folded in, or waits for the commit and applies on top.
    #[cfg(feature = "postgres")]
    select.push(" ORDER BY i.id FOR NO KEY UPDATE OF i");
    Ok(select
        .build_query_as::<GroupingWithIssue>()
        .fetch_all(&mut *conn)
        .await?)
}

async fn insert_grouping(
    conn: &mut DbConn,
    project_id: i32,
    issue_id: Uuid,
    grouping_key: &str,
    grouping_key_hash: &str,
) -> AppResult<Grouping> {
    Ok(sqlx::query_as(
        r#"
        INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(project_id)
    .bind(issue_id)
    .bind(grouping_key)
    .bind(grouping_key_hash)
    .fetch_one(&mut *conn)
    .await?)
}

/// A new issue for `e`'s first event and the grouping that leads to it.
async fn create_group(conn: &mut DbConn, e: &PreparedEvent) -> AppResult<(Issue, Grouping)> {
    let issue = IssueService::create_in(
        conn,
        e.r.project_id,
        e.metadata.ingested_at,
        &e.denormalized,
        e.level,
        e.platform.as_deref(),
        match e.feedback {
            Some(_) => feedback::CATEGORY,
            None => "error",
        },
    )
    .await?;
    let grouping = insert_grouping(
        conn,
        e.r.project_id,
        issue.id,
        &e.grouping_key,
        &e.grouping_key_hash,
    )
    .await?;
    Ok((issue, grouping))
}

/// Writes a folded issue's state. A regression reopens it.
async fn write_issue(conn: &mut DbConn, f: &IssueFold) -> AppResult<Issue> {
    Ok(sqlx::query_as::<_, Issue>(
        r#"
        UPDATE issues
        SET first_seen = $2,
            last_seen = $3,
            digested_event_count = digested_event_count + $4,
            stored_event_count = stored_event_count + $4,
            calculated_type = $5,
            calculated_value = $6,
            level = $7,
            culprit = $8,
            last_frame_filename = $9,
            last_frame_module = $10,
            last_frame_function = $11,
            status = CASE WHEN $12 THEN 'unresolved' ELSE status END,
            substatus = CASE WHEN $12 THEN 'regressed' ELSE substatus END,
            status_details = CASE WHEN $12 THEN '{}' ELSE status_details END,
            last_release = $13,
            first_release = $14
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(f.id)
    .bind(f.first_seen)
    .bind(f.last_seen)
    .bind(f.added)
    .bind(&f.calculated_type)
    .bind(&f.calculated_value)
    .bind(&f.level)
    .bind(&f.culprit)
    .bind(&f.last_frame_filename)
    .bind(&f.last_frame_module)
    .bind(&f.last_frame_function)
    .bind(f.regressed)
    .bind(&f.last_release)
    .bind(&f.first_release)
    .fetch_one(&mut *conn)
    .await?)
}

async fn regression_activity(conn: &mut DbConn, issue_id: Uuid) -> AppResult<()> {
    crate::services::IssueSocialService::add_activity_on(
        conn,
        issue_id,
        None,
        "set_regression",
        &serde_json::json!({ "status": crate::models::STATUS_UNRESOLVED }).to_string(),
    )
    .await
}

/// One issue's events in one environment.
struct EnvironmentRow<'a> {
    issue_id: Uuid,
    project_id: i32,
    environment: &'a str,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    first_release: &'a str,
    event_count: i32,
}

/// Where each issue has been seen, as Sentry's `GroupEnvironment`
/// (`event_manager.py`, `_get_or_create_group_environment`), with the
/// numbers Sentry reads from Snuba for it.
async fn upsert_environments(conn: &mut DbConn, rows: &[EnvironmentRow<'_>]) -> AppResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut upsert = sqlx::QueryBuilder::<crate::db::Db>::new(
        "INSERT INTO issue_environments \
         (issue_id, project_id, environment, first_seen, last_seen, first_release, event_count) ",
    );
    upsert.push_values(rows, |mut b, row| {
        b.push_bind(row.issue_id)
            .push_bind(row.project_id)
            .push_bind(row.environment)
            .push_bind(row.first_seen)
            .push_bind(row.last_seen)
            .push_bind(row.first_release)
            .push_bind(row.event_count);
    });
    // `first_release` as the issue's own: the first one sent, kept, so a
    // release's new issues there outlive its events.
    upsert.push(
        " ON CONFLICT (issue_id, environment) DO UPDATE \
         SET first_seen = CASE WHEN excluded.first_seen < issue_environments.first_seen \
                               THEN excluded.first_seen ELSE issue_environments.first_seen END, \
             last_seen = CASE WHEN excluded.last_seen > issue_environments.last_seen \
                              THEN excluded.last_seen ELSE issue_environments.last_seen END, \
             event_count = issue_environments.event_count + excluded.event_count, \
             first_release = CASE WHEN issue_environments.first_release = '' \
                                  THEN excluded.first_release ELSE issue_environments.first_release END",
    );
    upsert.build().execute(&mut *conn).await?;
    Ok(())
}

async fn insert_attachments(conn: &mut DbConn, e: &PreparedEvent, row_id: Uuid) -> AppResult<()> {
    for attachment in &e.attachments {
        sqlx::query(
            "INSERT INTO event_attachments (id, project_id, event_id, filename, content_type, size, data, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(Uuid::new_v4())
        .bind(e.r.project_id)
        .bind(row_id)
        .bind(&attachment.filename)
        .bind(&attachment.content_type)
        .bind(attachment.data.len() as i32)
        .bind(&attachment.data)
        .bind(e.metadata.ingested_at)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Keeps `user_reports` (what an error issue shows as its feedback) in step
/// with the digest, as Sentry's post-process `link_event_to_user_report`
/// does: each of `event_ids` (stored in this transaction) claims the
/// feedback about it that arrived before it.
async fn claim_user_reports(
    conn: &mut DbConn,
    project_id: i32,
    event_ids: &[Uuid],
) -> AppResult<()> {
    if event_ids.is_empty() {
        return Ok(());
    }
    let mut claim = sqlx::QueryBuilder::<crate::db::Db>::new(
        "UPDATE user_reports SET issue_id = (SELECT e.issue_id FROM events e \
         WHERE e.project_id = user_reports.project_id AND e.event_id = user_reports.event_id) \
         WHERE project_id = ",
    );
    claim
        .push_bind(project_id)
        .push(" AND issue_id IS NULL AND event_id IN (");
    let mut ids = claim.separated(", ");
    for id in event_ids {
        ids.push_bind(*id);
    }
    claim.push(")");
    claim.build().execute(&mut *conn).await?;
    Ok(())
}

/// A feedback about an event records itself against that event's issue, as
/// Sentry's `save_event_feedback` does.
async fn link_feedback(conn: &mut DbConn, e: &PreparedEvent, issue_id: Uuid) -> AppResult<()> {
    let Some(feedback) = &e.feedback else {
        return Ok(());
    };
    let Some(event_id) = feedback.associated_event_id else {
        return Ok(());
    };
    let error_issue: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT issue_id FROM events WHERE project_id = $1 AND event_id = $2")
            .bind(e.r.project_id)
            .bind(event_id)
            .fetch_optional(&mut *conn)
            .await?;
    sqlx::query(
        "INSERT INTO user_reports (id, project_id, issue_id, feedback_issue_id, event_id, name, email, comments, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::new_v4())
    .bind(e.r.project_id)
    .bind(error_issue.flatten())
    // Deleting the feedback deletes this copy of it.
    .bind(issue_id)
    .bind(event_id)
    // Cut to the VARCHAR(255) columns, which Postgres enforces.
    .bind(clamp(&feedback.name, 255))
    .bind(clamp(&feedback.contact_email, 255))
    .bind(&feedback.message)
    .bind(e.metadata.ingested_at)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A title that says nothing about the event. Mirrors Sentry's
/// `PLACEHOLDER_EVENT_TITLES` in Rustrak's vocabulary: "Unknown" is what an
/// event with neither exception nor message produces, "Error" an exception
/// carrying neither type nor value.
fn is_placeholder_title(calculated_type: &str, calculated_value: &str) -> bool {
    calculated_value.trim().is_empty() && matches!(calculated_type, "Unknown" | "Error")
}

/// Picks the type and value an issue should carry after a new event. The
/// incoming pair wins unless it is a placeholder about to replace a real title.
fn updated_title<'a>(
    existing: (&'a str, &'a str),
    incoming: (&'a str, &'a str),
) -> (&'a str, &'a str) {
    if is_placeholder_title(incoming.0, incoming.1) && !is_placeholder_title(existing.0, existing.1)
    {
        existing
    } else {
        incoming
    }
}

/// A grouping joined to the issue columns the fold starts from.
#[derive(sqlx::FromRow)]
struct GroupingWithIssue {
    #[sqlx(flatten)]
    grouping: Grouping,
    #[sqlx(flatten)]
    issue: PrevIssue,
}

/// The issue columns the fold reads before deciding what to write.
#[derive(sqlx::FromRow)]
pub(crate) struct PrevIssue {
    #[sqlx(rename = "issue_id")]
    id: Uuid,
    status: String,
    status_details: String,
    last_release: String,
    first_release: String,
    calculated_type: String,
    calculated_value: String,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    last_frame_filename: String,
    last_frame_module: String,
    last_frame_function: String,
    level: Option<String>,
    digested_event_count: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::DenormalizedFields;
    use chrono::{DateTime, TimeZone, Utc};

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + seconds, 0).unwrap()
    }

    fn fields(
        (calculated_type, calculated_value): (&str, &str),
        release: &str,
        culprit: &str,
        (file, module, function): (&str, &str, &str),
    ) -> DenormalizedFields {
        DenormalizedFields {
            calculated_type: calculated_type.into(),
            calculated_value: calculated_value.into(),
            transaction: String::new(),
            last_frame_filename: file.into(),
            last_frame_module: module.into(),
            last_frame_function: function.into(),
            culprit: culprit.into(),
            logger: String::new(),
            release: release.into(),
        }
    }

    fn resolved(status_details: &str) -> IssueFold {
        IssueFold {
            id: uuid::Uuid::nil(),
            status: "resolved".into(),
            status_details: status_details.into(),
            last_release: "1.0".into(),
            first_release: "1.0".into(),
            calculated_type: "TypeError".into(),
            calculated_value: "boom".into(),
            first_seen: at(10),
            last_seen: at(20),
            last_frame_filename: "a.rs".into(),
            last_frame_module: "m".into(),
            last_frame_function: "f".into(),
            level: Some("error".into()),
            culprit: String::new(),
            digested_event_count: 5,
            added: 0,
            regressed: false,
        }
    }

    /// Three events folded in order leave the issue where three sequential
    /// per-event updates would: each field follows today's rule, the count
    /// each event brought the issue to is its own, and only the first event
    /// of a resolved issue regresses it.
    #[test]
    fn fold_matches_sequential_updates() {
        let mut state = resolved("{}");
        let a = fields(("Unknown", ""), "", "ca", ("", "", ""));
        let b = fields(("ValueError", "bad"), "2.0", "cb", ("b.rs", "", "g"));
        let c = fields(("Unknown", ""), "2.1", "cc", ("", "", ""));

        let folded = [
            fold_event(&mut state, at(5), &a, None),
            fold_event(&mut state, at(30), &b, Some("warning")),
            fold_event(&mut state, at(25), &c, None),
        ];

        assert_eq!(
            folded.map(|f| (f.regressed, f.issue_event_count, f.muted)),
            [
                (true, Some(6), false),
                (false, Some(7), false),
                (false, Some(8), false)
            ]
        );
        let mut expected = resolved("{}");
        expected.status = "unresolved".into();
        expected.status_details = "{}".into();
        expected.last_release = "2.1".into();
        expected.calculated_type = "ValueError".into();
        expected.calculated_value = "bad".into();
        expected.first_seen = at(5);
        expected.last_seen = at(30);
        expected.last_frame_filename = "b.rs".into();
        expected.last_frame_function = "g".into();
        expected.level = Some("warning".into());
        expected.culprit = "cc".into();
        expected.digested_event_count = 8;
        expected.added = 3;
        expected.regressed = true;
        assert_eq!(state, expected);
    }

    /// "Resolved in next release" holds against events of the same release
    /// or of none, and gives way to the first event of another.
    #[test]
    fn in_next_release_suppresses_until_a_new_release() {
        let mut state = resolved(r#"{"in_next_release":true}"#);
        let same = fields(("TypeError", "boom"), "1.0", "c", ("", "", ""));
        let none = fields(("TypeError", "boom"), "", "c", ("", "", ""));
        let newer = fields(("TypeError", "boom"), "2.0", "c", ("", "", ""));

        assert!(!fold_event(&mut state, at(21), &same, None).regressed);
        assert!(!fold_event(&mut state, at(22), &none, None).regressed);
        assert_eq!(state.status, "resolved");
        assert!(fold_event(&mut state, at(23), &newer, None).regressed);
        assert_eq!(
            (state.status.as_str(), state.status_details.as_str()),
            ("unresolved", "{}")
        );
    }

    /// The incoming title wins unless it is a placeholder ("Unknown", or
    /// "Error" with no value) about to replace a real one.
    #[test]
    fn a_placeholder_never_replaces_a_real_title() {
        let boom = ("TypeError", "boom");
        for (existing, incoming, kept) in [
            (("Log Message", "old"), boom, boom),
            (("Unknown", ""), boom, boom),
            (boom, ("Unknown", ""), boom),
            (boom, ("Error", ""), boom),
            (("Unknown", ""), ("Error", ""), ("Error", "")),
            (boom, ("Error", "reset"), ("Error", "reset")),
        ] {
            assert_eq!(updated_title(existing, incoming), kept, "{incoming:?}");
        }
    }

    /// A muted issue keeps counting but records no count on its events, and
    /// an issue with no first release takes the first one sent.
    #[test]
    fn a_muted_issue_records_no_event_count() {
        let mut state = resolved("{}");
        state.status = "ignored".into();
        state.first_release = String::new();
        let e = fields(("TypeError", "boom"), "3.0", "c", ("", "", ""));

        let folded = fold_event(&mut state, at(21), &e, None);

        assert_eq!(
            (folded.regressed, folded.issue_event_count, folded.muted),
            (false, None, true)
        );
        assert_eq!(state.digested_event_count, 6);
        assert_eq!(state.first_release, "3.0");
    }
}
