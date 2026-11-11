//! Parsed source maps are reused across events.
//!
//! The lookup that decides *which* file a debug_id maps to still hits the
//! database on every event, so deletions and re-uploads take effect at once.
//! What gets cached is the expensive part behind it: reading the file from the
//! store and parsing it.

use crate::common::TestDb;
use async_trait::async_trait;
use bytes::Bytes;
use rustrak::models::CreateProject;
use rustrak::services::sourcemap::{rewrite_frames, DbSourceMapProvider};
use rustrak::services::sourcemap_store::{SourceMapStore, StoreError};
use rustrak::services::ProjectService;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct CountingStore {
    files: Mutex<HashMap<String, Bytes>>,
    reads: AtomicUsize,
    /// When set, every `get` waits here before returning, so a test can hold
    /// several readers at the miss at once.
    gate: Option<Arc<tokio::sync::Notify>>,
}

impl CountingStore {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            files: Mutex::new(HashMap::new()),
            reads: AtomicUsize::new(0),
            gate: None,
        })
    }

    fn gated(gate: Arc<tokio::sync::Notify>) -> Arc<Self> {
        Arc::new(Self {
            files: Mutex::new(HashMap::new()),
            reads: AtomicUsize::new(0),
            gate: Some(gate),
        })
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SourceMapStore for CountingStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<(), StoreError> {
        self.files.lock().unwrap().insert(key.to_string(), data);
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Bytes, StoreError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        self.files
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(key.to_string()))
    }

    async fn exists(&self, key: &str) -> Result<bool, StoreError> {
        Ok(self.files.lock().unwrap().contains_key(key))
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.files.lock().unwrap().remove(key);
        Ok(())
    }
}

fn sourcemap_with_source(source: &str) -> Bytes {
    Bytes::from(
        json!({
            "version": 3,
            "sources": [source],
            "sourcesContent": ["line 1\nline 2\nline 3\nline 4\nline 5"],
            "mappings": "AAAA;AACA;AACA;AACA;AACA"
        })
        .to_string(),
    )
}

fn push_uuid_bind(qb: &mut sqlx::QueryBuilder<rustrak::db::Db>, value: &str) {
    qb.push_bind(uuid::Uuid::parse_str(value).expect("value must be a valid uuid"));
}

/// Registers `data` under `storage_key` for `debug_id` in `project_id`,
/// exactly as a finished assembly would.
async fn register_sourcemap(
    pool: &rustrak::db::DbPool,
    store: &CountingStore,
    project_id: i32,
    debug_id: &str,
    storage_key: &str,
    data: Bytes,
) {
    store.put(storage_key, data.clone()).await.unwrap();
    let file_id = uuid::Uuid::new_v4().to_string();
    let mut qb = sqlx::QueryBuilder::new(
        "INSERT INTO source_file(id, checksum, size, storage_path) VALUES (",
    );
    push_uuid_bind(&mut qb, &file_id);
    qb.push(", ");
    qb.push_bind(storage_key.to_string());
    qb.push(", ");
    qb.push_bind(data.len() as i32);
    qb.push(", ");
    qb.push_bind(storage_key.to_string());
    qb.push(")");
    qb.build().execute(pool).await.expect("insert source_file");

    let mut qb = sqlx::QueryBuilder::new(
        "INSERT INTO source_file_metadata(id, project_id, debug_id, file_type, file_id) VALUES (",
    );
    push_uuid_bind(&mut qb, &uuid::Uuid::new_v4().to_string());
    qb.push(", ");
    qb.push_bind(project_id);
    qb.push(", ");
    push_uuid_bind(&mut qb, debug_id);
    qb.push(", ");
    qb.push_bind("source_map");
    qb.push(", ");
    push_uuid_bind(&mut qb, &file_id);
    qb.push(")");
    qb.build()
        .execute(pool)
        .await
        .expect("insert source_file_metadata");
}

async fn drop_sourcemap(pool: &rustrak::db::DbPool, project_id: i32, debug_id: &str) {
    let mut qb = sqlx::QueryBuilder::new("DELETE FROM source_file_metadata WHERE project_id = ");
    qb.push_bind(project_id);
    qb.push(" AND debug_id = ");
    push_uuid_bind(&mut qb, debug_id);
    qb.build().execute(pool).await.expect("delete metadata");
}

fn event_for(debug_id: &str) -> Value {
    json!({
        "debug_meta": {"images": [{"code_file": "app.js", "debug_id": debug_id}]},
        "exception": {"values": [{"stacktrace": {"frames": [
            {"filename": "app.js", "lineno": 5, "colno": 0}
        ]}}]}
    })
}

fn resolved_filename(event: &Value) -> &str {
    event["exception"]["values"][0]["stacktrace"]["frames"][0]["filename"]
        .as_str()
        .unwrap()
}

