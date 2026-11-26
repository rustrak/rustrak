//! A plugin's settings over HTTP: the form Rustrak draws writes them, checked
//! against the plugin's schema; the plugin reads them typed, secrets
//! included; the browser reads them without the secrets.

use actix_web::http::StatusCode;
use actix_web::test;
use rustrak::models::UserRole;
use rustrak::plugins::host::Source;
use serde_json::json;

use crate::common::{self, plugins::hello_rtp, TestDb};

#[actix_web::test]
async fn an_owner_configures_a_plugin_and_the_plugin_reads_it() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;
    let (web, _) = common::create_ingest_project(&db.pool, "web").await;
    let (api, _) = common::create_ingest_project(&db.pool, "api").await;
    state
        .plugins
        .set_projects(&db.pool, "hello", true, None)
        .await
        .unwrap();

    // Nothing set: the defaults, and the secret is only "not set".
    let (status, view) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri("/api/settings/plugins/hello/settings")
            .insert_header(crate::common::bearer(&owner))
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["resolved"]["greeting"], "Hello");
    assert_eq!(view["secrets"]["token"], false);
    assert!(view["resolved"].get("token").is_none());

    // A value the schema refuses names its field, and nothing is written.
    let (status, body) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri("/api/settings/plugins/hello/settings")
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({ "greeting": "Hola", "maxPathLength": 0 }))
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["fields"][0]["field"], "maxPathLength");

    let (status, view) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri("/api/settings/plugins/hello/settings")
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({ "greeting": "Hola", "token": "s3cret", "maxPathLength": 300 }))
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["values"]["greeting"], "Hola");
    assert_eq!(view["secrets"]["token"], true);
    assert!(
        !view.to_string().contains("s3cret"),
        "a secret never reaches a browser"
    );

    // One project overrides a project setting; the other keeps the instance's.
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/projects/{web}/plugin-settings/hello"))
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({ "maxPathLength": 50 }))
    );
    assert_eq!(status, StatusCode::OK);
    let (status, body) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/projects/{web}/plugin-settings/hello"))
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({ "greeting": "x" }))
    );
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // What the plugin's own code reads, through Settings::get(ctx).
    for (project, longest) in [(web, 50), (api, 300)] {
        let (status, seen) = crate::call_json!(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/projects/{project}/plugins/hello/config"))
                .insert_header(crate::common::bearer(&owner))
        );
        assert_eq!(status, StatusCode::OK, "{seen}");
        assert_eq!(
            seen,
            json!({ "greeting": "Hola", "maxPathLength": longest, "hasToken": true })
        );
    }
}

#[actix_web::test]
async fn people_read_settings_without_secrets_and_only_those_allowed_write_them() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let (web, _) = common::create_ingest_project(&db.pool, "web").await;
    let viewer = common::seed_user(&db.pool, "viewer@example.com", UserRole::Member).await;
    common::grant_project_role(&db.pool, web, viewer.id, "Viewer")
        .await
        .unwrap();
    let viewer = common::token_for(&db.pool, viewer.id).await;

    let (status, view) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{web}/plugin-settings/hello"))
            .insert_header(crate::common::bearer(&viewer))
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["resolved"]["maxPathLength"], 200);
    assert!(view["resolved"].get("token").is_none());

    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/projects/{web}/plugin-settings/hello"))
            .insert_header(crate::common::bearer(&viewer))
            .set_json(json!({ "maxPathLength": 50 }))
    );
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri("/api/settings/plugins/hello/settings")
            .insert_header(crate::common::bearer(&viewer))
            .set_json(json!({ "greeting": "x" }))
    );
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn settings_survive_a_restart() {
    let db = TestDb::new().await;
    let config = common::config();
    let state = common::state(&db.pool, config.clone()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let owner = common::create_test_token(&db.pool).await;
    let app = common::app_with(&state).await;
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri("/api/settings/plugins/hello/settings")
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({ "greeting": "Hola" }))
    );
    assert_eq!(status, StatusCode::OK);

    let restarted = rustrak::plugins::host::PluginHost::new(config.plugins_dir.clone());
    restarted.boot(&db.pool).await.unwrap();
    assert_eq!(
        restarted.get("hello").unwrap().settings(None)["greeting"],
        "Hola"
    );
}

#[actix_web::test]
async fn a_plugin_missing_a_required_setting_answers_503_until_it_is_set() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    let rtp = common::plugins::hello_rtp_with(|m| {
        // The secret's `required = false`, the first one after its type.
        let at = m.find("type = \"secret\"").unwrap();
        let (head, tail) = m.split_at(at);
        format!(
            "{head}{}",
            tail.replacen("required = false", "required = true", 1)
        )
    });
    state
        .plugins
        .install(&db.pool, rtp, Source::Upload)
        .await
        .unwrap();
    assert!(state
        .plugins
        .get("hello")
        .unwrap()
        .manifest
        .settings
        .iter()
        .any(|s| s.required));
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;
    let (web, _) = common::create_ingest_project(&db.pool, "web").await;
    state
        .plugins
        .set_projects(&db.pool, "hello", true, None)
        .await
        .unwrap();
    let config = || {
        test::TestRequest::get()
            .uri(&format!("/api/projects/{web}/plugins/hello/config"))
            .insert_header(crate::common::bearer(&owner))
    };

    let (status, body) = crate::call_json!(&app, config());
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(body.to_string().contains("token"), "{body}");

    state
        .plugins
        .set_settings(
            &db.pool,
            "hello",
            None,
            json!({ "token": "t" }).as_object().unwrap().clone(),
        )
        .await
        .unwrap();
    let (status, body) = crate::call_json!(&app, config());
    assert_eq!(status, StatusCode::OK, "{body}");
}
