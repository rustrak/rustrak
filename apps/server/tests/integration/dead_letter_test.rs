//! The recovery worker against a spool file that will never digest: after a
//! few attempts it is moved to `dead/` (never deleted) and stops holding a
//! pending slot. A database outage, however long, never counts.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::common::{create_test_project, TestDb};
use chrono::Utc;
use rustrak::digest::processors::Processors;
use rustrak::error::AppResult;
use rustrak::ingest::pipeline::recover_pending_events_once;
use rustrak::ingest::{store_event_with_metadata, EventMetadata};
use rustrak::services::sourcemap::{SourceMapEntry, SourceMapProvider};
use rustrak::telemetry::Counters;
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

/// More passes than any attempt budget could reasonably allow.
const PASSES: usize = 12;

struct Spool {
    dir: tempfile::TempDir,
    processors: Arc<Processors>,
    counters: &'static Counters,
}

impl Spool {
    fn new(pool: &rustrak::db::DbPool) -> Self {
        Self::with_provider(pool, crate::common::null_sourcemap_provider())
    }

    fn with_provider(pool: &rustrak::db::DbPool, provider: Arc<dyn SourceMapProvider>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let counters: &'static Counters = Box::leak(Box::new(Counters::new()));
        let processors = Arc::new(
            Processors::new(
                &crate::common::config_with_ingest_dir(dir.path()),
                provider,
                rustrak::digest::session_aggregator::SessionAggregator::new(
                    pool.clone(),
                    30,
                    10_000,
                ),
            )
            .with_counters(counters),
        );
        Self {
            dir,
            processors,
            counters,
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// An accepted event, pending as the startup scan would find it.
    async fn accept(&self, project_id: i32) -> (Uuid, PathBuf) {
        self.accept_message(project_id, "dead letter proof").await
    }

    /// [`Self::accept`], with its own message.
    async fn accept_message(&self, project_id: i32, message: &str) -> (Uuid, PathBuf) {
        self.accept_event(
            project_id,
            serde_json::json!({"level": "error", "message": message}),
        )
        .await
    }

    /// [`Self::accept`], of `event` under a new id.
    async fn accept_event(&self, project_id: i32, mut event: serde_json::Value) -> (Uuid, PathBuf) {
        let event_id = Uuid::new_v4();
        let metadata = EventMetadata {
            event_id: event_id.simple().to_string(),
            project_id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        event["event_id"] = metadata.event_id.clone().into();
        store_event_with_metadata(
            self.path(),
            &metadata.event_id,
            &serde_json::to_vec(&event).unwrap(),
            &metadata,
        )
        .await
        .unwrap();
        self.processors
            .errors
            .pending()
            .insert(project_id, event_id);
        (event_id, self.file(project_id, event_id))
    }

    /// `event`, accepted under a new id and queued for the live digest, as
    /// ingest does. Its spool file.
    async fn enqueue(
        &self,
        pool: &rustrak::db::DbPool,
        project_id: i32,
        mut event: serde_json::Value,
    ) -> PathBuf {
        let event_id = Uuid::new_v4();
        event["event_id"] = event_id.simple().to_string().into();
        let metadata = EventMetadata {
            event_id: event_id.simple().to_string(),
            project_id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        rustrak::ingest::pipeline::enqueue_digest(
            &self.processors,
            pool,
            event_id,
            metadata,
            event.to_string().into(),
        )
        .await
        .unwrap();
        self.file(project_id, event_id)
    }

    fn file(&self, project_id: i32, event_id: Uuid) -> PathBuf {
        self.path().join(format!(
            "project-{project_id}-{}.pending.json",
            event_id.simple()
        ))
    }

    fn dead(&self, file: &Path) -> PathBuf {
        self.path().join("dead").join(file.file_name().unwrap())
    }

    async fn recover(&self, pool: &rustrak::db::DbPool) -> bool {
        recover_pending_events_once(
            pool.clone(),
            Arc::clone(&self.processors),
            self.path().to_path_buf(),
        )
        .await
    }

    fn pending(&self, project_id: i32) -> i64 {
        self.processors
            .errors
            .pending()
            .counts(project_id)
            .installation
    }

    /// The transactions the digest has run.
    fn batches(&self) -> u64 {
        self.counters
            .render(Default::default(), None)
            .lines()
            .find_map(|l| l.strip_prefix("rustrak_digest_batches_total "))
            .unwrap()
            .parse()
            .unwrap()
    }

    fn attempts(&self, project_id: i32, event_id: Uuid) -> u32 {
        self.processors
            .errors
            .failed_attempts_for_test(project_id, event_id)
    }

    fn dead_lettered(&self) -> String {
        self.counters
            .render(Default::default(), None)
            .lines()
            .find(|l| l.starts_with("rustrak_spool_dead_lettered_total "))
            .unwrap()
            .to_string()
    }
}

/// A database that refuses this event's row the same way every time: a
/// constraint violation, the kind of error one payload causes.
async fn refuse_every_event_row(pool: &rustrak::db::DbPool) {
    #[cfg(feature = "sqlite")]
    let refuse =
        "CREATE TRIGGER refuse BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'refused'); END";
    #[cfg(feature = "postgres")]
    let refuse = "ALTER TABLE events ADD CONSTRAINT refuse CHECK (false) NOT VALID";
    sqlx::query(refuse).execute(pool).await.unwrap();
}

#[actix_web::test]
async fn a_deterministic_failure_is_dead_lettered_after_a_few_attempts() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Dead Letter").await;
    let spool = Spool::new(&db.pool);
    let (_, file) = spool.accept(project.id).await;
    refuse_every_event_row(&db.pool).await;

    assert!(spool.recover(&db.pool).await, "one failure is retried");
    assert!(file.exists(), "not dead after one attempt");

    for _ in 0..PASSES {
        spool.recover(&db.pool).await;
    }

    assert!(!file.exists());
    assert!(spool.dead(&file).exists(), "moved, never deleted");
    assert_eq!(spool.pending(project.id), 0, "its pending slot is released");
    assert_eq!(spool.dead_lettered(), "rustrak_spool_dead_lettered_total 1");
    assert!(!spool.recover(&db.pool).await, "nothing left to retry");
}

/// A missing table (a SQL bug in a release, a revoked grant) fails every
/// event alike: it is retried until fixed, never dead-letters the spool.
#[actix_web::test]
async fn a_query_bug_never_dead_letters_the_backlog() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Query Bug").await;
    let spool = Spool::new(&db.pool);
    let (_, file) = spool.accept(project.id).await;
    sqlx::query("ALTER TABLE events RENAME TO events_gone")
        .execute(&db.pool)
        .await
        .unwrap();

    for _ in 0..PASSES {
        assert!(spool.recover(&db.pool).await, "still queued");
    }

    assert!(file.exists());
    assert!(!spool.path().join("dead").exists());
    assert_eq!(spool.pending(project.id), 1);
    assert_eq!(spool.dead_lettered(), "rustrak_spool_dead_lettered_total 0");
}

/// Pool timeouts, a closed pool, a database that is down: none of it says
/// anything about the event, so none of it counts.
#[actix_web::test]
async fn a_database_outage_never_dead_letters_the_backlog() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Outage").await;
    let spool = Spool::new(&db.pool);
    let (_, file) = spool.accept(project.id).await;
    db.pool.close().await;

    for _ in 0..PASSES {
        assert!(spool.recover(&db.pool).await, "still queued");
    }

    assert!(file.exists());
    assert!(!spool.path().join("dead").exists());
    assert_eq!(spool.pending(project.id), 1);
    assert_eq!(spool.dead_lettered(), "rustrak_spool_dead_lettered_total 0");
}

/// A spool file the digest cannot read (EMFILE, EIO, here EISDIR) says
/// nothing about the event: it is retried, never dead-lettered.
#[actix_web::test]
async fn a_spool_read_error_never_dead_letters() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Read Error").await;
    let spool = Spool::new(&db.pool);
    let event_id = Uuid::new_v4();
    let file = spool.file(project.id, event_id);
    std::fs::create_dir(&file).unwrap();
    let metadata = EventMetadata {
        event_id: event_id.simple().to_string(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };

    for _ in 0..PASSES {
        let result = spool.processors.errors.digest(&db.pool, &metadata).await;
        assert!(result.is_err(), "{result:?}");
    }

    assert!(file.exists());
    assert!(!spool.path().join("dead").exists());
}

/// A side effect after the commit (here the alert rules query) broken by a
/// query bug keeps the stored event queued, never in `dead/`, and the replay
/// once it is fixed runs the step and lets the file leave the spool.
///
/// The event stops counting as pending from its commit on: it is in the
/// quota's stored counters, so the quota check would count it twice.
#[actix_web::test]
async fn a_post_commit_failure_does_not_dead_letter_a_stored_event() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Post Commit").await;
    let spool = Spool::new(&db.pool);
    let (event_id, file) = spool.accept(project.id).await;
    sqlx::query("ALTER TABLE alert_rules RENAME TO alert_rules_gone")
        .execute(&db.pool)
        .await
        .unwrap();

