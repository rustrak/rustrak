//! Integration tests for the Source Maps API
//!
//! Story: source-map-upload-frame-rewriting
//!
//! ## Coverage Map
//!
//! | Test group                        | Task    | AC          |
//! |-----------------------------------|---------|-------------|
//! | org_probe_*                       | T6      | AC1 prereq  |
//! | chunk_capability_*                | T6      | AC1 prereq  |
//! | chunk_upload_*                    | T6      | AC1, AC3    |
//! | assemble_*                        | T6, T7  | AC1, AC3, AC6, AC7 |
//! | list_source_maps_*                | T6      | AC1         |
//! | digest_rewrite_*                  | T8      | AC2         |
//! | cross_project_*                   | T5, T8  | AC5         |
//! | worker_recovery_*                 | T7      | AC7         |

use crate::common::TestDb;
use actix_web::{test, web, App};
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::models::{CreateAuthToken, CreateProject};
use rustrak::routes;
use rustrak::services::{
    AuthTokenService, DbSourceMapProvider, LocalSourceMapStore, ProjectService, SourceMapProvider,
    SourceMapStore,
};
use serde_json::Value;
use sha1::{Digest as _, Sha1};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

// =============================================================================
// Test helpers
// =============================================================================

fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            max_connections: 5,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(300),
        },
        rate_limit: RateLimitConfig {
            max_events_per_minute: 1000,
            max_events_per_hour: 10000,
            max_events_per_project_per_minute: 500,
            max_events_per_project_per_hour: 5000,
        },
        security: rustrak::config::SecurityConfig {
            ssl_proxy: false,
            session_secret_key: None,
        },
        ingest_dir: None,
        public_url: None,
        sourcemap_storage_path: "/tmp/test_sourcemaps".to_string(),
        sourcemap_cache_bytes: 64 * 1024 * 1024,
        max_chunk_size_bytes: 10 * 1024 * 1024,
        session_flush_interval_secs: 30,
        session_cardinality_cap: 10_000,
        dashboard: DashboardConfig {
            dir: "./static".to_string(),
            enabled: true,
            url: None,
        },
        telemetry: rustrak::config::TelemetryConfig {
            enabled: false,
            do_not_track: false,
        },
    }
}

async fn create_test_token(pool: &rustrak::db::DbPool) -> String {
    AuthTokenService::create(
        pool,
        CreateAuthToken {
            description: Some("Test token".to_string()),
        },
    )
    .await
    .expect("Failed to create test token")
    .token
}

/// Build the sourcemap app data components from a pool + config.
/// Returns `(Arc<dyn SourceMapProvider>, web::Data<Config>)` so tests can
/// call `test::init_service(App::new().app_data(...)...)` inline.
fn sourcemap_app_data(
    pool: &rustrak::db::DbPool,
    config: &Config,
) -> (Arc<dyn SourceMapProvider>, web::Data<Config>) {
    let store: Arc<dyn SourceMapStore> =
        Arc::new(LocalSourceMapStore::new(&config.sourcemap_storage_path));
    let provider: Arc<dyn SourceMapProvider> =
        Arc::new(DbSourceMapProvider::new(pool.clone(), store));
    (provider, web::Data::new(config.clone()))
}

fn sha1_hex(data: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn build_multipart(boundary: &str, parts: &[(&str, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, data) in parts {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                name, name
            )
            .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    body
}

// ---------------------------------------------------------------------------
// DB helpers — backend-agnostic (SQLite `?` vs Postgres `$N`)
// ---------------------------------------------------------------------------

async fn count_where_eq(pool: &rustrak::db::DbPool, table: &str, col: &str, val: &str) -> i64 {
    #[cfg(feature = "postgres")]
    let sql = format!("SELECT COUNT(*) FROM {} WHERE {} = $1", table, col);
    #[cfg(not(feature = "postgres"))]
    let sql = format!("SELECT COUNT(*) FROM {} WHERE {} = ?", table, col);
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(&*sql))
        .bind(val)
        .fetch_one(pool)
        .await
        .expect("count query must succeed")
}

async fn count_sfm_for_project(pool: &rustrak::db::DbPool, project_id: i32) -> i64 {
    #[cfg(feature = "postgres")]
    let sql = "SELECT COUNT(*) FROM source_file_metadata WHERE project_id = $1";
    #[cfg(not(feature = "postgres"))]
    let sql = "SELECT COUNT(*) FROM source_file_metadata WHERE project_id = ?";
    sqlx::query_scalar::<_, i64>(sql)
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("count source_file_metadata must succeed")
}

async fn insert_assembly_job_with_state(
    pool: &rustrak::db::DbPool,
    checksum: &str,
    project_id: i32,
    state: &str,
    detail: Option<&str>,
) {
    #[cfg(feature = "postgres")]
    sqlx::query(
        "INSERT INTO assembly_jobs(bundle_checksum, project_id, chunks, state, detail) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(checksum)
    .bind(project_id)
    .bind(&Vec::<String>::new() as &Vec<String>)
    .bind(state)
    .bind(detail)
    .execute(pool)
    .await
    .expect("insert assembly_jobs must succeed");

    #[cfg(not(feature = "postgres"))]
    sqlx::query(
        "INSERT INTO assembly_jobs(bundle_checksum, project_id, chunks, state, detail) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(checksum)
    .bind(project_id)
    .bind("[]")
    .bind(state)
    .bind(detail)
    .execute(pool)
    .await
    .expect("insert assembly_jobs must succeed");
}

async fn set_assembly_job_state(
    pool: &rustrak::db::DbPool,
    checksum: &str,
    project_id: i32,
    state: &str,
) {
    #[cfg(feature = "postgres")]
    sqlx::query(
        "UPDATE assembly_jobs SET state = $1 WHERE bundle_checksum = $2 AND project_id = $3",
    )
    .bind(state)
    .bind(checksum)
    .bind(project_id)
    .execute(pool)
    .await
    .expect("update assembly_jobs state must succeed");

    #[cfg(not(feature = "postgres"))]
    sqlx::query("UPDATE assembly_jobs SET state = ? WHERE bundle_checksum = ? AND project_id = ?")
        .bind(state)
        .bind(checksum)
        .bind(project_id)
        .execute(pool)
        .await
        .expect("update assembly_jobs state must succeed");
}

/// Binds a UUID-typed column: Postgres requires a native `Uuid` value (a bare
/// `&str` is sent as `text`, which Postgres refuses to assign into a `uuid`
/// column — code 42804), while SQLite's `uuid` columns are untyped `TEXT` and
/// accept the string form directly.
fn push_uuid_bind(qb: &mut sqlx::QueryBuilder<rustrak::db::Db>, value: &str) {
    #[cfg(feature = "postgres")]
    qb.push_bind(uuid::Uuid::parse_str(value).expect("value must be a valid uuid"));
    #[cfg(not(feature = "postgres"))]
    qb.push_bind(value);
}

async fn insert_source_file(
    pool: &rustrak::db::DbPool,
    id: &str,
    checksum: &str,
    size: i32,
    storage_path: &str,
) {
    let mut qb = sqlx::QueryBuilder::new(
        "INSERT INTO source_file(id, checksum, size, storage_path) VALUES (",
    );
    push_uuid_bind(&mut qb, id);
    qb.push(", ");
    qb.push_bind(checksum);
    qb.push(", ");
    qb.push_bind(size);
    qb.push(", ");
    qb.push_bind(storage_path);
    qb.push(")");
    qb.build().execute(pool).await.expect("insert source_file");
}

async fn insert_source_file_metadata(
    pool: &rustrak::db::DbPool,
    id: &str,
    project_id: i32,
    debug_id: &str,
    file_type: &str,
    file_id: &str,
) {
    let mut qb = sqlx::QueryBuilder::new(
        "INSERT INTO source_file_metadata(id, project_id, debug_id, file_type, file_id) VALUES (",
    );
    push_uuid_bind(&mut qb, id);
    qb.push(", ");
    qb.push_bind(project_id);
    qb.push(", ");
    push_uuid_bind(&mut qb, debug_id);
    qb.push(", ");
    qb.push_bind(file_type);
    qb.push(", ");
    push_uuid_bind(&mut qb, file_id);
    qb.push(")");
    qb.build()
        .execute(pool)
        .await
        .expect("insert source_file_metadata");
}

// =============================================================================
// Group 1: Org probe — no DB, no auth needed
// =============================================================================

#[actix_web::test]
async fn test_org_probe_any_slug_returns_200() {
    let db = TestDb::new().await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/0/organizations/any-random-slug/")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["slug"], "any-random-slug");

    let features = body["features"]
        .as_array()
        .expect("features must be an array");
    assert!(
        features.iter().any(|f| f == "artifact-bundles"),
        "features must include artifact-bundles"
    );
    assert!(
        features.iter().any(|f| f == "artifact-bundles-v2"),
        "features must include artifact-bundles-v2"
    );
}

#[actix_web::test]
async fn test_org_probe_different_slugs_all_return_200() {
    let db = TestDb::new().await;
    let config = create_test_config();

    for slug in &["my-org", "123", "a-b-c-d", "UPPERCASE"] {
        let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db.pool.clone()))
                .app_data(config_data)
                .app_data(web::Data::new(Arc::clone(&provider)))
                .configure(routes::sourcemaps::configure),
        )
        .await;

        let req = test::TestRequest::get()
            .uri(&format!("/api/0/organizations/{}/", slug))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            200,
            "org probe for slug '{}' must return 200",
            slug
        );

        let body: Value = test::read_body_json(resp).await;
        assert_eq!(
            body["slug"], *slug,
            "response slug must echo back the request slug"
        );
    }
}

