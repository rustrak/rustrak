//! A plugin installed on a real server: its routes under the project, public
//! ones by the DSN key, user ones by session or token, activation per
//! project, and surviving a restart.

use actix_web::http::StatusCode;
use actix_web::test;
use rustrak::plugins::host::Source;

use crate::common::{self, plugins::hello_rtp, TestDb};

async fn body(resp: actix_web::dev::ServiceResponse) -> String {
    String::from_utf8(test::read_body(resp).await.to_vec()).unwrap()
}

#[actix_web::test]
async fn a_public_route_takes_the_dsn_key_and_a_user_route_a_token() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    let (project_id, key) = common::create_ingest_project(&db.pool, "web").await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .expect("installs");
    state
        .plugins
        .set_projects(&db.pool, "hello", true, None)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let token = common::create_test_token(&db.pool).await;

    let hit = |key: &str| {
        test::TestRequest::post()
            .uri(&format!(
                "/api/projects/{project_id}/plugins/hello/hit?sentry_key={key}"
            ))
            .set_payload("/home")
            .to_request()
    };
    assert_eq!(
        test::call_service(&app, hit(&key)).await.status(),
        StatusCode::NO_CONTENT
    );
    let wrong = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        test::call_service(&app, hit(&wrong)).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let count = |auth: Option<&str>| {
        let mut req = test::TestRequest::get()
            .uri(&format!("/api/projects/{project_id}/plugins/hello/count"));
        if let Some(t) = auth {
            req = req.insert_header(("Authorization", format!("Bearer {t}")));
        }
        req.to_request()
    };
    assert_eq!(
        test::call_service(&app, count(None)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let resp = test::call_service(&app, count(Some(&token))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body(resp).await, "1");
}

#[actix_web::test]
async fn a_plugin_answers_only_in_projects_it_is_active_in() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    let (project_id, key) = common::create_ingest_project(&db.pool, "web").await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;

    let hit = test::TestRequest::post()
        .uri(&format!(
            "/api/projects/{project_id}/plugins/hello/hit?sentry_key={key}"
        ))
        .to_request();
    assert_eq!(
        test::call_service(&app, hit).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn unknown_plugins_and_routes_are_404() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    let (project_id, key) = common::create_ingest_project(&db.pool, "web").await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    state
        .plugins
        .set_projects(&db.pool, "hello", true, None)
        .await
        .unwrap();
    let app = common::app_with(&state).await;

    for uri in [
        format!("/api/projects/{project_id}/plugins/nope/hit?sentry_key={key}"),
        format!("/api/projects/{project_id}/plugins/hello/missing?sentry_key={key}"),
    ] {
        let resp = test::call_service(&app, test::TestRequest::post().uri(&uri).to_request()).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[actix_web::test]
async fn a_runaway_plugin_times_out_without_taking_the_server_down() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let token = common::create_test_token(&db.pool).await;

    let spin = test::TestRequest::get()
        .uri("/api/plugins/hello/spin")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, spin).await.status(),
        StatusCode::GATEWAY_TIMEOUT
    );

    let health = test::TestRequest::get().uri("/health").to_request();
    assert!(test::call_service(&app, health).await.status().is_success());
}

#[actix_web::test]
async fn a_plugin_sleeps_within_its_budget_and_no_longer() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let token = common::create_test_token(&db.pool).await;

    let sleep = test::TestRequest::get()
        .uri("/api/plugins/hello/sleep")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let answer = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        test::call_service(&app, sleep),
    )
    .await
    .expect("the sleep ends with the call's budget");
    assert_eq!(answer.status(), StatusCode::GATEWAY_TIMEOUT);

    let nap = test::TestRequest::get()
        .uri("/api/plugins/hello/nap")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, nap).await.status(),
        StatusCode::NO_CONTENT
    );
}

#[actix_web::test]
async fn installed_plugins_are_loaded_again_after_a_restart() {
    let db = TestDb::new().await;
    let config = common::config();
    let (project_id, key) = common::create_ingest_project(&db.pool, "web").await;
    let token = common::create_test_token(&db.pool).await;
    {
        let state = common::state(&db.pool, config.clone()).await;
        state
            .plugins
            .install(&db.pool, hello_rtp(), Source::Upload)
            .await
            .unwrap();
        state
            .plugins
            .set_projects(&db.pool, "hello", true, None)
            .await
            .unwrap();
        let app = common::app_with(&state).await;
        let hit = test::TestRequest::post()
            .uri(&format!(
                "/api/projects/{project_id}/plugins/hello/hit?sentry_key={key}"
            ))
            .to_request();
        assert_eq!(
            test::call_service(&app, hit).await.status(),
            StatusCode::NO_CONTENT
        );
    }

    // A new process: same database, same plugin directory.
    let state = common::state(&db.pool, config).await;
    state.plugins.boot(&db.pool).await.expect("boots");
    let app = common::app_with(&state).await;
    let count = test::TestRequest::get()
        .uri(&format!("/api/projects/{project_id}/plugins/hello/count"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, count).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body(resp).await, "1");
}

#[actix_web::test]
async fn a_package_for_another_rustrak_version_is_refused() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    let rtp = common::plugins::hello_rtp_with(|m| {
        m.replace(r#"rustrak = ">=0.33.0""#, r#"rustrak = ">=99.0.0""#)
    });
    let Err(err) = state.plugins.install(&db.pool, rtp, Source::Upload).await else {
        panic!("should be refused");
    };
    assert!(err.to_string().contains("99.0.0"), "{err}");
    assert!(state.plugins.get("hello").is_none());
}

#[actix_web::test]
async fn what_the_code_declares_cannot_be_written_in_the_manifest() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    // A route made public, or the database hidden, by editing the packed
    // TOML: the routes and the database are read from the code, so a
    // manifest that names them is refused.
    for (edit, why) in [
        (
            "\n[[routes]]\nmethod = \"GET\"\npath = \"/count\"\nauth = \"public\"\nscope = \"project\"\n",
            "a route made public",
        ),
        ("\n[capabilities]\nsql = false\n", "the database hidden"),
    ] {
        let rtp = common::plugins::hello_rtp_with(|m| format!("{m}{edit}"));
        let Err(err) = state.plugins.install(&db.pool, rtp, Source::Upload).await else {
            panic!("{why} should be refused");
        };
        assert!(err.to_string().contains("unknown field"), "{why}: {err}");
        assert!(state.plugins.get("hello").is_none(), "{why}");
    }
}

/// Each call is counted for the telemetry under the plugin's id when it came
/// from the registry, and as `upload` when it did not: an uploaded plugin's
/// id can name a company's own code.
#[actix_web::test]
async fn calls_are_counted_by_registry_id_or_as_upload() {
    let counters = rustrak::telemetry::Counters::global();
    let calls = |key: &str| {
        counters
            .snapshot()
            .plugins
            .calls
            .get(key)
            .copied()
            .unwrap_or(0)
    };
    for (source, key) in [(Source::Upload, "upload"), (Source::Registry, "hello")] {
        let db = TestDb::new().await;
        let state = common::state(&db.pool, common::config()).await;
        let (project_id, key_) = common::create_ingest_project(&db.pool, "web").await;
        state
            .plugins
            .install(&db.pool, hello_rtp(), source)
            .await
            .unwrap();
        state
            .plugins
            .set_projects(&db.pool, "hello", true, None)
            .await
            .unwrap();
        let app = common::app_with(&state).await;
        let before = calls(key);
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!(
                    "/api/projects/{project_id}/plugins/hello/hit?sentry_key={key_}"
                ))
                .set_payload("/home")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(calls(key) > before, "{key} was not counted");
    }
}