    spool.recover(&db.pool).await;
    assert!(file.exists(), "its side effects are still to run");
    assert_eq!(spool.pending(project.id), 0, "committed, so not pending");
    for _ in 0..PASSES {
        spool.recover(&db.pool).await;
    }
    assert!(file.exists(), "the alert step is retried");
    assert!(
        !spool.dead(&file).exists(),
        "stored, so never dead-lettered"
    );

    sqlx::query("ALTER TABLE alert_rules_gone RENAME TO alert_rules")
        .execute(&db.pool)
        .await
        .unwrap();
    spool.recover(&db.pool).await;
    crate::common::eventually(|| async { (!file.exists()).then_some(()) }).await;
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_id = $1")
        .bind(event_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(stored, 1);
    assert_eq!(spool.pending(project.id), 0);
}

/// The live digest never opens a closed connection either: its event waits
/// in the spool, retried as an outage until the drain lets it go.
#[actix_web::test]
async fn a_closed_committer_is_not_reopened_by_the_live_digest() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Closed Live").await;
    let spool = Spool::new(&db.pool);
    spool.processors.errors.close_committer().await;

    let file = spool
        .enqueue(
            &db.pool,
            project.id,
            serde_json::json!({"message": "after close"}),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert!(file.exists());
    assert_eq!(events_stored(&db.pool).await, 0);
    spool.processors.errors.start_draining();
}

/// Something outside the digest holding what its next batch takes first:
/// SQLite's write lock, or Postgres's advisory lock on the project.
struct Blocker(<rustrak::db::Db as sqlx::Database>::Connection);

impl Blocker {
    async fn hold(pool: &rustrak::db::DbPool, project_id: i32) -> Self {
        let mut conn = sqlx::ConnectOptions::connect(pool.connect_options().as_ref())
            .await
            .unwrap();
        #[cfg(feature = "sqlite")]
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut conn)
            .await
            .unwrap();
        #[cfg(feature = "postgres")]
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(i64::from(project_id))
            .execute(&mut conn)
            .await
            .unwrap();
        #[cfg(feature = "sqlite")]
        let _ = project_id;
        Self(conn)
    }