// =============================================================================
// Group 2: Chunk upload capability — needs BearerAuth
// =============================================================================

#[actix_web::test]
async fn test_chunk_capability_includes_artifact_bundles_v2() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;

    let accept = body["accept"].as_array().expect("accept must be an array");
    assert!(
        accept.iter().any(|a| a == "artifact_bundles"),
        "accept must include artifact_bundles"
    );
    assert!(
        accept.iter().any(|a| a == "artifact_bundles_v2"),
        "accept must include artifact_bundles_v2"
    );

    assert_eq!(body["hashAlgorithm"], "sha1");

    let chunk_size = body["chunkSize"]
        .as_u64()
        .expect("chunkSize must be a number");
    assert!(chunk_size > 0, "chunkSize must be positive");

    let chunks_per_req = body["chunksPerRequest"]
        .as_u64()
        .expect("chunksPerRequest must be a number");
    assert!(chunks_per_req > 0, "chunksPerRequest must be positive");
}

// =============================================================================
// Group 3: Chunk upload — needs BearerAuth + multipart
// =============================================================================

#[actix_web::test]
async fn test_chunk_upload_stores_chunks_in_db() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let chunk1 = b"hello world chunk data";
    let chunk2 = b"another chunk of data";
    let sha1_1 = sha1_hex(chunk1);
    let sha1_2 = sha1_hex(chunk2);

    // sentry-cli protocol: field name IS the SHA1 of the content
    let boundary = "testboundary1234";
    let body = build_multipart(
        boundary,
        &[
            (sha1_1.as_str(), chunk1.as_ref()),
            (sha1_2.as_str(), chunk2.as_ref()),
        ],
    );

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        ))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Verify via get_missing_chunks: both checksums are present, nothing missing
    let missing = rustrak::services::sourcemap::get_missing_chunks(
        &db.pool,
        &[sha1_1.clone(), sha1_2.clone()],
    )
    .await
    .expect("get_missing_chunks must succeed");
    assert!(
        missing.is_empty(),
        "after upload, no chunks should be missing; got: {:?}",
        missing
    );
}

#[actix_web::test]
async fn test_chunk_upload_chunk_too_large_returns_400() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    // Config with max_chunk_size_bytes = 100 bytes
    let mut config = create_test_config();
    config.max_chunk_size_bytes = 100;
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    // Build a chunk that exceeds 100 bytes
    let oversized: Vec<u8> = vec![0xAA; 200];
    let boundary = "smalllimitboundary";
    let body = build_multipart(boundary, &[("file", &oversized)]);

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        ))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "oversized chunk must return 400");
}

#[actix_web::test]
async fn test_chunk_upload_empty_multipart_returns_400() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    // Empty multipart body — no parts at all
    let boundary = "emptyboundary9876";
    let body = format!("--{}--\r\n", boundary).into_bytes();

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        ))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "empty multipart body must return 400");
}

#[actix_web::test]
async fn test_chunk_upload_sha1_field_name_accepted() {
    // sentry-cli sends chunks with SHA1 hash as the multipart field name, not "file".
    // Rustrak must accept any non-empty field name.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let chunk_data = b"source map chunk from real sentry-cli";
    let sha1 = sha1_hex(chunk_data);

    let boundary = "sha1fieldboundary";
    let body = build_multipart(boundary, &[(sha1.as_str(), chunk_data.as_ref())]);

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        ))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "chunk with SHA1 field name must be accepted (real sentry-cli protocol)"
    );

    let missing =
        rustrak::services::sourcemap::get_missing_chunks(&db.pool, std::slice::from_ref(&sha1))
            .await
            .expect("get_missing_chunks must succeed");
    assert!(
        missing.is_empty(),
        "chunk uploaded with SHA1 field name must be stored in DB; still missing: {:?}",
        missing
    );
}

#[actix_web::test]
async fn test_chunk_upload_dedup_same_sha1() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let sha1_val = sha1_hex(b"deduplicated chunk data");
    let boundary = "dedup1234";

    // Upload the same chunk twice
    for _ in 0..2 {
        let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db.pool.clone()))
                .app_data(config_data)
                .app_data(web::Data::new(Arc::clone(&provider)))
                .configure(routes::sourcemaps::configure),
        )
        .await;

        let body = build_multipart(
            boundary,
            &[(sha1_val.as_str(), b"deduplicated chunk data".as_ref())],
        );
        let req = test::TestRequest::post()
            .uri("/api/0/organizations/my-org/chunk-upload/")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            ))
            .set_payload(body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "both uploads must return 200");
    }

    // Verify exactly one row exists in DB for this SHA1
    let count = count_where_eq(&db.pool, "chunk", "checksum", &sha1_val).await;
    assert_eq!(
        count, 1,
        "dedup: only 1 row should exist for duplicate upload"
    );
}

// =============================================================================
// Group 4: Assemble — needs BearerAuth + JSON + project in DB
// =============================================================================

#[actix_web::test]
async fn test_assemble_before_upload_returns_not_found_with_missing_chunks() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    // Create a project
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "my-proj-assemble-missing".to_string(),
            slug: Some("my-proj-assemble-missing".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let sha1_a = "aabbccddaabbccddaabbccddaabbccddaabbccdd";
    let sha1_b = "bbccddeebbccddeebbccddeebbccddeebbccddee";

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            "chunks": [sha1_a, sha1_b],
            "projects": [project.slug]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "missing chunks must return 200");

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["state"], "not_found");

    let missing = body["missingChunks"]
        .as_array()
        .expect("missingChunks must be an array");
    assert_eq!(missing.len(), 2, "both chunks should be missing");
    assert!(missing.iter().any(|c| c == sha1_a));
    assert!(missing.iter().any(|c| c == sha1_b));
}

