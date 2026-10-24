//! Debug information files (DIFs): `POST .../files/difs/assemble/` and the
//! worker that stores them, for ProGuard mappings and JVM source bundles.
//!
//! The endpoint's contract follows Sentry's `DifAssembleEndpoint`
//! (`src/sentry/api/endpoints/debug_files.py`, 2458718b2c) and its tests in
//! `tests/sentry/api/endpoints/test_dif_assemble.py`.

use std::io::Write as _;
use std::sync::Arc;

use actix_web::{test, web, App};
use rustrak::models::CreateProject;
use rustrak::routes;
use rustrak::services::{LocalSourceMapStore, ProjectService, SourceMapProvider};
use serde_json::{json, Value};
use uuid::Uuid;

use super::sourcemaps_api_test::{
    create_test_config, create_test_token, sha1_hex, sourcemap_app_data,
};
use crate::common::TestDb;

const MAPPING_UUID: &str = "11111111-2222-3333-4444-555555555555";
const BUNDLE_ID: &str = "66666666-7777-8888-9999-000000000000";

const MAPPING: &[u8] = b"com.example.Greeter -> a.a:
    1:1:java.lang.String greet(java.lang.String):6:6 -> a
";

/// A real `sentry-cli debug-files bundle-jvm` output (see `tests/unit/jvm_test.rs`).
fn source_bundle() -> Vec<u8> {
    include_bytes!("../fixtures/jvm_source_bundle.zip").to_vec()
}

async fn project(db: &TestDb, slug: &str) -> i32 {
    ProjectService::create(
        &db.pool,
        CreateProject {
            name: slug.to_string(),
            slug: Some(slug.to_string()),
            platform: None,
        },
    )
    .await
    .expect("project creation must succeed")
    .id
}

async fn store_chunk(db: &TestDb, data: &[u8]) -> String {
    let checksum = sha1_hex(data);
    rustrak::services::sourcemap::store_chunks(
        &db.pool,
        vec![(checksum.clone(), data.to_vec())],
        10 * 1024 * 1024,
    )
    .await
    .expect("store_chunks must succeed");
    checksum
}

async fn assemble(db: &TestDb, token: &str, slug: &str, body: Value) -> (u16, Value) {
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
        .uri(&format!(
            "/api/0/projects/my-org/{slug}/files/difs/assemble/"
        ))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    let body: Value = serde_json::from_slice(&test::read_body(resp).await).unwrap_or(Value::Null);
    (status, body)
}

async fn run_worker(db: &TestDb) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let store = Arc::new(LocalSourceMapStore::new(dir.path()));
    rustrak::workers::sourcemap_assembly::AssemblyWorker::new(db.pool.clone(), store, 100 << 20)
        .run_once()
        .await
        .expect("worker poll must succeed");
    dir
}

async fn file_types(db: &TestDb, project_id: i32) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT CAST(debug_id AS TEXT), file_type FROM source_file_metadata WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_all(&db.pool)
    .await
    .expect("metadata lookup must succeed");
    rows.sort();
    rows
}

#[actix_web::test]
async fn chunk_upload_advertises_debug_files_and_proguard() {
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
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let accept: Vec<&str> = body["accept"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    // Without these, sentry-cli 2.x falls back to the legacy `files/dsyms/` upload.
    assert!(accept.contains(&"debug_files"), "{accept:?}");
    assert!(accept.contains(&"proguard"), "{accept:?}");
}

#[actix_web::test]
async fn assemble_with_no_chunks_is_not_found() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    project(&db, "difs-empty").await;
    let checksum = sha1_hex(b"1");

    let (status, body) = assemble(
        &db,
        &token,
        "difs-empty",
        json!({checksum.clone(): {"name": "dif", "chunks": []}}),
    )
    .await;

    assert_eq!(status, 200);
    assert_eq!(body[&checksum]["state"], "not_found");
    assert_eq!(body[&checksum]["missingChunks"], json!([]));
}

#[actix_web::test]
async fn assemble_lists_missing_chunks() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    project(&db, "difs-missing").await;
    let checksum = sha1_hex(MAPPING);

    let (status, body) = assemble(
        &db,
        &token,
        "difs-missing",
        json!({checksum.clone(): {"name": "dif", "chunks": [checksum.clone()]}}),
    )
    .await;

    assert_eq!(status, 200);
    assert_eq!(body[&checksum]["state"], "not_found");
    assert_eq!(body[&checksum]["missingChunks"], json!([checksum]));
}