    async fn release(self) {
        sqlx::Connection::close(self.0).await.unwrap();
    }
}

async fn events_stored(pool: &rustrak::db::DbPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Shutdown closes the digest's connection before its last checkpoint and
/// the pool, and stops waiting for it after a few seconds when a batch holds
/// it: closed for good all the same, so a digest after that does not open it
/// again. Its event stays in the spool for the next start.
#[actix_web::test]
async fn a_committer_closed_while_a_batch_holds_it_is_not_reopened() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Closed Mid Batch").await;
    let spool = Spool::new(&db.pool);
    spool.accept(project.id).await;
    let blocker = Blocker::hold(&db.pool, project.id).await;
    let recovery = tokio::spawn(recover_pending_events_once(
        db.pool.clone(),
        Arc::clone(&spool.processors),
        spool.path().to_path_buf(),
    ));
    crate::common::eventually(|| async { (spool.batches() >= 1).then_some(()) }).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let closing = tokio::time::timeout(
        Duration::from_millis(200),
        spool.processors.errors.close_committer(),
    )
    .await;
    assert!(closing.is_err(), "the batch was not holding the connection");
    blocker.release().await;
    recovery.await.unwrap();
    let stored = events_stored(&db.pool).await;
    let (_, file) = spool.accept(project.id).await;
    spool.recover(&db.pool).await;

    assert!(file.exists(), "the closed connection was opened again");
    assert_eq!(events_stored(&db.pool).await, stored);
}

