//! The source map assembly worker as the server runs it: woken by an
//! assemble request, and picking up at startup what a previous run left.

use crate::common::{create_test_token, eventually, TestDb};
use crate::integration::sourcemaps_api_test::sha1_hex;
use actix_web::test;
use rustrak::app::AppState;
use rustrak::models::CreateProject;
use rustrak::services::sourcemap::{list_files, rewrite_frames, store_chunks};
use rustrak::services::ProjectService;
use serde_json::{json, Value};
use std::io::Write as _;
use zip::write::SimpleFileOptions;

const SOURCE_MAP: &str = r#"{"version":3,"sources":["src/app.ts"],"sourcesContent":["a\nb\nc"],"mappings":"AAAA;AACA;AACA"}"#;

fn bundle(entries: &[(&str, &str)], symlink: Option<&str>) -> Vec<u8> {
    let files: serde_json::Map<String, Value> = entries
        .iter()
        .map(|(path, debug_id)| {
            (
                format!("~/{path}"),
                json!({"type": "source_map", "headers": {"debug-id": debug_id}}),
            )
        })
        .collect();
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut buf);
    let opts = SimpleFileOptions::default();
    for (path, _) in entries {
        if Some(*path) == symlink {
            zip.add_symlink(format!("~/{path}"), "/etc/passwd", opts)
                .unwrap();
        } else {
            zip.start_file(format!("~/{path}"), opts).unwrap();
            zip.write_all(SOURCE_MAP.as_bytes()).unwrap();
        }
    }
    zip.start_file("manifest.json", opts).unwrap();
    zip.write_all(json!({ "files": files }).to_string().as_bytes())
        .unwrap();
    zip.finish().unwrap();
    buf.into_inner()
}

struct Server {
    db: TestDb,
    state: AppState,
    token: String,
}

impl Server {
    async fn new() -> Self {
        let db = TestDb::new().await;
        let state = crate::common::state(&db.pool, crate::common::config()).await;
        let token = create_test_token(&db.pool).await;
        Self { db, state, token }
    }

    async fn project(&self, slug: &str) -> i32 {
        ProjectService::create(
            &self.db.pool,
            CreateProject {
                name: slug.into(),
                slug: Some(slug.into()),
                platform: None,
            },
        )
        .await
        .unwrap()
        .id
    }

    /// The assemble answer for `bundle`, its chunk uploaded first.
    async fn assemble(&self, slug: &str, bundle: &[u8]) -> Value {
        let checksum = sha1_hex(bundle);
        store_chunks(&self.db.pool, vec![(checksum, bundle.to_vec())], 100 << 20)
            .await
            .unwrap();
        self.poll(slug, bundle).await
    }

    async fn poll(&self, slug: &str, bundle: &[u8]) -> Value {
        let checksum = sha1_hex(bundle);
        let app = crate::common::app_with(&self.state).await;
        let req = test::TestRequest::post()
            .uri("/api/0/organizations/my-org/artifactbundle/assemble/")
            .insert_header(("Authorization", format!("Bearer {}", self.token)))
            .set_json(json!({"checksum": checksum, "chunks": [checksum], "projects": [slug]}))
            .to_request();
        test::call_and_read_body_json(&app, req).await
    }

    /// The assemble answer once the job submitted for `bundle` is done.
    async fn assembled(&self, slug: &str, bundle: &[u8]) -> Value {
        self.assemble(slug, bundle).await;
        eventually(|| async {
            let answer = self.poll(slug, bundle).await;
            (!matches!(answer["state"].as_str(), Some("created" | "assembling"))).then_some(answer)
        })
        .await
    }
}

#[actix_web::test]
async fn an_assemble_request_wakes_the_worker() {
    let server = Server::new().await;
    server.project("woken").await;
    server.state.spawn_assembly_worker();

    for debug_id in [
        "11111111-1111-1111-1111-111111111111",
        "22222222-2222-2222-2222-222222222222",
    ] {
        let bundle = bundle(&[("app.js.map", debug_id)], None);
        assert_eq!(server.assemble("woken", &bundle).await["state"], "created");
        assert_eq!(server.assembled("woken", &bundle).await["state"], "ok");
    }
}

#[actix_web::test]
async fn a_job_a_previous_run_left_unfinished_is_assembled_at_startup() {
    let server = Server::new().await;
    let project_id = server.project("restart").await;
    let bundle = bundle(
        &[("app.js.map", "33333333-3333-3333-3333-333333333333")],
        None,
    );
    // Queued with no worker running, then marked as a crashed run leaves it.
    server.assemble("restart", &bundle).await;
    sqlx::query("UPDATE assembly_jobs SET state = 'assembling' WHERE project_id = $1")
        .bind(project_id)
        .execute(&server.db.pool)
        .await
        .unwrap();

    server.state.spawn_assembly_worker();

    let state = eventually(|| async {
        let state: String =
            sqlx::query_scalar("SELECT state FROM assembly_jobs WHERE project_id = $1")
                .bind(project_id)
                .fetch_one(&server.db.pool)
                .await
                .unwrap();
        (state != "assembling").then_some(state)
    })
    .await;
    assert_eq!(state, "ok");
}

#[actix_web::test]
async fn an_archive_entry_outside_the_bundle_fails_the_job() {
    let server = Server::new().await;
    server.project("traversal").await;
    server.state.spawn_assembly_worker();
    let bundle = bundle(
        &[("../../evil.map", "44444444-4444-4444-4444-444444444444")],
        None,
    );

    let answer = server.assembled("traversal", &bundle).await;
    assert_eq!(answer["state"], "error");
    assert!(answer["detail"]
        .as_str()
        .unwrap()
        .contains("path traversal"));
}

#[actix_web::test]
async fn a_symlink_in_the_archive_is_not_extracted() {
    let server = Server::new().await;
    let project_id = server.project("symlink").await;
    server.state.spawn_assembly_worker();
    let kept = "55555555-5555-5555-5555-555555555555";
    let bundle = bundle(
        &[
            ("app.js.map", kept),
            ("link.map", "66666666-6666-6666-6666-666666666666"),
        ],
        Some("link.map"),
    );

    assert_eq!(server.assembled("symlink", &bundle).await["state"], "ok");
    let stored: Vec<String> = list_files(&server.db.pool, project_id)
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.debug_id)
        .collect();
    assert_eq!(stored, [kept]);
}

#[actix_web::test]
async fn an_assembled_map_rewrites_only_its_own_projects_frames() {
    let server = Server::new().await;
    let owner = server.project("owner").await;
    let other = server.project("other").await;
    server.state.spawn_assembly_worker();
    let debug_id = "77777777-7777-7777-7777-777777777777";
    let bundle = bundle(&[("app.js.map", debug_id)], None);
    assert_eq!(server.assembled("owner", &bundle).await["state"], "ok");

    for (project_id, filename) in [(owner, "src/app.ts"), (other, "app.js")] {
        let mut event = json!({
            "debug_meta": {"images": [{"code_file": "app.js", "debug_id": debug_id}]},
            "exception": {"values": [{"stacktrace": {"frames": [
                {"filename": "app.js", "lineno": 2, "colno": 0}
            ]}}]}
        });
        rewrite_frames(
            server.state.sourcemap_provider.get_ref().as_ref(),
            project_id,
            &mut event,
        )
        .await
        .unwrap();
        assert_eq!(
            event["exception"]["values"][0]["stacktrace"]["frames"][0]["filename"],
            filename
        );
    }
}
