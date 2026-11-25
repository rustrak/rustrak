//! A plugin's data in Rustrak's Storage page: its size, and its history (the
//! tables with `created_at`) cleared with the retention the admin sets, by the
//! same cleanup as Rustrak's own.

use actix_web::http::StatusCode;
use actix_web::test;
use rustrak::plugins::host::Source;
use serde_json::{json, Value};

use crate::common::{self, plugins::hello_rtp, TestDb};

/// Hits written straight into hello's file: two of 40 days ago, one of today,
/// and a day already counted (no created_at).
fn seed(path: &std::path::Path) {
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch(
        "INSERT INTO hits (project_id, path, created_at) VALUES
           (1, '/old', unixepoch() - 40 * 86400),
           (1, '/older', unixepoch() - 41 * 86400),
           (1, '/new', unixepoch());
         INSERT INTO daily_hits (project_id, day, hits) VALUES (1, '2020-01-01', 7);",
    )
    .unwrap();
}

#[actix_web::test]
async fn an_admin_sets_a_plugins_retention_and_the_cleanup_clears_its_history() {
    let db = TestDb::new().await;
    let config = common::config();
    let state = common::state(&db.pool, config.clone()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    seed(&config.plugins_dir.join("hello").join("data.sqlite"));
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;

    // Kept forever until someone says otherwise: nothing has expired.
    macro_rules! report {
        ($app:expr) => {{
            let req = test::TestRequest::get()
                .uri("/api/storage/plugins")
                .insert_header(crate::common::bearer(&owner))
                .to_request();
            let resp = test::call_service($app, req).await;
            assert_eq!(resp.status(), StatusCode::OK);
            test::read_body_json::<Value, _>(resp).await
        }};
    }
    let before = report!(&app);
    assert_eq!(before[0]["id"], "hello");
    assert_eq!(before[0]["retention_days"], Value::Null);
    assert!(before[0]["bytes"].as_u64().unwrap() > 0);
    assert_eq!(before[0]["quota_bytes"], 512 * 1024 * 1024);
    assert_eq!(
        before[0]["history"],
        json!([{ "table": "hits", "rows": 3, "expired": 0 }])
    );

    let req = test::TestRequest::put()
        .uri("/api/storage/retention")
        .insert_header(crate::common::bearer(&owner))
        .set_json(json!({ "plugins": { "hello": 30 } }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), StatusCode::OK);
    let set = report!(&app);
    assert_eq!(set[0]["retention_days"], 30);
    assert_eq!(set[0]["history"][0]["expired"], 2);

    // A window of no days is refused, as for Rustrak's own data.
    let req = test::TestRequest::put()
        .uri("/api/storage/retention")
        .insert_header(crate::common::bearer(&owner))
        .set_json(json!({ "plugins": { "hello": 0 } }))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_REQUEST
    );

    let req = test::TestRequest::post()
        .uri("/api/storage/retention/run")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::ACCEPTED
    );
    let last_run = common::eventually(|| async {
        let req = test::TestRequest::get()
            .uri("/api/storage/retention")
            .insert_header(crate::common::bearer(&owner))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        let last = body["last_run"].clone();
        (last["status"] != "running" && !last.is_null()).then_some(last)
    })
    .await;
    assert_eq!(last_run["status"], "ok", "{last_run}");
    assert_eq!(last_run["removed"]["plugin_rows"], 2);

    let after = report!(&app);
    assert_eq!(
        after[0]["history"],
        json!([{ "table": "hits", "rows": 1, "expired": 0 }])
    );
    let file = rusqlite::Connection::open(config.plugins_dir.join("hello/data.sqlite")).unwrap();
    let daily: i64 = file
        .query_row("SELECT count(*) FROM daily_hits", [], |r| r.get(0))
        .unwrap();
    assert_eq!(daily, 1, "a table without created_at is never touched");
}

#[actix_web::test]
async fn deleting_data_now_can_include_a_plugins_history() {
    let db = TestDb::new().await;
    let config = common::config();
    let state = common::state(&db.pool, config.clone()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    seed(&config.plugins_dir.join("hello").join("data.sqlite"));
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;
    let request = json!({
        "older_than_days": 30,
        "include_events": false, "include_transactions": false, "include_logs": false,
        "plugins": ["hello"],
    });

    // Counted first, as the dialog shows it before deleting.
    let req = test::TestRequest::post()
        .uri("/api/storage/cleanup/preview")
        .insert_header(crate::common::bearer(&owner))
        .set_json(&request)
        .to_request();
    let preview: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(preview["plugin_rows"], 2, "{preview}");

    // The retention dialog's preview counts a plugin's window too.
    let req = test::TestRequest::post()
        .uri("/api/storage/retention/preview")
        .insert_header(crate::common::bearer(&owner))
        .set_json(json!({ "plugins": { "hello": 30 } }))
        .to_request();
    let retention: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(retention["plugin_rows"], 2, "{retention}");

    let req = test::TestRequest::post()
        .uri("/api/storage/cleanup")
        .insert_header(crate::common::bearer(&owner))
        .set_json(&request)
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::ACCEPTED
    );
    let status = common::eventually(|| async {
        let req = test::TestRequest::get()
            .uri("/api/storage/cleanup/status")
            .insert_header(crate::common::bearer(&owner))
            .to_request();
        let body: Value = test::call_and_read_body_json(&app, req).await;
        (body["state"] != "running").then_some(body)
    })
    .await;
    assert_eq!(status["state"], "completed", "{status}");
    assert_eq!(status["removed"]["plugin_rows"], 2);
    let file = rusqlite::Connection::open(config.plugins_dir.join("hello/data.sqlite")).unwrap();
    let left: i64 = file
        .query_row("SELECT count(*) FROM hits", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 1);
}