/// Out of memory on one huge event (Postgres 53200, SQLite NOMEM) is the
/// event's doing as much as the database's: the batch splits, its
/// neighbours are stored, and the live digest goes on instead of retrying
/// the same batch for ever. The event stays in the spool.
#[cfg(feature = "postgres")]
#[actix_web::test]
async fn one_event_the_database_runs_out_of_memory_on_does_not_stall_the_live_digest() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Out Of Memory").await;
    for statement in [
        "CREATE FUNCTION oom() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
         IF NEW.data->>'message' = 'huge' THEN \
         RAISE EXCEPTION 'out of memory' USING ERRCODE = '53200'; END IF; \
         RETURN NEW; END $$",
        "CREATE TRIGGER oom BEFORE INSERT ON events FOR EACH ROW EXECUTE FUNCTION oom()",
    ] {
        sqlx::query(statement).execute(&db.pool).await.unwrap();
    }
    let spool = Spool::new(&db.pool);
    let gate = hold_the_commit_gate(&spool).await;
    let mut files = Vec::new();
    for message in ["before", "huge", "beside", "after"] {
        files.push(
            spool
                .enqueue(
                    &db.pool,
                    project.id,
                    serde_json::json!({"message": message}),
                )
                .await,
        );
    }
    drop(gate);

    crate::common::eventually(|| async { (events_stored(&db.pool).await == 3).then_some(()) })
        .await;
    spool
        .enqueue(
            &db.pool,
            project.id,
            serde_json::json!({"message": "later"}),
        )
        .await;
    crate::common::eventually(|| async { (events_stored(&db.pool).await == 4).then_some(()) })
        .await;

    assert!(files[1].exists(), "its file stays");
    assert!(!spool.path().join("dead").exists());
    assert!(!spool.processors.errors.is_halted());
    spool.processors.errors.start_draining();
}

/// A record written by another release (a version this one does not read)
/// is left where it is for the release that can, and stops holding a
/// pending slot, so it cannot make ingest shed forever.
#[actix_web::test]
async fn a_record_of_another_version_is_left_in_place() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Other Version").await;
    let spool = Spool::new(&db.pool);
    let (_, file) = spool.accept(project.id).await;
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    record["version"] = 2.into();
    std::fs::write(&file, record.to_string()).unwrap();

    for _ in 0..PASSES {
        spool.recover(&db.pool).await;
    }

    assert!(file.exists());
    assert!(!spool.path().join("dead").exists());
    assert_eq!(spool.pending(project.id), 0);
}

/// The digest pipeline must not live on the runtime of the HTTP worker whose
/// request started it. Actix restarts a worker whose thread died while the
/// process lives on; a pipeline that died with it would leave every event
/// queued after that in the spool until the next start.
#[actix_web::test]
async fn the_digest_pipeline_outlives_the_runtime_that_started_it() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Pipeline Runtime").await;
    let spool = Spool::new(&db.pool);
    let event_id = Uuid::new_v4();
    let metadata = EventMetadata {
        event_id: event_id.simple().to_string(),
        project_id: project.id,
        ingested_at: Utc::now(),
        remote_addr: None,
    };
    let payload = serde_json::json!({"event_id": metadata.event_id, "message": "outlives"});
    let file = spool.file(project.id, event_id);

    // An HTTP worker's runtime: it accepts the event, then dies.
    let (processors, pool) = (Arc::clone(&spool.processors), db.pool.clone());
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime
            .block_on(rustrak::ingest::pipeline::enqueue_digest(
                &processors,
                &pool,
                event_id,
                metadata,
                payload.to_string().into(),
            ))
            .unwrap();
    })
    .join()
    .unwrap();

    crate::common::eventually(|| async { (!file.exists()).then_some(()) }).await;
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(stored, 1);
}

/// A spool entry recovery cannot open (EACCES, here EISDIR) stays where it
/// is but stops holding a pending slot, so it cannot keep ingest shedding or
/// eat the project's quota for ever.
#[actix_web::test]
async fn an_entry_that_cannot_be_opened_stops_counting_as_pending() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Cannot Open").await;
    let spool = Spool::new(&db.pool);
    let event_id = Uuid::new_v4();
    let file = spool.file(project.id, event_id);
    std::fs::create_dir(&file).unwrap();
    spool
        .processors
        .errors
        .pending()
        .insert(project.id, event_id);

    for _ in 0..3 {
        spool.recover(&db.pool).await;
    }

    assert!(file.exists());
    assert!(!spool.path().join("dead").exists());
    assert_eq!(spool.pending(project.id), 0);
}

/// A file with an event's name that is not a pending record can never be
/// read: it goes to `dead/` on sight instead of being skipped forever while
/// it holds a pending slot.
#[actix_web::test]
async fn an_unreadable_record_is_dead_lettered() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Corrupt").await;
    let spool = Spool::new(&db.pool);
    let event_id = Uuid::new_v4();
    let file = spool.file(project.id, event_id);
    std::fs::write(&file, b"{not a pending record").unwrap();
    spool
        .processors
        .errors
        .pending()
        .insert(project.id, event_id);

    spool.recover(&db.pool).await;

    assert!(!file.exists());
    assert_eq!(
        std::fs::read(spool.dead(&file)).unwrap(),
        b"{not a pending record"
    );
    assert_eq!(spool.pending(project.id), 0);
    assert_eq!(spool.dead_lettered(), "rustrak_spool_dead_lettered_total 1");
}