#[actix_web::test]
async fn assemble_rejects_a_malformed_body() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    project(&db, "difs-bad").await;
    let checksum = sha1_hex(b"1");

    for body in [
        json!({"not-a-checksum": {"name": "dif", "chunks": []}}),
        json!({checksum.clone(): {"chunks": []}}),
        json!({checksum.clone(): {"name": "dif", "debug_id": "invalid-debug-id", "chunks": []}}),
    ] {
        let (status, _) = assemble(&db, &token, "difs-bad", body.clone()).await;
        assert_eq!(status, 400, "{body}");
    }
}

#[actix_web::test]
async fn assemble_unknown_project_is_404() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let checksum = sha1_hex(b"1");

    let (status, _) = assemble(
        &db,
        &token,
        "no-such-project",
        json!({checksum: {"name": "dif", "chunks": []}}),
    )
    .await;

    assert_eq!(status, 404);
}

#[actix_web::test]
async fn proguard_mapping_is_stored_with_its_cache() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-proguard").await;
    let chunk = store_chunk(&db, MAPPING).await;
    // sentry-cli names a mapping after its UUID; that is where the debug ID comes from.
    let body = json!({chunk.clone(): {
        "name": format!("/proguard/{MAPPING_UUID}.txt"),
        "chunks": [chunk.clone()]
    }});

    let (_, created) = assemble(&db, &token, "difs-proguard", body.clone()).await;
    assert_eq!(created[&chunk]["state"], "created");
    run_worker(&db).await;
    let (_, done) = assemble(&db, &token, "difs-proguard", body).await;

    assert_eq!(done[&chunk]["state"], "ok", "{done}");
    assert_eq!(
        file_types(&db, project_id).await,
        [
            (MAPPING_UUID.to_string(), "proguard".to_string()),
            (MAPPING_UUID.to_string(), "proguard_cache".to_string()),
        ]
    );
}

#[actix_web::test]
async fn source_bundle_is_stored_under_its_manifest_debug_id() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-bundle").await;
    let bundle = source_bundle();
    let chunk = store_chunk(&db, &bundle).await;
    let body = json!({chunk.clone(): {"name": "Greeter.zip", "chunks": [chunk.clone()]}});

    assemble(&db, &token, "difs-bundle", body.clone()).await;
    run_worker(&db).await;
    let (_, done) = assemble(&db, &token, "difs-bundle", body).await;

    assert_eq!(done[&chunk]["state"], "ok", "{done}");
    assert_eq!(
        file_types(&db, project_id).await,
        [(BUNDLE_ID.to_string(), "source_bundle".to_string())]
    );
}

#[actix_web::test]
async fn an_unsupported_file_fails_with_a_detail() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    project(&db, "difs-unknown").await;
    let data = b"\xcf\xfa\xed\xfe not really a mach-o";
    let chunk = store_chunk(&db, data).await;
    let body = json!({chunk.clone(): {"name": "App.dSYM", "chunks": [chunk.clone()]}});

    assemble(&db, &token, "difs-unknown", body.clone()).await;
    sqlx::query("UPDATE assembly_jobs SET max_retries = 1")
        .execute(&db.pool)
        .await
        .unwrap();
    run_worker(&db).await;
    let (_, done) = assemble(&db, &token, "difs-unknown", body).await;

    assert_eq!(done[&chunk]["state"], "error", "{done}");
    assert!(done[&chunk]["detail"].is_string(), "{done}");
}

#[actix_web::test]
async fn an_invalid_proguard_mapping_fails() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    project(&db, "difs-invalid").await;
    let chunk = store_chunk(&db, b"not a mapping at all").await;
    let body = json!({chunk.clone(): {
        "name": format!("/proguard/mapping-{MAPPING_UUID}.txt"),
        "chunks": [chunk.clone()]
    }});

    assemble(&db, &token, "difs-invalid", body.clone()).await;
    sqlx::query("UPDATE assembly_jobs SET max_retries = 1")
        .execute(&db.pool)
        .await
        .unwrap();
    run_worker(&db).await;
    let (_, done) = assemble(&db, &token, "difs-invalid", body).await;

    assert_eq!(done[&chunk]["state"], "error", "{done}");
}