async fn setup(name: &str) -> (TestDb, i32) {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: name.to_string(),
            slug: Some(name.to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");
    (db, project.id)
}

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[tokio::test]
async fn parsed_sourcemap_is_reused_across_events() {
    let (db, project_id) = setup("smcache-reuse").await;
    let store = CountingStore::new();
    let debug_id = uuid::Uuid::new_v4().to_string();
    register_sourcemap(
        &db.pool,
        &store,
        project_id,
        &debug_id,
        SHA_A,
        sourcemap_with_source("src/a.ts"),
    )
    .await;
    let provider = DbSourceMapProvider::new(db.pool.clone(), store.clone());

    for _ in 0..3 {
        let mut event = event_for(&debug_id);
        rewrite_frames(&provider, project_id, &mut event)
            .await
            .unwrap();
        assert_eq!(resolved_filename(&event), "src/a.ts");
    }

    assert_eq!(
        store.reads(),
        1,
        "the file is read and parsed once, then reused"
    );
}

#[tokio::test]
async fn deleted_sourcemap_stops_resolving_immediately() {
    let (db, project_id) = setup("smcache-delete").await;
    let store = CountingStore::new();
    let debug_id = uuid::Uuid::new_v4().to_string();
    register_sourcemap(
        &db.pool,
        &store,
        project_id,
        &debug_id,
        SHA_A,
        sourcemap_with_source("src/a.ts"),
    )
    .await;
    let provider = DbSourceMapProvider::new(db.pool.clone(), store.clone());

    let mut event = event_for(&debug_id);
    rewrite_frames(&provider, project_id, &mut event)
        .await
        .unwrap();
    assert_eq!(resolved_filename(&event), "src/a.ts");

    drop_sourcemap(&db.pool, project_id, &debug_id).await;

    let mut event = event_for(&debug_id);
    rewrite_frames(&provider, project_id, &mut event)
        .await
        .unwrap();
    assert_eq!(
        resolved_filename(&event),
        "app.js",
        "a cached parse must not outlive the database row"
    );
}

#[tokio::test]
async fn reuploaded_sourcemap_with_same_debug_id_is_reparsed() {
    let (db, project_id) = setup("smcache-reupload").await;
    let store = CountingStore::new();
    let debug_id = uuid::Uuid::new_v4().to_string();
    register_sourcemap(
        &db.pool,
        &store,
        project_id,
        &debug_id,
        SHA_A,
        sourcemap_with_source("src/old.ts"),
    )
    .await;
    let provider = DbSourceMapProvider::new(db.pool.clone(), store.clone());

    let mut event = event_for(&debug_id);
    rewrite_frames(&provider, project_id, &mut event)
        .await
        .unwrap();
    assert_eq!(resolved_filename(&event), "src/old.ts");

    // Same debug_id, different content: a new source_file row.
    drop_sourcemap(&db.pool, project_id, &debug_id).await;
    register_sourcemap(
        &db.pool,
        &store,
        project_id,
        &debug_id,
        SHA_B,
        sourcemap_with_source("src/new.ts"),
    )
    .await;

    let mut event = event_for(&debug_id);
    rewrite_frames(&provider, project_id, &mut event)
        .await
        .unwrap();
    assert_eq!(resolved_filename(&event), "src/new.ts");
    assert_eq!(store.reads(), 2);
}

#[tokio::test]
async fn cache_evicts_least_recently_used_when_over_budget() {
    let (db, project_id) = setup("smcache-evict").await;
    let store = CountingStore::new();
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let map_a = sourcemap_with_source("src/a.ts");
    let map_b = sourcemap_with_source("src/b.ts");
    // Room for one map, not two.
    let budget = map_a.len() + map_b.len() / 2;
    register_sourcemap(&db.pool, &store, project_id, &id_a, SHA_A, map_a).await;
    register_sourcemap(&db.pool, &store, project_id, &id_b, SHA_B, map_b).await;
    let provider =
        DbSourceMapProvider::new(db.pool.clone(), store.clone()).with_cache_budget(budget);

    let mut event = event_for(&id_a);
    rewrite_frames(&provider, project_id, &mut event)
        .await
        .unwrap();
    let mut event = event_for(&id_b);
    rewrite_frames(&provider, project_id, &mut event)
        .await
        .unwrap();
    assert_eq!(store.reads(), 2);

    // B is the most recent; A had to go.
    let mut event = event_for(&id_b);
    rewrite_frames(&provider, project_id, &mut event)
        .await
        .unwrap();
    assert_eq!(store.reads(), 2, "B is still cached");
    let mut event = event_for(&id_a);
    rewrite_frames(&provider, project_id, &mut event)
        .await
        .unwrap();
    assert_eq!(resolved_filename(&event), "src/a.ts");
    assert_eq!(store.reads(), 3, "A was evicted and is read again");
}

#[tokio::test]
async fn concurrent_misses_share_one_read() {
    let (db, project_id) = setup("smcache-stampede").await;
    let gate = Arc::new(tokio::sync::Notify::new());
    let store = CountingStore::gated(Arc::clone(&gate));
    let debug_id = uuid::Uuid::new_v4().to_string();
    register_sourcemap(
        &db.pool,
        &store,
        project_id,
        &debug_id,
        SHA_A,
        sourcemap_with_source("src/a.ts"),
    )
    .await;
    let provider = Arc::new(DbSourceMapProvider::new(db.pool.clone(), store.clone()));

    let tasks: Vec<_> = (0..5)
        .map(|_| {
            let provider = Arc::clone(&provider);
            let debug_id = debug_id.clone();
            tokio::spawn(async move {
                let mut event = event_for(&debug_id);
                rewrite_frames(provider.as_ref(), project_id, &mut event)
                    .await
                    .unwrap();
                resolved_filename(&event).to_string()
            })
        })
        .collect();

    // Let every task reach the store (or the wait behind the first reader).
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    gate.notify_waiters();
    for task in tasks {
        assert_eq!(task.await.unwrap(), "src/a.ts");
    }

    assert_eq!(
        store.reads(),
        1,
        "concurrent misses must wait for the first load"
    );
}