/// `dead/` is not the backlog: the startup scan and the recovery listing
/// leave it alone.
#[actix_web::test]
async fn dead_letters_are_not_pending() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Dead Is Not Pending").await;
    let spool = tempfile::tempdir().unwrap();
    let dead = spool.path().join("dead");
    std::fs::create_dir(&dead).unwrap();
    let file = dead.join(format!(
        "project-{}-{}.pending.json",
        project.id,
        Uuid::new_v4().simple()
    ));
    std::fs::write(&file, b"{}").unwrap();

    let state = crate::common::state(
        &db.pool,
        crate::common::config_with_ingest_dir(spool.path()),
    )
    .await;

    assert_eq!(
        state
            .processors
            .errors
            .pending()
            .counts(project.id)
            .installation,
        0
    );
    assert!(rustrak::ingest::list_pending_event_metadata(spool.path())
        .await
        .unwrap()
        .is_empty());
    assert!(file.exists());
}

/// Recovery takes the same digest slots as ingest, so a large backlog at
/// startup cannot run more digests against the pool than the cap.
#[actix_web::test]
async fn recovery_waits_for_a_digest_slot() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Recovery Slots").await;
    let spool = Spool::new(&db.pool);
    let (_, file) = spool.accept(project.id).await;
    let slots = spool.processors.processing_slot.available_permits() as u32;
    let held = spool
        .processors
        .processing_slot
        .clone()
        .acquire_many_owned(slots)
        .await
        .unwrap();

    let blocked = tokio::time::timeout(Duration::from_millis(300), spool.recover(&db.pool)).await;
    assert!(blocked.is_err(), "recovery ran a digest without a slot");
    assert!(file.exists());

    drop(held);
    assert!(!spool.recover(&db.pool).await);
    assert_eq!(
        spool.pending(project.id),
        0,
        "digested once a slot was free"
    );
}

/// A database that refuses one payload's row, whatever else is in its batch.
async fn refuse_the_poison(pool: &rustrak::db::DbPool) {
    #[cfg(feature = "sqlite")]
    let refuse = "CREATE TRIGGER refuse_poison BEFORE INSERT ON events \
                  WHEN json_extract(NEW.data, '$.message') = 'poison' \
                  BEGIN SELECT RAISE(ABORT, 'refused'); END";
    #[cfg(feature = "postgres")]
    let refuse = "ALTER TABLE events ADD CONSTRAINT refuse_poison \
                  CHECK (data->>'message' IS DISTINCT FROM 'poison') NOT VALID";
    sqlx::query(refuse).execute(pool).await.unwrap();
}

/// One event the database refuses, in the middle of a batch, costs its
/// neighbours a few transactions, not their digest: the batch is split until
/// the refused event is alone, everything else is stored, and only that one
/// event counts an attempt.
#[actix_web::test]
async fn a_poison_event_is_isolated_at_batch_sizes_1_2_255_256() {
    for n in [1usize, 2, 255, 256] {
        let db = TestDb::new().await;
        let project = create_test_project(&db.pool, "Poison").await;
        let spool = Spool::new(&db.pool);
        refuse_the_poison(&db.pool).await;
        let mut refs = Vec::new();
        let mut poison = None;
        for i in 0..n {
            let message = if i == n / 2 {
                "poison"
            } else {
                "dead letter proof"
            };
            let (event_id, file) = spool.accept_message(project.id, message).await;
            if i == n / 2 {
                poison = Some((event_id, file));
            }
            refs.push(rustrak::digest::processors::SpoolRef {
                project_id: project.id,
                event_id,
                size: 0,
                owner: spool
                    .processors
                    .errors
                    .in_flight()
                    .register(project.id, event_id),
            });
        }
        let (poison, poison_file) = poison.unwrap();
        let before = spool.batches();

        let results =
            rustrak::ingest::pipeline::digest_refs(&spool.processors, &db.pool, refs).await;

        assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1, "n = {n}");
        let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(stored, n as i64 - 1, "n = {n}");
        assert!(poison_file.exists(), "n = {n}");
        assert_eq!(spool.attempts(project.id, poison), 1, "n = {n}");
        let bound = 2 * (n as f64).log2().ceil() as u64 + 1;
        assert!(
            spool.batches() > before,
            "n = {n}: transactions are counted"
        );
        assert!(
            spool.batches() - before <= bound,
            "n = {n}: {} transactions",
            spool.batches() - before
        );
    }
}