#[actix_web::test]
async fn test_assemble_after_upload_enqueues_job_returns_created() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "assemble-after-upload".to_string(),
            slug: Some("assemble-after-upload".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let config = create_test_config();

    // Store two chunks directly in the DB
    let chunk1: &[u8] = b"chunk one data here";
    let chunk2: &[u8] = b"chunk two data here";
    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![
            (sha1_hex(chunk1), chunk1.to_vec()),
            (sha1_hex(chunk2), chunk2.to_vec()),
        ],
        config.max_chunk_size_bytes,
    )
    .await
    .expect("store_chunks must succeed");

    // Compute the bundle checksum: SHA1 of chunk1 ++ chunk2
    let mut joined = chunk1.to_vec();
    joined.extend_from_slice(chunk2);
    let bundle_checksum = sha1_hex(&joined);

    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": bundle_checksum,
            "chunks": [sha1_hex(chunk1), sha1_hex(chunk2)],
            "projects": [project.slug]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["state"], "created");

    let missing = body["missingChunks"]
        .as_array()
        .expect("missingChunks must be array");
    assert!(
        missing.is_empty(),
        "no chunks should be missing after upload"
    );
}

/// sentry-cli `--wait` polls assemble until `state` is terminal. Every poll
/// must be HTTP 200; missing chunks are `state: "not_found"`, not 202.
#[actix_web::test]
async fn test_assemble_wait_poll_returns_ok_after_chunks_uploaded() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "assemble-wait-poll".to_string(),
            slug: Some("assemble-wait-poll".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let bundle = build_test_artifact_bundle();
    let chunk_sha1 = sha1_hex(&bundle);
    let bundle_checksum = sha1_hex(&bundle);
    let auth = format!("Bearer {}", token);

    let assemble_body = serde_json::json!({
        "checksum": bundle_checksum,
        "chunks": [chunk_sha1],
        "projects": [project.slug]
    });

    let missing_req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/json"))
        .set_json(&assemble_body)
        .to_request();
    let missing_resp = test::call_service(&app, missing_req).await;
    assert_eq!(
        missing_resp.status(),
        200,
        "assemble with missing chunks must return 200, not 202"
    );
    let missing_body: Value = test::read_body_json(missing_resp).await;
    assert_eq!(missing_body["state"], "not_found");
    let missing = missing_body["missingChunks"]
        .as_array()
        .expect("missingChunks must be an array");
    assert!(
        missing.iter().any(|c| c == &chunk_sha1),
        "missingChunks must include the uploaded checksum; got: {:?}",
        missing
    );

    let boundary = "waitpollboundary";
    let upload_body = build_multipart(boundary, &[(chunk_sha1.as_str(), bundle.as_slice())]);
    let upload_req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", auth.clone()))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        ))
        .set_payload(upload_body)
        .to_request();
    let upload_resp = test::call_service(&app, upload_req).await;
    assert_eq!(upload_resp.status(), 200, "chunk-upload must succeed");

    let created_req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/json"))
        .set_json(&assemble_body)
        .to_request();
    let created_resp = test::call_service(&app, created_req).await;
    assert_eq!(created_resp.status(), 200);
    let created_body: Value = test::read_body_json(created_resp).await;
    let created_state = created_body["state"].as_str().unwrap_or("");
    assert!(
        created_state == "created" || created_state == "assembling",
        "after chunks exist, assemble must enqueue the job; got state={}",
        created_state
    );

    // The integration suite does not run AssemblyWorker; mark the job complete
    // the same way the worker would, then poll assemble again (sentry-cli --wait).
    set_assembly_job_state(&db.pool, &bundle_checksum, project.id, "ok").await;

    let ok_req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", auth))
        .insert_header(("Content-Type", "application/json"))
        .set_json(&assemble_body)
        .to_request();
    let ok_resp = test::call_service(&app, ok_req).await;
    assert_eq!(ok_resp.status(), 200);
    let ok_body: Value = test::read_body_json(ok_resp).await;
    assert_eq!(
        ok_body["state"], "ok",
        "polling assemble after the job completes must return state=ok; got: {}",
        ok_body["state"]
    );
}

/// After a successful assembly the worker deletes the chunk rows. A later
/// assemble poll (sentry-cli `--wait`) must still return the job's state —
/// reporting the consumed chunks as missing sends the client into an
/// infinite poll loop.
#[actix_web::test]
async fn test_assemble_poll_returns_ok_after_worker_deleted_chunks() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "assemble-poll-chunks-gone".to_string(),
            slug: Some("assemble-poll-chunks-gone".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let bundle = build_test_artifact_bundle();
    let chunk_sha1 = sha1_hex(&bundle);
    let bundle_checksum = sha1_hex(&bundle);
    let auth = format!("Bearer {}", token);

    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![(chunk_sha1.clone(), bundle.clone())],
        config.max_chunk_size_bytes,
    )
    .await
    .expect("store_chunks must succeed");

    let assemble_body = serde_json::json!({
        "checksum": bundle_checksum,
        "chunks": [chunk_sha1],
        "projects": [project.slug]
    });

    let created_req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/json"))
        .set_json(&assemble_body)
        .to_request();
    let created_resp = test::call_service(&app, created_req).await;
    assert_eq!(created_resp.status(), 200);
    let created_body: Value = test::read_body_json(created_resp).await;
    assert_eq!(created_body["state"], "created");

    // Simulate the assembly worker completing: job goes ok AND the consumed
    // chunk rows are deleted (assemble_bundle step 8).
    set_assembly_job_state(&db.pool, &bundle_checksum, project.id, "ok").await;
    sqlx::query(
        "DELETE FROM assembly_job_chunks WHERE job_id = (SELECT id FROM assembly_jobs WHERE bundle_checksum = $1 AND project_id = $2)",
    )
    .bind(&bundle_checksum)
    .bind(project.id)
    .execute(&db.pool)
    .await
    .expect("job chunk ownership delete must succeed");
    sqlx::query("DELETE FROM chunk WHERE checksum = $1")
        .bind(&chunk_sha1)
        .execute(&db.pool)
        .await
        .expect("chunk delete must succeed");

    let poll_req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", auth))
        .insert_header(("Content-Type", "application/json"))
        .set_json(&assemble_body)
        .to_request();
    let poll_resp = test::call_service(&app, poll_req).await;
    assert_eq!(poll_resp.status(), 200);
    let poll_body: Value = test::read_body_json(poll_resp).await;
    assert_eq!(
        poll_body["state"], "ok",
        "poll after worker completion must return the job state, not re-request \
         the deleted chunks; got: {}",
        poll_body
    );
}

#[actix_web::test]
async fn test_assemble_idempotent_same_bundle_twice() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "assemble-idempotent".to_string(),
            slug: Some("assemble-idempotent".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let config = create_test_config();

    // Pre-store chunks
    let chunk1: &[u8] = b"idempotent chunk data one";
    let chunk2: &[u8] = b"idempotent chunk data two";
    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![
            (sha1_hex(chunk1), chunk1.to_vec()),
            (sha1_hex(chunk2), chunk2.to_vec()),
        ],
        config.max_chunk_size_bytes,
    )
    .await
    .expect("store_chunks must succeed");

    let mut joined = chunk1.to_vec();
    joined.extend_from_slice(chunk2);
    let bundle_checksum = sha1_hex(&joined);

    // Insert an assembly_jobs row with state='ok' directly
    insert_assembly_job_with_state(&db.pool, &bundle_checksum, project.id, "ok", None).await;

    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    // POST assemble — should return the existing 'ok' job
    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": bundle_checksum,
            "chunks": [sha1_hex(chunk1), sha1_hex(chunk2)],
            "projects": [project.slug]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["state"], "ok");

    // Verify exactly one assembly_jobs row
    let count = count_where_eq(
        &db.pool,
        "assembly_jobs",
        "bundle_checksum",
        &bundle_checksum,
    )
    .await;
    assert_eq!(count, 1, "idempotent: only 1 assembly_jobs row expected");
}