#[actix_web::test]
async fn stored_files_deobfuscate_an_event() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-event").await;
    let chunk = store_chunk(&db, MAPPING).await;
    let body = json!({chunk.clone(): {
        "name": format!("/proguard/{MAPPING_UUID}.txt"),
        "chunks": [chunk.clone()]
    }});
    assemble(&db, &token, "difs-event", body).await;
    let dir = run_worker(&db).await;

    let store = Arc::new(LocalSourceMapStore::new(dir.path()));
    let provider: Arc<dyn SourceMapProvider> = Arc::new(
        rustrak::services::DbSourceMapProvider::new(db.pool.clone(), store),
    );
    let mut event = json!({
        "platform": "java",
        "release": "com.example@1.0+1",
        "exception": {"values": [{"type": "IllegalStateException", "module": "java.lang", "stacktrace": {"frames": [
            {"function": "a", "module": "a.a", "filename": "SourceFile", "lineno": 1}
        ]}}]},
        "debug_meta": {"images": [{"type": "proguard", "uuid": MAPPING_UUID}]}
    });

    rustrak::services::jvm::process_event(provider.as_ref(), project_id, &mut event).await;

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["module"], "com.example.Greeter");
    assert_eq!(frame["function"], "greet");
    assert_eq!(frame["lineno"], 6);
    assert_eq!(frame["in_app"], true, "release package com.example");
}

/// Two builds obfuscate the same crash differently. Sentry groups after
/// deobfuscating, so both land in one issue named after the real class.
#[actix_web::test]
async fn digest_groups_builds_by_their_deobfuscated_crash() {
    use rustrak::ingest::{store_event, EventMetadata};

    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-digest").await;
    let builds = [
        ("aaaaaaaa-0000-0000-0000-000000000001", "a.b", "a.a"),
        ("aaaaaaaa-0000-0000-0000-000000000002", "x.y", "x.x"),
    ];
    for (uuid, error_class, greeter_class) in builds {
        let mapping = format!(
            "com.example.CrashError -> {error_class}:\n    1:1:void <init>():3:3 -> <init>\n\
             com.example.Greeter -> {greeter_class}:\n    1:1:java.lang.String greet(java.lang.String):6:6 -> a\n"
        );
        let chunk = store_chunk(&db, mapping.as_bytes()).await;
        let body =
            json!({chunk.clone(): {"name": format!("/proguard/{uuid}.txt"), "chunks": [chunk]}});
        assemble(&db, &token, "difs-digest", body).await;
    }
    let dir = tempfile::TempDir::new().expect("temp dir");
    let store = Arc::new(LocalSourceMapStore::new(dir.path()));
    let worker = rustrak::workers::sourcemap_assembly::AssemblyWorker::new(
        db.pool.clone(),
        store.clone(),
        100 << 20,
    );
    worker.run_once().await.unwrap();
    worker.run_once().await.unwrap();
    let provider: Arc<dyn SourceMapProvider> = Arc::new(
        rustrak::services::DbSourceMapProvider::new(db.pool.clone(), store),
    );

    let ingest = tempfile::TempDir::new().unwrap();
    let rate_limit = rustrak::config::RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    for (uuid, error_class, greeter_class) in builds {
        let (module, ty) = error_class.rsplit_once('.').unwrap();
        let event_id = Uuid::new_v4().simple().to_string();
        let event = json!({
            "event_id": event_id,
            "timestamp": chrono::Utc::now().timestamp() as f64,
            "platform": "java",
            "level": "error",
            "exception": {"values": [{"type": ty, "module": module, "value": "boom", "stacktrace": {"frames": [
                {"function": "a", "module": greeter_class, "filename": "SourceFile", "lineno": 1}
            ]}}]},
            "debug_meta": {"images": [{"type": "proguard", "uuid": uuid}]}
        });
        store_event(
            ingest.path(),
            &event_id,
            &serde_json::to_vec(&event).unwrap(),
        )
        .await
        .unwrap();
        let metadata = EventMetadata {
            event_id,
            project_id,
            ingested_at: chrono::Utc::now(),
            remote_addr: None,
        };
        crate::common::process_error_event(
            &db.pool,
            &metadata,
            ingest.path(),
            &rate_limit,
            Arc::clone(&provider),
        )
        .await
        .expect("digest must succeed");
    }

    let issues: Vec<(String,)> =
        sqlx::query_as("SELECT calculated_type FROM issues WHERE project_id = $1")
            .bind(project_id)
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(issues, [("CrashError".to_string(),)]);
}