/// The same spool file twice in a failing batch is one event: the split may
/// part the copies, but it counts one attempt, not two.
#[actix_web::test]
async fn two_copies_of_a_poison_event_count_one_attempt() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Poison Copies").await;
    let spool = Spool::new(&db.pool);
    refuse_the_poison(&db.pool).await;
    let (poison, file) = spool.accept_message(project.id, "poison").await;
    let copy = || rustrak::digest::processors::SpoolRef {
        project_id: project.id,
        event_id: poison,
        size: 0,
        owner: spool
            .processors
            .errors
            .in_flight()
            .register(project.id, poison),
    };

    rustrak::ingest::pipeline::digest_refs(&spool.processors, &db.pool, vec![copy(), copy()]).await;

    assert!(file.exists());
    assert_eq!(spool.attempts(project.id, poison), 1);
}

/// A schema that refuses every row is not ten poison events: after five
/// distinct events fail alone the digest halts instead of dead-lettering the
/// backlog, and a probe once the schema is fixed stores all of it.
#[actix_web::test]
async fn a_broken_schema_halts_the_digest_instead_of_dead_lettering() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Broken Schema").await;
    let spool = Spool::new(&db.pool);
    let mut accepted = Vec::new();
    for _ in 0..10 {
        accepted.push(spool.accept(project.id).await);
    }
    refuse_every_event_row(&db.pool).await;

    spool.recover(&db.pool).await;
    assert!(spool.processors.errors.is_halted());
    for _ in 0..PASSES {
        spool.recover(&db.pool).await;
    }
    for (event_id, file) in &accepted {
        assert!(file.exists());
        assert_eq!(spool.attempts(project.id, *event_id), 0);
    }
    assert!(!spool.path().join("dead").exists());
    assert_eq!(spool.dead_lettered(), "rustrak_spool_dead_lettered_total 0");
    // Still ready: the dashboard the operator needs stays up, and says why.
    let (status, ready) = readiness(&spool, &db.pool).await;
    assert_eq!(status, 200);
    assert_eq!(ready["checks"]["digest"], "halted");

    #[cfg(feature = "sqlite")]
    let fix = "DROP TRIGGER refuse";
    #[cfg(feature = "postgres")]
    let fix = "ALTER TABLE events DROP CONSTRAINT refuse";
    sqlx::query(fix).execute(&db.pool).await.unwrap();
    spool.processors.errors.probe_now_for_test();
    for _ in 0..3 {
        if !spool.recover(&db.pool).await {
            break;
        }
    }

    assert!(!spool.processors.errors.is_halted());
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(stored, 10);
    assert_eq!(
        readiness(&spool, &db.pool).await.1["checks"]["digest"],
        "ok"
    );
}

/// Poison events alone in the spool are not a broken database, which still
/// stores any other event: however many fail alone, the digest never halts
/// and each reaches `dead/` after its attempts.
#[actix_web::test]
async fn poison_events_alone_in_the_spool_are_dead_lettered_without_halting() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Poison Alone").await;
    let spool = Spool::new(&db.pool);
    refuse_the_poison(&db.pool).await;
    let mut poison = Vec::new();
    for _ in 0..6 {
        poison.push(spool.accept_message(project.id, "poison").await);
    }

    for _ in 0..PASSES {
        spool.recover(&db.pool).await;
        assert!(
            !spool.processors.errors.is_halted(),
            "a database that stores other events halted the digest"
        );
    }

    for (_, file) in &poison {
        assert!(spool.dead(file).exists(), "{file:?} was not dead-lettered");
    }
    assert_eq!(spool.dead_lettered(), "rustrak_spool_dead_lettered_total 6");
    assert_eq!(spool.pending(project.id), 0);
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        stored, 0,
        "the check that the database stores events left nothing"
    );
    for table in [
        "issues",
        "groupings",
        "issue_environments",
        "issue_activity",
    ] {
        let rows: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(rows, 0, "{table}: the check left a row");
    }
    let counted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM projects WHERE stored_event_count <> 0 OR digested_event_count <> 0 \
         OR quota_minute_count <> 0 OR quota_hour_count <> 0",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let installation: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM installation WHERE digested_event_count <> 0 \
         OR quota_minute_count <> 0 OR quota_hour_count <> 0",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        (counted, installation),
        (0, 0),
        "the check left quota counted"
    );
}