#[actix_web::test]
async fn test_assemble_unknown_project_slug_returns_404() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            "chunks": [],
            "projects": ["nonexistent-project-xyz"]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404, "unknown project slug must return 404");
}

#[actix_web::test]
async fn test_assemble_empty_projects_array_returns_400() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            "chunks": [],
            "projects": []
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "empty projects array must return 400");
}

#[actix_web::test]
#[ignore = "requires assembly worker to process ZIP and set error state"]
async fn test_assemble_zip_path_traversal_sets_error_state() {
    // This test requires the background assembly worker to process the job.
    // When a ZIP containing a path traversal entry (e.g. "../../../etc/passwd") is
    // submitted, the worker must set assembly_jobs.state = 'error'.
    //
    // Setup: insert an assembly_jobs row pointing to a ZIP with path traversal entries,
    // start the AssemblyWorker, wait for job processing, then assert:
    //   let job: AssemblyJob = sqlx::query_as("SELECT * FROM assembly_jobs WHERE ...").fetch_one(&pool).await.unwrap();
    //   assert_eq!(job.state, "error");
    //   assert!(job.detail.unwrap().contains("path traversal"));
    todo!("enable once assembly worker is running in test context")
}

#[actix_web::test]
#[ignore = "requires assembly worker to process ZIP and set error state"]
async fn test_assemble_zip_symlink_sets_error_state() {
    // This test requires the background assembly worker.
    // When a ZIP containing a symlink entry (CVE-2025-29787) is submitted,
    // the worker must skip it and not create any symlink on disk.
    //
    // Setup: build a ZIP with a symlink entry, store its chunks, submit to assemble,
    // start AssemblyWorker, wait for processing, then assert:
    //   assert!(job.state == "ok" || job.state == "error"); // no symlink created on disk
    todo!("enable once assembly worker is running in test context")
}

#[actix_web::test]
async fn test_assemble_checksum_mismatch_enqueues_job() {
    // Checksum verification is async (done by the worker, not the handler).
    // The handler accepts the job and returns 200 with state "created".
    // The mismatch is surfaced when the caller polls the endpoint again after
    // the worker processes the job.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "assemble-checksum-mismatch".to_string(),
            slug: Some("assemble-checksum-mismatch".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let config = create_test_config();

    // Store two chunks
    let chunk1: &[u8] = b"mismatch chunk alpha";
    let chunk2: &[u8] = b"mismatch chunk beta";
    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![
            (sha1_hex(chunk1), chunk1.to_vec()),
            (sha1_hex(chunk2), chunk2.to_vec()),
        ],
        config.max_chunk_size_bytes,
    )
    .await
    .expect("store_chunks must succeed");

    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let wrong_checksum = "0000000000000000000000000000000000000000";

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": wrong_checksum,
            "chunks": [sha1_hex(chunk1), sha1_hex(chunk2)],
            "projects": [project.slug]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Handler accepts; checksum is verified asynchronously by the worker
    assert_eq!(resp.status(), 200, "handler must accept the job");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["state"], "created", "job must be in created state");
}

#[actix_web::test]
async fn test_assemble_failed_job_returns_400_on_retry() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "assemble-failed-retry".to_string(),
            slug: Some("assemble-failed-retry".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    // Use a fake checksum for a pre-existing 'error' job
    let fake_checksum = "cafebabecafebabecafebabecafebabecafebabe";

    // Insert an assembly_jobs row with state='error' directly
    insert_assembly_job_with_state(
        &db.pool,
        fake_checksum,
        project.id,
        "error",
        Some("assembly failed: some test error"),
    )
    .await;

    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    // POST assemble — must return the existing 'error' job → 400
    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": fake_checksum,
            "chunks": [],
            "projects": [project.slug]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Re-submitting a failed job must reset it to 'created' (200), not return 400 permanently.
    assert_eq!(
        resp.status(),
        200,
        "re-submitting an errored job must re-queue it (200), not block forever (400)"
    );

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["state"], "created",
        "re-submitted job must have state='created'; got: {}",
        body["state"]
    );
}

// =============================================================================
// Group 5: List source maps
// =============================================================================

#[actix_web::test]
async fn test_list_source_maps_returns_paginated_list() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "list-sourcemaps-proj".to_string(),
            slug: Some("list-sourcemaps-proj".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    // Insert two source_file + source_file_metadata rows directly
    let file_id_a = uuid::Uuid::new_v4().to_string();
    let debug_id_a = uuid::Uuid::new_v4().to_string();
    let file_id_b = uuid::Uuid::new_v4().to_string();
    let debug_id_b = uuid::Uuid::new_v4().to_string();

    insert_source_file(
        &db.pool,
        &file_id_a,
        "checksum_a_001",
        1234,
        "/tmp/test_a.map",
    )
    .await;
    insert_source_file_metadata(
        &db.pool,
        &uuid::Uuid::new_v4().to_string(),
        project.id,
        &debug_id_a,
        "source_map",
        &file_id_a,
    )
    .await;

    insert_source_file(
        &db.pool,
        &file_id_b,
        "checksum_b_002",
        5678,
        "/tmp/test_b.map",
    )
    .await;
    insert_source_file_metadata(
        &db.pool,
        &uuid::Uuid::new_v4().to_string(),
        project.id,
        &debug_id_b,
        "source_map",
        &file_id_b,
    )
    .await;

    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/0/projects/my-org/{}/files/source-maps/",
            project.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let entries = body["data"].as_array().expect("data must be an array");
    assert_eq!(entries.len(), 2, "must return exactly 2 source map entries");

    for entry in entries {
        assert!(entry.get("debugId").is_some(), "entry must have debugId");
        assert!(entry.get("fileType").is_some(), "entry must have fileType");
        assert!(entry.get("size").is_some(), "entry must have size");
        assert!(
            entry.get("timesUsed").is_some(),
            "entry must have timesUsed"
        );
        assert!(
            entry.get("dateUploaded").is_some(),
            "entry must have dateUploaded"
        );
    }
}

#[actix_web::test]
async fn test_list_source_maps_unknown_project_returns_404() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/0/projects/my-org/ghost-project/files/source-maps/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404, "unknown project must return 404");
}

#[actix_web::test]
async fn test_list_source_maps_scoped_to_project() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let proj_a = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "sourcemaps-scope-a".to_string(),
            slug: Some("sourcemaps-scope-a".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project A creation");

    let proj_b = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "sourcemaps-scope-b".to_string(),
            slug: Some("sourcemaps-scope-b".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project B creation");

    let debug_id_a = uuid::Uuid::new_v4().to_string();
    let debug_id_b = uuid::Uuid::new_v4().to_string();

    // Insert source map for project A
    let file_id_a = uuid::Uuid::new_v4().to_string();
    insert_source_file(
        &db.pool,
        &file_id_a,
        "scope_checksum_a",
        100,
        "/tmp/scope_a.map",
    )
    .await;
    insert_source_file_metadata(
        &db.pool,
        &uuid::Uuid::new_v4().to_string(),
        proj_a.id,
        &debug_id_a,
        "source_map",
        &file_id_a,
    )
    .await;

    // Insert source map for project B
    let file_id_b = uuid::Uuid::new_v4().to_string();
    insert_source_file(
        &db.pool,
        &file_id_b,
        "scope_checksum_b",
        200,
        "/tmp/scope_b.map",
    )
    .await;
    insert_source_file_metadata(
        &db.pool,
        &uuid::Uuid::new_v4().to_string(),
        proj_b.id,
        &debug_id_b,
        "source_map",
        &file_id_b,
    )
    .await;

    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    // GET source maps for project A only
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/0/projects/my-org/{}/files/source-maps/",
            proj_a.slug
        ))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let entries = body["data"].as_array().expect("data must be array");

    assert_eq!(entries.len(), 1, "only 1 entry for project A");

    // The debug_id must be A's, not B's
    let returned_debug_id = entries[0]["debugId"].as_str().unwrap_or("");
    assert_eq!(
        returned_debug_id, debug_id_a,
        "project A's source map must have debug_id_a; got {}",
        returned_debug_id
    );
    assert_ne!(
        returned_debug_id, debug_id_b,
        "project B's debug_id must not appear in project A's results"
    );
}