fn zip_of(entries: &[(String, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in entries {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

async fn legacy_upload(
    db: &TestDb,
    token: &str,
    slug: &str,
    config: &rustrak::config::Config,
    zip: &[u8],
) -> actix_web::dev::ServiceResponse {
    let boundary = "difboundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"mapping.zip\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend(zip);
    body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());

    let (provider, config_data) = sourcemap_app_data(&db.pool, config);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(config_data)
            .app_data(web::Data::new(Arc::clone(&provider)))
            .configure(routes::sourcemaps::configure),
    )
    .await;
    let req = test::TestRequest::post()
        .uri(&format!("/api/0/projects/my-org/{slug}/files/dsyms/"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(body)
        .to_request();
    test::call_service(&app, req).await
}

/// `sentry-cli upload-proguard` (2.58) skips chunk upload: it posts a zip
/// holding `proguard/<uuid>.txt` to the legacy `files/dsyms/` endpoint and
/// reads back the DIFs created (Sentry's `DebugFilesEndpoint.post`).
#[actix_web::test]
async fn legacy_upload_queues_a_proguard_mapping() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-legacy").await;
    let zip = zip_of(&[(format!("proguard/{MAPPING_UUID}.txt"), MAPPING)]);

    let resp = legacy_upload(&db, &token, "difs-legacy", &create_test_config(), &zip).await;
    assert_eq!(resp.status(), 201);
    let difs: Value = test::read_body_json(resp).await;
    assert_eq!(difs[0]["debugId"], MAPPING_UUID);
    assert_eq!(difs[0]["uuid"], MAPPING_UUID);
    assert_eq!(difs[0]["symbolType"], "proguard");
    assert_eq!(difs[0]["objectName"], "proguard-mapping");
    assert_eq!(difs[0]["sha1"], sha1_hex(MAPPING));

    run_worker(&db).await;

    assert_eq!(
        file_types(&db, project_id).await,
        [
            (MAPPING_UUID.to_string(), "proguard".to_string()),
            (MAPPING_UUID.to_string(), "proguard_cache".to_string()),
        ]
    );
}

const DSYM_ID: &str = "821b5334-aca1-3d75-bea8-579e8f53103a";

/// What `sentry-cli debug-files upload` sends for a dSYM: the DWARF file,
/// named after its path in the bundle, with the object's debug ID.
async fn upload_dsym_files(db: &TestDb, token: &str, slug: &str) {
    for (name, data) in [
        (
            "Contents/Resources/DWARF/crash",
            include_bytes!("../fixtures/crash_arm64.dwarf").as_slice(),
        ),
        (
            "crash.src.zip",
            include_bytes!("../fixtures/crash_arm64.src.zip").as_slice(),
        ),
    ] {
        let chunk = store_chunk(db, data).await;
        let body = json!({chunk.clone(): {"name": name, "debug_id": DSYM_ID, "chunks": [chunk]}});
        assemble(db, token, slug, body).await;
    }
}

#[actix_web::test]
async fn dsym_is_stored_with_its_symcache() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-dsym").await;
    let data = include_bytes!("../fixtures/crash_arm64.dwarf");
    let chunk = store_chunk(&db, data).await;
    let body = json!({chunk.clone(): {
        "name": "Contents/Resources/DWARF/crash",
        "debug_id": DSYM_ID,
        "chunks": [chunk.clone()]
    }});

    assemble(&db, &token, "difs-dsym", body.clone()).await;
    let _dir = run_worker(&db).await;
    let (_, done) = assemble(&db, &token, "difs-dsym", body).await;

    assert_eq!(done[&chunk]["state"], "ok", "{done}");
    assert_eq!(
        file_types(&db, project_id).await,
        [
            (DSYM_ID.to_string(), "macho".to_string()),
            (DSYM_ID.to_string(), "symcache".to_string()),
        ]
    );
}

/// The app's executable after its dSYM, same UUID but only a symbol table
/// (`crash_arm64.exe`: `crash.c` built without `-g`, its LC_UUID set to the
/// dSYM's). Sentry keeps both files and Symbolicator prefers the one with
/// debug info (`object_quality`), so the DWARF SymCache stays.
#[actix_web::test]
async fn an_executable_does_not_replace_its_dsym_symcache() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-exe").await;
    let symcache_file = || async {
        let (id,): (String,) = sqlx::query_as(
            "SELECT CAST(file_id AS TEXT) FROM source_file_metadata
             WHERE project_id = $1 AND file_type = 'symcache'",
        )
        .bind(project_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        id
    };
    let mut done = Value::Null;
    let mut files = Vec::new();
    for (name, data) in [
        (
            "Contents/Resources/DWARF/crash",
            include_bytes!("../fixtures/crash_arm64.dwarf").as_slice(),
        ),
        (
            "Crash.app/Crash",
            include_bytes!("../fixtures/crash_arm64.exe").as_slice(),
        ),
    ] {
        let chunk = store_chunk(&db, data).await;
        let body =
            json!({chunk.clone(): {"name": name, "debug_id": DSYM_ID, "chunks": [chunk.clone()]}});
        assemble(&db, &token, "difs-exe", body.clone()).await;
        run_worker(&db).await;
        done = assemble(&db, &token, "difs-exe", body).await.1;
        assert_eq!(done[&chunk]["state"], "ok", "{done}");
        files.push(symcache_file().await);
    }

    assert_eq!(files[0], files[1], "{done}");
}

/// An iOS crash in the app's own code, digested with its dSYM and sources
/// stored: symbolicated before grouping, so the issue is named after it.
#[actix_web::test]
async fn digest_symbolicates_an_ios_crash() {
    use rustrak::ingest::{store_event, EventMetadata};

    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-ios").await;
    upload_dsym_files(&db, &token, "difs-ios").await;
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(LocalSourceMapStore::new(dir.path()));
    let worker = rustrak::workers::sourcemap_assembly::AssemblyWorker::new(
        db.pool.clone(),
        store.clone(),
        100 << 20,
    );
    worker.run_once().await.unwrap();
    worker.run_once().await.unwrap();
    let provider: Arc<dyn SourceMapProvider> = Arc::new(
        rustrak::services::DbSourceMapProvider::new(db.pool.clone(), store),
    );

    let event_id = Uuid::new_v4().simple().to_string();
    let event = json!({
        "event_id": event_id,
        "timestamp": chrono::Utc::now().timestamp() as f64,
        "platform": "cocoa",
        "level": "fatal",
        "contexts": {"os": {"type": "os", "name": "iOS"}},
        "exception": {"values": [{
            "type": "EXC_ARITHMETIC", "value": "divide by zero",
            "mechanism": {"type": "mach", "handled": false},
            "stacktrace": {"frames": [
                {"instruction_addr": "0x104000430", "in_app": true},
                {"instruction_addr": "0x10400041c", "in_app": true}
            ]}
        }]},
        "debug_meta": {"images": [{
            "type": "macho", "debug_id": DSYM_ID, "arch": "arm64",
            "code_file": "/private/var/containers/Bundle/Application/7A8E/Crash.app/Crash",
            "image_addr": "0x104000000", "image_size": 16384
        }]}
    });
    let ingest = tempfile::TempDir::new().unwrap();
    store_event(
        ingest.path(),
        &event_id,
        &serde_json::to_vec(&event).unwrap(),
    )
    .await
    .unwrap();
    let metadata = EventMetadata {
        event_id: event_id.clone(),
        project_id,
        ingested_at: chrono::Utc::now(),
        remote_addr: None,
    };
    let rate_limit = rustrak::config::RateLimitConfig {
        max_events_per_minute: 1000,
        max_events_per_hour: 10000,
        max_events_per_project_per_minute: 500,
        max_events_per_project_per_hour: 5000,
    };
    crate::common::process_error_event(&db.pool, &metadata, ingest.path(), &rate_limit, provider)
        .await
        .expect("digest must succeed");

    let (data,): (String,) =
        sqlx::query_as("SELECT CAST(data AS TEXT) FROM events WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let data: Value = serde_json::from_str(&data).unwrap();
    let frames = &data["exception"]["values"][0]["stacktrace"]["frames"];
    let names: Vec<&str> = frames
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["function"].as_str().unwrap_or("?"))
        .collect();
    assert_eq!(names, ["compute", "divide", "compute", "divide"]);
    assert_eq!(frames[3]["context_line"], "    return a / b;");
    assert_eq!(data["debug_meta"]["images"][0]["debug_status"], "found");
}

/// Same mapping bytes under a second UUID: Sentry clones the stored DIF for
/// the new debug ID (`_clone_proguard_debug_file_for_reupload`).
#[actix_web::test]
async fn a_known_mapping_under_a_new_uuid_is_stored_for_it_too() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-reupload").await;
    let other = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let request = |uuid: &str, chunk: &str| json!({chunk: {"name": format!("/proguard/{uuid}.txt"), "chunks": [chunk]}});
    let chunk = store_chunk(&db, MAPPING).await;
    assemble(&db, &token, "difs-reupload", request(MAPPING_UUID, &chunk)).await;
    run_worker(&db).await;

    let (_, done) = assemble(&db, &token, "difs-reupload", request(other, &chunk)).await;

    assert_eq!(done[&chunk]["state"], "ok", "{done}");
    assert_eq!(
        file_types(&db, project_id).await,
        [
            (MAPPING_UUID.to_string(), "proguard".to_string()),
            (MAPPING_UUID.to_string(), "proguard_cache".to_string()),
            (other.to_string(), "proguard".to_string()),
            (other.to_string(), "proguard_cache".to_string()),
        ]
    );
}

/// Sentry keeps an assemble state for 10 minutes (`set_assemble_status`,
/// `ex=600`). After that a failed file is unknown, so sentry-cli uploads
/// its chunks again and the job is re-queued.
#[actix_web::test]
async fn an_old_failure_asks_for_the_chunks_again() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    project(&db, "difs-old-failure").await;
    let chunk = store_chunk(&db, b"not a mapping at all").await;
    let body = json!({chunk.clone(): {
        "name": format!("/proguard/{MAPPING_UUID}.txt"),
        "chunks": [chunk.clone()]
    }});
    assemble(&db, &token, "difs-old-failure", body.clone()).await;
    sqlx::query("UPDATE assembly_jobs SET max_retries = 1")
        .execute(&db.pool)
        .await
        .unwrap();
    run_worker(&db).await;
    sqlx::query("UPDATE assembly_jobs SET updated_at = $1")
        .bind(chrono::Utc::now() - chrono::Duration::minutes(11))
        .execute(&db.pool)
        .await
        .unwrap();

    let (_, polled) = assemble(&db, &token, "difs-old-failure", body).await;

    assert_eq!(polled[&chunk]["state"], "not_found", "{polled}");
    assert_eq!(polled[&chunk]["missingChunks"], json!([chunk]));
}