/// Counts the events `prepare` has read, by the source map each one asks for.
struct CountingProvider(Arc<AtomicUsize>);

#[async_trait::async_trait]
impl SourceMapProvider for CountingProvider {
    async fn fetch_sourcemap(
        &self,
        _project_id: i32,
        _debug_id: &str,
        _file_type: &str,
    ) -> AppResult<Option<SourceMapEntry>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
}

/// An event with one minified frame: preparing it asks for its source map.
fn minified_event() -> serde_json::Value {
    serde_json::json!({
        "level": "error",
        "exception": {"values": [{"type": "TypeError", "value": "x is undefined",
            "stacktrace": {"frames": [{"abs_path": "app.min.js", "lineno": 1, "colno": 10}]}}]},
        "debug_meta": {"images": [{"type": "sourcemap", "code_file": "app.min.js",
            "debug_id": "c941d872-af1f-4f0c-a7ff-ad3d295fe153"}]},
    })
}

/// More events than the circuit breaker needs to suspect the database.
const DELETED_PROJECT_EVENTS: usize = 6;

/// The deleted project's events left the spool without halting anything:
/// none in `dead/`, none pending.
fn assert_dropped(spool: &Spool, project_id: i32, files: &[PathBuf]) {
    assert!(
        !spool.processors.errors.is_halted(),
        "a deleted project halted the digest"
    );
    for file in files {
        assert!(!file.exists(), "{file:?} stayed in the spool");
    }
    assert!(!spool.path().join("dead").exists());
    assert_eq!(spool.pending(project_id), 0);
    // Each reached the transaction, where the quota finds no row to admit
    // it against and drops it: the race happened, after `prepare`.
    let rate_limited = format!(
        "rustrak_digest_total{{result=\"rate_limited\"}} {}",
        files.len()
    );
    assert!(spool
        .counters
        .render(Default::default(), None)
        .lines()
        .any(|l| l == rate_limited));
}

/// Every commit permit, so prepared events wait for their commit.
async fn hold_the_commit_gate(spool: &Spool) -> tokio::sync::OwnedSemaphorePermit {
    let slots = spool.processors.processing_slot.available_permits() as u32;
    Arc::clone(&spool.processors.processing_slot)
        .acquire_many_owned(slots)
        .await
        .unwrap()
}

/// A project deleted after recovery prepared its events and before their
/// commit: they are dropped, like those `prepare` finds no project for, and
/// never halt the digest of every other project.
#[actix_web::test]
async fn a_project_deleted_before_a_recovery_commit_is_dropped() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Deleted Midway").await;
    let prepared = Arc::new(AtomicUsize::new(0));
    let spool = Spool::with_provider(&db.pool, Arc::new(CountingProvider(Arc::clone(&prepared))));
    let mut files = Vec::new();
    for _ in 0..DELETED_PROJECT_EVENTS {
        files.push(spool.accept_event(project.id, minified_event()).await.1);
    }
    let gate = hold_the_commit_gate(&spool).await;

    let recovery = tokio::spawn(recover_pending_events_once(
        db.pool.clone(),
        Arc::clone(&spool.processors),
        spool.path().to_path_buf(),
    ));
    crate::common::eventually(|| async {
        (prepared.load(Ordering::SeqCst) == DELETED_PROJECT_EVENTS).then_some(())
    })
    .await;
    rustrak::services::ProjectService::delete(&db.pool, project.id)
        .await
        .unwrap();
    drop(gate);
    recovery.await.unwrap();

    assert_dropped(&spool, project.id, &files);
}

/// The same race on the live digest.
#[actix_web::test]
async fn a_project_deleted_before_a_live_commit_is_dropped() {
    let db = TestDb::new().await;
    let project = create_test_project(&db.pool, "Deleted Live").await;
    let prepared = Arc::new(AtomicUsize::new(0));
    let spool = Spool::with_provider(&db.pool, Arc::new(CountingProvider(Arc::clone(&prepared))));
    let gate = hold_the_commit_gate(&spool).await;
    let mut files = Vec::new();
    for _ in 0..DELETED_PROJECT_EVENTS {
        files.push(spool.enqueue(&db.pool, project.id, minified_event()).await);
    }

    crate::common::eventually(|| async {
        (prepared.load(Ordering::SeqCst) == DELETED_PROJECT_EVENTS).then_some(())
    })
    .await;
    rustrak::services::ProjectService::delete(&db.pool, project.id)
        .await
        .unwrap();
    drop(gate);
    crate::common::eventually(|| async { files.iter().all(|f| !f.exists()).then_some(()) }).await;

    assert_dropped(&spool, project.id, &files);
}