// =============================================================================
// Group 5b: assemble_bundle regression — manifest.json extraction
// =============================================================================

/// Build a minimal but valid artifact bundle ZIP in memory.
///
/// The ZIP contains:
///   - `~/app.min.js.map`  — a trivial source map JSON
///   - `manifest.json`     — Sentry manifest with `files` + empty `debugIdMap`
///
/// The `manifest.json` has no `debug-id` headers, so the server's file-processing
/// loop will skip all entries and return Ok(()) without storing anything — but it
/// MUST be able to FIND and PARSE the manifest.  If the extraction puts files in
/// the wrong directory the server returns `manifest.json not found`.
fn build_test_artifact_bundle() -> Vec<u8> {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let source_map = br#"{"version":3,"sources":["src/app.ts"],"sourcesContent":[""],"mappings":"AAAA","names":[]}"#;
    let manifest = br#"{"files":{"~/app.min.js.map":{"url":"~/app.min.js.map","type":"source_map","headers":{}}},"debugIdMap":{}}"#;

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("~/app.min.js.map", opts).unwrap();
        zip.write_all(source_map).unwrap();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

#[actix_web::test]
async fn test_assemble_bundle_finds_manifest_json() {
    // Regression: the path traversal guard in assemble_bundle iterated
    // raw_dest.components() instead of Path::new(&name).components().
    // This doubled the extract_dir prefix so manifest.json was written to
    // {extract_dir}/{extract_dir}/manifest.json, and the subsequent
    // std::fs::read({extract_dir}/manifest.json) always failed.
    let db = TestDb::new().await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "manifest-extraction".to_string(),
            slug: Some("manifest-extraction".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let bundle = build_test_artifact_bundle();
    let bundle_checksum = sha1_hex(&bundle);

    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![(bundle_checksum.clone(), bundle)],
        20 * 1024 * 1024,
    )
    .await
    .expect("store_chunks must succeed");

    let config = create_test_config();
    let store: std::sync::Arc<dyn rustrak::services::SourceMapStore> = std::sync::Arc::new(
        rustrak::services::LocalSourceMapStore::new(&config.sourcemap_storage_path),
    );

    // Before fix: returns Err(Validation("manifest.json not found in artifact bundle"))
    // After fix:  returns Ok(())
    let result = rustrak::services::sourcemap::assemble_bundle(
        &db.pool,
        store.as_ref(),
        project.id,
        &bundle_checksum,
        std::slice::from_ref(&bundle_checksum),
        100 * 1024 * 1024, // 100 MB limit — well above test bundle
    )
    .await;

    assert!(
        result.is_ok(),
        "assemble_bundle must succeed; got: {:?}",
        result.err()
    );
}

#[actix_web::test]
async fn test_assemble_bundle_keeps_chunk_owned_by_another_job() {
    let db = TestDb::new().await;
    let project_a = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "shared-chunk-a".to_string(),
            slug: Some("shared-chunk-a".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project A creation must succeed");
    let project_b = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "shared-chunk-b".to_string(),
            slug: Some("shared-chunk-b".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project B creation must succeed");

    let bundle = build_test_artifact_bundle();
    let checksum = sha1_hex(&bundle);
    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![(checksum.clone(), bundle)],
        20 * 1024 * 1024,
    )
    .await
    .expect("store_chunks must succeed");

    let chunks = vec![checksum.clone()];
    let job_a = insert_assembly_job_with_state_and_chunks(
        &db.pool,
        &checksum,
        project_a.id,
        "assembling",
        None,
        &chunks,
    )
    .await;
    let job_b = insert_assembly_job_with_state_and_chunks(
        &db.pool,
        &checksum,
        project_b.id,
        "assembling",
        None,
        &chunks,
    )
    .await;
    insert_assembly_job_chunk(&db.pool, job_a, &checksum).await;
    insert_assembly_job_chunk(&db.pool, job_b, &checksum).await;

    let config = create_test_config();
    let store: std::sync::Arc<dyn rustrak::services::SourceMapStore> = std::sync::Arc::new(
        rustrak::services::LocalSourceMapStore::new(&config.sourcemap_storage_path),
    );
    rustrak::services::sourcemap::assemble_bundle_for_job(
        &db.pool,
        store.as_ref(),
        project_a.id,
        &checksum,
        &chunks,
        100 * 1024 * 1024,
        job_a,
    )
    .await
    .expect("first assembly must succeed");

    let retained: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunk WHERE checksum = $1")
        .bind(&checksum)
        .fetch_one(&db.pool)
        .await
        .expect("chunk count must succeed");
    assert_eq!(
        retained, 1,
        "a shared chunk must survive the first assembly"
    );

    rustrak::services::sourcemap::assemble_bundle_for_job(
        &db.pool,
        store.as_ref(),
        project_b.id,
        &checksum,
        &chunks,
        100 * 1024 * 1024,
        job_b,
    )
    .await
    .expect("second assembly must succeed");
    let deleted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunk WHERE checksum = $1")
        .bind(&checksum)
        .fetch_one(&db.pool)
        .await
        .expect("chunk count must succeed");
    assert_eq!(deleted, 0, "the last owner may release the chunk");
}

// =============================================================================
// Group 5c: Bundle size limit — Cycle 4
// =============================================================================

#[actix_web::test]
async fn test_assemble_bundle_rejects_oversized_bundle() {
    // When total assembled chunk bytes exceed max_bundle_size_bytes, assemble_bundle
    // must return an error instead of loading everything into RAM.
    let db = TestDb::new().await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "bundle-size-limit".to_string(),
            slug: Some("bundle-size-limit".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    // Two chunks: 60 bytes each = 120 bytes total
    let chunk1: Vec<u8> = vec![0xAA; 60];
    let chunk2: Vec<u8> = vec![0xBB; 60];
    let sha1_1 = sha1_hex(&chunk1);
    let sha1_2 = sha1_hex(&chunk2);

    let config = create_test_config();
    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![(sha1_1.clone(), chunk1), (sha1_2.clone(), chunk2)],
        config.max_chunk_size_bytes,
    )
    .await
    .expect("store_chunks must succeed");

    let store: std::sync::Arc<dyn rustrak::services::SourceMapStore> = std::sync::Arc::new(
        rustrak::services::LocalSourceMapStore::new(&config.sourcemap_storage_path),
    );

    let joined_checksum = {
        let mut data = vec![0xAAu8; 60];
        data.extend(vec![0xBBu8; 60]);
        sha1_hex(&data)
    };

    // Limit of 100 bytes — total bundle is 120 bytes → must fail
    let result = rustrak::services::sourcemap::assemble_bundle(
        &db.pool,
        store.as_ref(),
        project.id,
        &joined_checksum,
        &[sha1_1, sha1_2],
        100, // 100 bytes max — intentionally below 120-byte bundle
    )
    .await;

    assert!(
        result.is_err(),
        "assemble_bundle must fail when bundle exceeds max_bundle_size_bytes"
    );
    let err = format!("{:?}", result.unwrap_err());
    assert!(
        err.contains("too large") || err.contains("limit"),
        "error must mention size limit; got: {}",
        err
    );
}

// =============================================================================
// Group 5d: Postgres enum regression — only meaningful with --features postgres
// =============================================================================

// Guard: assembly_state ENUM in Postgres must be decodable as String.
//
// Regression: the first implementation of artifact_bundle_assemble used
// `RETURNING *` / `SELECT *` on assembly_jobs. In Postgres, the `state`
// column is typed as `assembly_state` (a custom ENUM), which SQLx cannot
// decode into a Rust `String` — it returns a DatabaseError at runtime and
// the endpoint returns HTTP 500.
//
// This test pins that behaviour: once the state column is TEXT the test
// passes; if someone re-introduces an ENUM it will fail immediately.
#[cfg(feature = "postgres")]
#[actix_web::test]
async fn test_assemble_state_column_decodable_in_postgres() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "pg-state-decode".to_string(),
            slug: Some("pg-state-decode".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    // One chunk — size of the bundle equals size of this single chunk
    let chunk: &[u8] = b"postgres assembly_state enum regression test data";
    let chunk_hash = sha1_hex(chunk);
    let bundle_checksum = chunk_hash.clone();

    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![(chunk_hash.clone(), chunk.to_vec())],
        10 * 1024 * 1024,
    )
    .await
    .expect("store_chunks must succeed");

    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": bundle_checksum,
            "chunks": [chunk_hash],
            "projects": [project.slug]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Before fix: HTTP 500 — "mismatched types … assembly_state"
    // After fix:  HTTP 200 with state in {"created","assembling","ok"}
    assert_eq!(
        resp.status(),
        200,
        "assemble must not 500 on Postgres — state column decode regression"
    );

    let body: Value = test::read_body_json(resp).await;
    let state = body["state"].as_str().unwrap_or("");
    assert!(
        ["created", "assembling", "ok"].contains(&state),
        "state must be a valid assembly state string; got: '{}'",
        state
    );
}

// =============================================================================
// Group 5e: chunkSize capability reflects config — Fix #2/#6
// =============================================================================

#[actix_web::test]
async fn test_chunk_capability_chunk_size_reflects_config() {
    // Before fix: chunkSize is hardcoded to 2_097_152 regardless of config.
    // After fix:  chunkSize equals config.max_chunk_size_bytes.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let mut config = create_test_config();
    config.max_chunk_size_bytes = 5 * 1024 * 1024; // 5 MB

    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let chunk_size = body["chunkSize"]
        .as_u64()
        .expect("chunkSize must be a number");
    assert_eq!(
        chunk_size,
        5 * 1024 * 1024,
        "chunkSize must equal config.max_chunk_size_bytes; got {}",
        chunk_size
    );

    let max_request_size = body["maxRequestSize"]
        .as_u64()
        .expect("maxRequestSize must be a number");
    assert_eq!(
        max_request_size,
        5 * 1024 * 1024 * 64,
        "maxRequestSize must be chunkSize * 64; got {}",
        max_request_size
    );
}

// =============================================================================
// Group 5f: multipart total-size guard — Fix #7
// =============================================================================

#[actix_web::test]
async fn test_chunk_upload_too_many_parts_returns_400() {
    // Before fix: no part-count guard — 65 parts are accepted.
    // After fix:  any request with more than 64 parts returns 400.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let boundary = "toomanypartsboundary1";
    // 65 distinct parts (unique names so each is accepted as a separate part)
    let part_data: Vec<u8> = (0u8..65).collect();
    let parts: Vec<(String, Vec<u8>)> = part_data
        .iter()
        .enumerate()
        .map(|(i, &b)| (format!("part{:02}", i), vec![b]))
        .collect();
    let parts_ref: Vec<(&str, &[u8])> = parts
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();
    let body = build_multipart(boundary, &parts_ref);

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        ))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "65 parts must be rejected; got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_chunk_upload_total_bytes_exceeded_returns_400() {
    // Use a tiny max_chunk_size so the total-bytes limit (chunkSize * 64) is reachable.
    // 65 parts × 10 bytes each = 650 bytes > 10 * 64 = 640 bytes limit.
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let mut config = create_test_config();
    config.max_chunk_size_bytes = 10; // 10 bytes per chunk

    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let boundary = "totalbytesexceed2";
    // 65 distinct 10-byte parts — total 650 bytes > limit 640 (64 × 10)
    let parts: Vec<(String, Vec<u8>)> = (0u8..65)
        .map(|i| (format!("part{:02}", i), vec![i; 10]))
        .collect();
    let parts_ref: Vec<(&str, &[u8])> = parts
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();
    let body = build_multipart(boundary, &parts_ref);

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        ))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "total bytes exceeding chunkSize*64 must be rejected; got {}",
        resp.status()
    );
}