/// Each mapping fits the limit, together they do not: the limit is for
/// everything the zip expands to.
#[actix_web::test]
async fn legacy_upload_limits_the_whole_expanded_zip() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    project(&db, "difs-legacy-bomb").await;
    let mut config = create_test_config();
    config.max_chunk_size_bytes = 1024; // 64 KiB for the upload
    let mapping = vec![b' '; 40 * 1024];
    let zip = zip_of(&[
        (format!("proguard/{MAPPING_UUID}.txt"), &mapping),
        (format!("proguard/{BUNDLE_ID}.txt"), &mapping),
    ]);

    let resp = legacy_upload(&db, &token, "difs-legacy-bomb", &config, &zip).await;

    assert_eq!(resp.status(), 400);
}

/// A manifest past the size limit is not read, and the request's debug ID
/// does not stand in for it.
#[actix_web::test]
async fn a_source_bundle_with_an_oversized_manifest_fails() {
    let db = TestDb::new().await;
    let token = create_test_token(&db.pool).await;
    let project_id = project(&db, "difs-bundle-bomb").await;
    let mut manifest = vec![b' '; rustrak::services::source_bundle::MAX_BUNDLE_ENTRY_SIZE as usize];
    manifest.extend(format!(r#"{{"debug_id":"{BUNDLE_ID}","files":{{}}}}"#).as_bytes());
    // symbolic's header, then a zip whose offsets count it.
    let mut cursor = std::io::Cursor::new(b"SYSB\x02\0\0\0".to_vec());
    cursor.set_position(8);
    let mut zip = zip::ZipWriter::new(cursor);
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&manifest).unwrap();
    let bundle = zip.finish().unwrap().into_inner();
    let chunk = store_chunk(&db, &bundle).await;
    let body = json!({chunk.clone(): {
        "name": "Greeter.zip",
        "debug_id": BUNDLE_ID,
        "chunks": [chunk.clone()]
    }});

    assemble(&db, &token, "difs-bundle-bomb", body.clone()).await;
    sqlx::query("UPDATE assembly_jobs SET max_retries = 1")
        .execute(&db.pool)
        .await
        .unwrap();
    run_worker(&db).await;
    let (_, done) = assemble(&db, &token, "difs-bundle-bomb", body).await;

    assert_eq!(done[&chunk]["state"], "error", "{done}");
    assert_eq!(file_types(&db, project_id).await, []);
}