/// What makes a database refuse every event after its row is written.
struct Refusal {
    what: &'static str,
    /// The event every one of the backlog is a copy of.
    event: serde_json::Value,
    statements: &'static [&'static str],
}

#[cfg(feature = "sqlite")]
const REFUSE_ISSUE_UPDATES: &[&str] =
    &["CREATE TRIGGER refuse BEFORE UPDATE ON issues BEGIN SELECT RAISE(ABORT, 'refused'); END"];
#[cfg(feature = "sqlite")]
const REFUSE_ENVIRONMENTS: &[&str] = &[
    "CREATE TRIGGER refuse_insert BEFORE INSERT ON issue_environments BEGIN SELECT RAISE(ABORT, 'refused'); END",
    "CREATE TRIGGER refuse_update BEFORE UPDATE ON issue_environments BEGIN SELECT RAISE(ABORT, 'refused'); END",
];
#[cfg(feature = "postgres")]
const REFUSE_ISSUE_UPDATES: &[&str] = &[
    "CREATE FUNCTION refuse() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'refused'; END $$",
    "CREATE TRIGGER refuse BEFORE UPDATE ON issues FOR EACH ROW EXECUTE FUNCTION refuse()",
];
#[cfg(feature = "postgres")]
const REFUSE_ENVIRONMENTS: &[&str] = &[
    "CREATE FUNCTION refuse() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'refused'; END $$",
    "CREATE TRIGGER refuse BEFORE INSERT OR UPDATE ON issue_environments FOR EACH ROW EXECUTE FUNCTION refuse()",
];

/// A database that refuses every event further on than its row (the update
/// of the issue it joins, the environment it was seen in) refuses every
/// event, not a few poison ones: the digest halts and nothing reaches `dead/`.
#[actix_web::test]
async fn a_database_wide_failure_after_the_event_row_halts_the_digest() {
    for refusal in [
        Refusal {
            what: "the update of an existing issue",
            event: serde_json::json!({"level": "error", "message": "seen before"}),
            statements: REFUSE_ISSUE_UPDATES,
        },
        Refusal {
            what: "the environments upsert",
            event: serde_json::json!({"level": "error", "message": "in production",
                "environment": "production"}),
            statements: REFUSE_ENVIRONMENTS,
        },
    ] {
        let what = refusal.what;
        let db = TestDb::new().await;
        let project = create_test_project(&db.pool, "Refused Later").await;
        let spool = Spool::new(&db.pool);
        spool.accept_event(project.id, refusal.event.clone()).await;
        assert!(
            !spool.recover(&db.pool).await,
            "{what}: the first one stores"
        );
        let mut accepted = Vec::new();
        for _ in 0..10 {
            accepted.push(spool.accept_event(project.id, refusal.event.clone()).await);
        }
        for statement in refusal.statements {
            sqlx::query(sqlx::AssertSqlSafe(*statement))
                .execute(&db.pool)
                .await
                .unwrap();
        }

        for _ in 0..PASSES {
            spool.recover(&db.pool).await;
        }

        assert!(spool.processors.errors.is_halted(), "{what}: not halted");
        for (event_id, file) in &accepted {
            assert!(file.exists(), "{what}: {file:?} left the spool");
            assert_eq!(spool.attempts(project.id, *event_id), 0, "{what}");
        }
        assert!(!spool.path().join("dead").exists(), "{what}");
    }
}

/// `GET /health/ready` against the spool's processors.
async fn readiness(spool: &Spool, pool: &rustrak::db::DbPool) -> (u16, serde_json::Value) {
    let app = actix_web::test::init_service(
        actix_web::App::new()
            .app_data(actix_web::web::Data::from(Arc::clone(&spool.processors)))
            .app_data(actix_web::web::Data::new(pool.clone()))
            .configure(rustrak::routes::health::configure),
    )
    .await;
    let resp = actix_web::test::call_service(
        &app,
        actix_web::test::TestRequest::get()
            .uri("/health/ready")
            .to_request(),
    )
    .await;
    let status = resp.status().as_u16();
    (status, actix_web::test::read_body_json(resp).await)
}