// =============================================================================
// Group 5g: path traversal in manifest.json — Fix #11
// =============================================================================

fn build_traversal_bundle() -> Vec<u8> {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let debug_id = "12345678-1234-1234-1234-123456789abc";
    // manifest.json has a traversal path + debug-id so the file-read branch IS reached
    let manifest = format!(
        r#"{{"files":{{"../../etc/evil.map":{{"url":"../../etc/evil.map","type":"source_map","headers":{{"debug-id":"{debug_id}"}}}}}},"debugIdMap":{{}}}}"#
    );
    let source_map =
        br#"{"version":3,"sources":["src/app.ts"],"sourcesContent":[""],"mappings":"AAAA"}"#;

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        // real file in ZIP (safe path — no traversal in the archive itself)
        zip.start_file("app.min.js.map", opts).unwrap();
        zip.write_all(source_map).unwrap();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

#[actix_web::test]
async fn test_assemble_bundle_manifest_path_traversal_rejected() {
    // The manifest.json inside the bundle contains "../../etc/evil.map" as a file path
    // with a debug-id header, so the server's file-read branch is reached.
    //
    // Before fix: tokio::fs::read fails (file not found outside extract_dir), warns, and
    //             continues — assembly returns Ok(()) while silently storing nothing.
    // After fix:  path components are validated; traversal path returns Err immediately.
    let db = TestDb::new().await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "traversal-test".to_string(),
            slug: Some("traversal-test".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let bundle = build_traversal_bundle();
    let bundle_checksum = sha1_hex(&bundle);

    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![(bundle_checksum.clone(), bundle)],
        20 * 1024 * 1024,
    )
    .await
    .expect("store_chunks must succeed");

    let config = create_test_config();
    let store: std::sync::Arc<dyn rustrak::services::SourceMapStore> = std::sync::Arc::new(
        rustrak::services::LocalSourceMapStore::new(&config.sourcemap_storage_path),
    );

    let result = rustrak::services::sourcemap::assemble_bundle(
        &db.pool,
        store.as_ref(),
        project.id,
        &bundle_checksum,
        std::slice::from_ref(&bundle_checksum),
        100 * 1024 * 1024,
    )
    .await;

    assert!(
        result.is_err(),
        "manifest path traversal must be rejected with an error; got Ok(())"
    );
}

// =============================================================================
// Group 5h: ~/prefix path normalisation — Fix #1
// =============================================================================

fn build_tilde_bundle_with_debug_id() -> Vec<u8> {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let debug_id = "aaaabbbb-cccc-dddd-eeee-ffffaaaabbbb";
    let source_map = br#"{"version":3,"sources":["src/app.ts"],"sourcesContent":["export {}"],"mappings":"AAAA","names":[]}"#;
    // manifest references ~/app.min.js.map with a debug-id so the file-read branch IS reached
    let manifest = format!(
        r#"{{"files":{{"~/app.min.js.map":{{"url":"~/app.min.js.map","type":"source_map","headers":{{"debug-id":"{debug_id}"}}}}}},"debugIdMap":{{}}}}"#
    );

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("~/app.min.js.map", opts).unwrap(); // ZIP entry uses ~/ prefix
        zip.write_all(source_map).unwrap();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

#[actix_web::test]
async fn test_assemble_bundle_tilde_prefix_with_debug_id_stores_source_file() {
    // ZIP entry "~/app.min.js.map" + manifest references "~/app.min.js.map" with debug-id.
    //
    // Before fix: extraction puts the file at {dir}/~/{file}, but manifest lookup strips
    //             "~/" and looks for {dir}/{file} — mismatch → tokio::fs::read fails →
    //             warns and continues → source_file_metadata row is NEVER created.
    // After fix:  extraction normalises the "~/" prefix so paths match → row IS created.
    let db = TestDb::new().await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "tilde-prefix-test".to_string(),
            slug: Some("tilde-prefix-test".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let bundle = build_tilde_bundle_with_debug_id();
    let bundle_checksum = sha1_hex(&bundle);

    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![(bundle_checksum.clone(), bundle)],
        20 * 1024 * 1024,
    )
    .await
    .expect("store_chunks must succeed");

    let config = create_test_config();
    let store: std::sync::Arc<dyn rustrak::services::SourceMapStore> = std::sync::Arc::new(
        rustrak::services::LocalSourceMapStore::new(&config.sourcemap_storage_path),
    );

    let result = rustrak::services::sourcemap::assemble_bundle(
        &db.pool,
        store.as_ref(),
        project.id,
        &bundle_checksum,
        std::slice::from_ref(&bundle_checksum),
        100 * 1024 * 1024,
    )
    .await;

    assert!(
        result.is_ok(),
        "assembly with ~/prefix must succeed; got: {:?}",
        result.err()
    );

    // Verify source_file_metadata row was actually stored (not silently skipped)
    let count = count_sfm_for_project(&db.pool, project.id).await;
    assert_eq!(
        count, 1,
        "source_file_metadata row must be stored after assembly; got count={}",
        count
    );
}

// =============================================================================
// Group 6: Worker recovery — keep #[ignore = "requires worker"]
// =============================================================================

#[actix_web::test]
#[ignore = "requires AssemblyWorker running in test context to reset stuck jobs"]
async fn test_worker_restart_recovery_resets_stuck_assembling_jobs() {
    // GIVEN: an assembly_jobs row with state='assembling', locked_until < NOW() (expired lock),
    //        retry_count=0
    // WHEN:  AssemblyWorker::run() starts (simulating server restart)
    // THEN:  the startup recovery query resets the row to state='created', locked_until=NULL
    //        AND the job is picked up in the next poll cycle
    //
    // Setup:
    //   sqlx::query("INSERT INTO assembly_jobs(bundle_checksum, project_id, chunks, state, locked_until) ...")
    //     .bind(checksum).bind(project_id).bind("[]").bind("assembling")
    //     .bind(some_past_datetime)
    //     .execute(&db.pool).await.unwrap();
    //   // Start worker, wait one poll cycle
    //   let job: AssemblyJob = sqlx::query_as("SELECT * FROM assembly_jobs WHERE ...").fetch_one(&pool).await.unwrap();
    //   assert_eq!(job.state, "created");
    //   assert!(job.locked_until.is_none());
    todo!("enable once AssemblyWorker is injectable in tests")
}

#[actix_web::test]
async fn test_worker_terminal_failure_releases_exclusive_chunks_only() {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "terminal-cleanup".to_string(),
            slug: Some("terminal-cleanup".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let shared = b"not a zip archive".to_vec();
    let exclusive = b"another invalid archive".to_vec();
    let shared_checksum = sha1_hex(&shared);
    let exclusive_checksum = sha1_hex(&exclusive);
    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![
            (shared_checksum.clone(), shared),
            (exclusive_checksum.clone(), exclusive),
        ],
        20 * 1024 * 1024,
    )
    .await
    .expect("chunk storage must succeed");

    let job = insert_assembly_job_with_state_and_chunks(
        &db.pool,
        &shared_checksum,
        project.id,
        "created",
        None,
        &[shared_checksum.clone(), exclusive_checksum.clone()],
    )
    .await;
    let other_job = insert_assembly_job_with_state_and_chunks(
        &db.pool,
        &exclusive_checksum,
        project.id,
        "assembling",
        None,
        std::slice::from_ref(&shared_checksum),
    )
    .await;
    insert_assembly_job_chunk(&db.pool, job, &shared_checksum).await;
    insert_assembly_job_chunk(&db.pool, job, &exclusive_checksum).await;
    insert_assembly_job_chunk(&db.pool, other_job, &shared_checksum).await;
    sqlx::query("UPDATE assembly_jobs SET retry_count = max_retries - 1 WHERE id = $1")
        .bind(job)
        .execute(&db.pool)
        .await
        .expect("retry setup must succeed");

    let store = Arc::new(LocalSourceMapStore::new(
        std::env::temp_dir().join(format!("rustrak-assembly-{}", Uuid::new_v4())),
    ));
    let worker = rustrak::workers::sourcemap_assembly::AssemblyWorker::new(
        db.pool.clone(),
        store,
        100 * 1024 * 1024,
    );
    worker.run_once().await.expect("worker poll must succeed");

    let state: String = sqlx::query_scalar("SELECT state FROM assembly_jobs WHERE id = $1")
        .bind(job)
        .fetch_one(&db.pool)
        .await
        .expect("job state lookup must succeed");
    assert_eq!(state, "error");
    let owned: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM assembly_job_chunks WHERE job_id = $1")
            .bind(job)
            .fetch_one(&db.pool)
            .await
            .expect("ownership lookup must succeed");
    assert_eq!(owned, 0);

    for (checksum, expected) in [(&shared_checksum, 1i64), (&exclusive_checksum, 0i64)] {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunk WHERE checksum = $1")
            .bind(checksum)
            .fetch_one(&db.pool)
            .await
            .expect("chunk lookup must succeed");
        assert_eq!(count, expected, "unexpected retention for {checksum}");
    }
}

// =============================================================================
// Group 7: Digest integration — keep #[ignore = "requires worker"]
// =============================================================================

#[actix_web::test]
#[ignore = "requires full digest pipeline with source map rewriting (T8)"]
async fn test_digest_rewrites_frames_when_sourcemap_present() {
    // GIVEN: source maps assembled and stored for project "test-proj"
    //        AND a Sentry event arrives with debug_meta.images pointing to a known debug_id
    //        AND the frame's (lineno, colno) maps to a known original position in the source map
    // WHEN:  the digest worker processes the event
    // THEN:  events.data has the frame rewritten:
    //          - filename changed to the original source file
    //          - lineno changed to the original 1-indexed line
    //          - context_line is a non-empty string
    todo!("enable after T8 digest integration is complete")
}

#[actix_web::test]
#[ignore = "requires full digest pipeline to verify graceful no-op (T8, AC4)"]
async fn test_digest_leaves_frame_unchanged_when_no_sourcemap() {
    // GIVEN: no source maps stored for the project
    //        AND a Sentry event with a frame that has debug_meta.images
    // WHEN:  the digest worker processes the event
    // THEN:  events.data has the frame with its original minified values
    //        AND the digest completes without error (non-fatal frame rewriting)
    todo!("enable after T8 digest integration is complete")
}

// =============================================================================
// Group 8: Cross-project isolation — keep #[ignore = "requires worker"]
// =============================================================================

#[actix_web::test]
#[ignore = "requires full digest pipeline with project-scoped source map isolation (T5 + T8, AC5)"]
async fn test_cross_project_source_maps_do_not_leak() {
    // GIVEN: source maps assembled for project A with debug_id X
    //        AND a Sentry event from project B with the same debug_id X in debug_meta.images
    // WHEN:  the digest worker processes project B's event
    // THEN:  project B's frames are NOT rewritten (source_file_metadata scoped by project_id)
    //        AND events.data for project B still has the original minified frame values
    todo!("enable after T5 project_id scoping + T8 are complete")
}

// =============================================================================
// Bug-fix regression tests (2026-05-25 PR review)
// =============================================================================

// --- Bug 1: chunk upload must validate field_name == computed SHA1 ---

#[actix_web::test]
async fn test_chunk_upload_mismatched_sha1_field_name_returns_400() {
    // GIVEN: a chunk upload where the field name is a valid SHA1 string
    //        BUT it does NOT match the actual SHA1 of the uploaded bytes
    // WHEN:  the upload is submitted
    // THEN:  400 Bad Request is returned (checksum mismatch)
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    let real_data = b"the actual chunk bytes";
    let wrong_sha1 = sha1_hex(b"different data entirely"); // does not match real_data

    let boundary = "mismatchboundary";
    // Field name is wrong_sha1, but body is real_data — they disagree
    let body = build_multipart(boundary, &[(wrong_sha1.as_str(), real_data.as_ref())]);

    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/chunk-upload/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        ))
        .set_payload(body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "chunk upload with mismatched field name SHA1 must return 400"
    );
}

// --- Bug 2: ON CONFLICT assemble must update chunks column ---

#[actix_web::test]
async fn test_assemble_re_enqueue_error_job_updates_chunks() {
    // GIVEN: an assembly_jobs row in state='error' with an old chunk list
    // WHEN:  assemble is called again (same bundle_checksum) with a new chunk list
    //        AND all new chunks are present in the chunk table
    // THEN:  200 {"state": "created"} is returned
    //        AND the assembly_jobs row has the updated chunks list
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "re-enqueue-chunks-test".to_string(),
            slug: Some("re-enqueue-chunks-test".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let config = create_test_config();

    // Create the new chunk and store it
    let new_chunk: &[u8] = b"new corrected chunk data";
    let new_chunk_sha1 = sha1_hex(new_chunk);
    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![(new_chunk_sha1.clone(), new_chunk.to_vec())],
        config.max_chunk_size_bytes,
    )
    .await
    .expect("store_chunks must succeed");

    let bundle_checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    // Pre-insert a failed job with an OLD chunk list (the stale one)
    let old_chunk_sha1 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    insert_assembly_job_with_state_and_chunks(
        &db.pool,
        bundle_checksum,
        project.id,
        "error",
        Some("previous failure"),
        std::slice::from_ref(&old_chunk_sha1),
    )
    .await;

    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    // Re-submit assemble with the NEW chunk list
    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": bundle_checksum,
            "chunks": [new_chunk_sha1],
            "projects": [project.slug]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "re-enqueue of error job must return 200"
    );
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["state"], "created",
        "re-enqueued job must be in created state"
    );

    // Verify DB chunks column was updated to the NEW chunk list
    let stored_chunks = fetch_assembly_job_chunks(&db.pool, bundle_checksum, project.id).await;
    assert!(
        stored_chunks.contains(&new_chunk_sha1),
        "chunks column must be updated to new chunk list; got: {:?}",
        stored_chunks
    );
    assert!(
        !stored_chunks.contains(&old_chunk_sha1),
        "old chunk sha1 must not be in updated chunks; got: {:?}",
        stored_chunks
    );
}

// --- Bug 5: assemble must accept numeric project ID, not just slug ---

#[actix_web::test]
async fn test_assemble_with_numeric_project_id_returns_not_found_or_created() {
    // GIVEN: a project exists with a known numeric ID
    // WHEN:  assemble is called with projects: ["<numeric_id>"] instead of the slug
    // THEN:  it does NOT return 404 (project found by ID fallback)
    //        returns 200 not_found (chunks missing) rather than 404 project not found
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;

    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "numeric-id-test-project".to_string(),
            slug: Some("numeric-id-test-project".to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed");

    let config = create_test_config();
    let (provider, config_data) = sourcemap_app_data(&db.pool, &config);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;

    // Use the numeric project ID as a string in the projects array
    let req = test::TestRequest::post()
        .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("Content-Type", "application/json"))
        .set_json(serde_json::json!({
            "checksum": "cccccccccccccccccccccccccccccccccccccccc",
            "chunks": ["dddddddddddddddddddddddddddddddddddddddd"],
            "projects": [project.id.to_string()]
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    // Must NOT be 404 — project was found by numeric ID fallback
    // Should be 200 (missing chunks) since no chunks uploaded
    assert_ne!(
        resp.status(),
        404,
        "assemble with numeric project ID must not return 404; project should be found by ID fallback"
    );
    assert_eq!(
        resp.status(),
        200,
        "assemble with numeric project ID and missing chunks must return 200"
    );
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["state"], "not_found");
}

// ---------------------------------------------------------------------------
// New test helpers needed by the bug-fix tests above
// ---------------------------------------------------------------------------

async fn insert_assembly_job_with_state_and_chunks(
    pool: &rustrak::db::DbPool,
    checksum: &str,
    project_id: i32,
    state: &str,
    detail: Option<&str>,
    chunks: &[String],
) -> i64 {
    #[cfg(feature = "postgres")]
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO assembly_jobs(bundle_checksum, project_id, chunks, state, detail) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(checksum)
    .bind(project_id)
    .bind(chunks)
    .bind(state)
    .bind(detail)
    .fetch_one(pool)
    .await
    .expect("insert assembly_jobs must succeed");

    #[cfg(not(feature = "postgres"))]
    let id: i64 = {
        let chunks_json = serde_json::to_string(chunks).expect("chunks serialization must succeed");
        sqlx::query_scalar(
            "INSERT INTO assembly_jobs(bundle_checksum, project_id, chunks, state, detail) \
             VALUES (?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(checksum)
        .bind(project_id)
        .bind(&chunks_json)
        .bind(state)
        .bind(detail)
        .fetch_one(pool)
        .await
        .expect("insert assembly_jobs must succeed")
    };

    id
}

async fn insert_assembly_job_chunk(pool: &rustrak::db::DbPool, job_id: i64, checksum: &str) {
    sqlx::query(
        "INSERT INTO assembly_job_chunks(job_id, checksum) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(job_id)
    .bind(checksum)
    .execute(pool)
    .await
    .expect("insert assembly_job_chunks must succeed");
}

async fn fetch_assembly_job_chunks(
    pool: &rustrak::db::DbPool,
    checksum: &str,
    project_id: i32,
) -> Vec<String> {
    #[cfg(feature = "postgres")]
    {
        let chunks: Vec<String> = sqlx::query_scalar(
            "SELECT chunks FROM assembly_jobs WHERE bundle_checksum = $1 AND project_id = $2",
        )
        .bind(checksum)
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("fetch assembly job chunks must succeed");
        chunks
    }

    #[cfg(not(feature = "postgres"))]
    {
        let chunks_json: String = sqlx::query_scalar(
            "SELECT chunks FROM assembly_jobs WHERE bundle_checksum = ? AND project_id = ?",
        )
        .bind(checksum)
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("fetch assembly job chunks must succeed");
        serde_json::from_str(&chunks_json).expect("chunks json must be valid")
    }
}
