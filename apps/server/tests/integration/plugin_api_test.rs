//! Managing plugins over HTTP: installing a package, choosing where it runs,
//! listing what the dashboard should load, and serving its UI bundle.

use actix_web::http::StatusCode;
use actix_web::test;
use rustrak::plugins::host::Source;
use serde_json::Value;

use crate::common::{
    self,
    plugins::{hello_rtp, hello_rtp_with},
    TestDb,
};

async fn json(resp: actix_web::dev::ServiceResponse) -> Value {
    serde_json::from_slice(&test::read_body(resp).await).unwrap()
}

#[actix_web::test]
async fn an_owner_installs_a_package_and_everyone_sees_it_listed() {
    let db = TestDb::new().await;
    let app = common::app(&db.pool).await;
    let owner = common::create_test_token(&db.pool).await;

    let install = test::TestRequest::post()
        .uri("/api/settings/plugins")
        .insert_header(crate::common::bearer(&owner))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(hello_rtp())
        .to_request();
    let resp = test::call_service(&app, install).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let installed = json(resp).await;
    assert_eq!(installed["id"], "hello");
    assert_eq!(installed["version"], "0.1.0");

    let list = test::TestRequest::get()
        .uri("/api/plugins")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    let plugins = json(test::call_service(&app, list).await).await;
    let hello = &plugins[0];
    assert_eq!(hello["id"], "hello");
    assert_eq!(hello["name"], "Hello");
    assert_eq!(hello["allProjects"], false);
    assert_eq!(hello["icon"], "star");
    assert_eq!(hello["author"], "Rustrak");
    assert!(hello["readme"]["en"]
        .as_str()
        .unwrap()
        .starts_with("# Hello"));
    assert!(
        hello["readme"]["es"]
            .as_str()
            .unwrap()
            .starts_with("# Hello"),
        "its Spanish README"
    );
    assert_eq!(hello["database"], true);
    assert!(hello["permissions"][0]["resource"].is_string());
    let ui = &hello["ui"];
    let entry = ui["entry"].as_str().unwrap();
    assert!(entry.starts_with("/api/plugin-assets/hello/") && entry.ends_with("/index.js"));
    assert_eq!(ui["styles"], entry.replace("index.js", "index.css"));
    let page = |path: &str, scope: &str| {
        ui["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["path"] == path && p["scope"] == scope)
            .cloned()
            .unwrap_or_else(|| panic!("no {scope} page {path:?}"))
    };
    assert_eq!(
        page("", "project")["sidebar"],
        serde_json::json!({ "label": "nav.hits", "icon": "performance", "rank": 1 })
    );
    assert!(page("hits/:hitId", "project")["sidebar"].is_null());
    assert_eq!(page("", "instance")["sidebar"]["label"], "nav.hello");
    assert_eq!(ui["widgets"][0]["zone"], "issue.details.after");
    // What Rustrak draws its settings form from, and labels it with.
    assert_eq!(hello["settings"][0]["key"], "greeting");
    assert_eq!(hello["settings"][2]["scope"], "project");
    assert_eq!(
        hello["messages"]["en"]["settings"]["greeting"]["label"],
        "Greeting"
    );
    assert_eq!(
        hello["messages"]["es"]["settings"]["greeting"]["label"],
        "Saludo"
    );
}

#[actix_web::test]
async fn only_an_owner_installs_or_configures() {
    let db = TestDb::new().await;
    let app = common::app(&db.pool).await;
    let user = common::seed_user(
        &db.pool,
        "member@example.com",
        rustrak::models::UserRole::Member,
    )
    .await;
    let token = common::token_for(&db.pool, user.id).await;

    let install = test::TestRequest::post()
        .uri("/api/settings/plugins")
        .insert_header(crate::common::bearer(&token))
        .set_payload(hello_rtp())
        .to_request();
    assert_eq!(
        test::call_service(&app, install).await.status(),
        StatusCode::FORBIDDEN
    );

    let configure = test::TestRequest::put()
        .uri("/api/settings/plugins/hello")
        .insert_header(crate::common::bearer(&token))
        .set_json(serde_json::json!({ "allProjects": true }))
        .to_request();
    assert_eq!(
        test::call_service(&app, configure).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[actix_web::test]
async fn a_broken_package_is_a_400_that_says_why() {
    let db = TestDb::new().await;
    let app = common::app(&db.pool).await;
    let owner = common::create_test_token(&db.pool).await;
    let install = test::TestRequest::post()
        .uri("/api/settings/plugins")
        .insert_header(crate::common::bearer(&owner))
        .set_payload("not a zip")
        .to_request();
    let resp = test::call_service(&app, install).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(json(resp)
        .await
        .to_string()
        .contains("not a plugin package"));
}

#[actix_web::test]
async fn activating_everywhere_turns_its_routes_on() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;
    let (project_id, key) = common::create_ingest_project(&db.pool, "web").await;
    let hit = || {
        test::TestRequest::post()
            .uri(&format!(
                "/api/projects/{project_id}/plugins/hello/hit?sentry_key={key}"
            ))
            .to_request()
    };
    assert_eq!(
        test::call_service(&app, hit()).await.status(),
        StatusCode::NOT_FOUND
    );

    let configure = test::TestRequest::put()
        .uri("/api/settings/plugins/hello")
        .insert_header(crate::common::bearer(&owner))
        .set_json(serde_json::json!({ "allProjects": true }))
        .to_request();
    let resp = test::call_service(&app, configure).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(json(resp).await["allProjects"], true);
    assert_eq!(
        test::call_service(&app, hit()).await.status(),
        StatusCode::NO_CONTENT
    );
}

/// The bundle's address of the installed `hello`, as the dashboard gets it.
async fn entry_of(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
) -> String {
    let list = test::TestRequest::get()
        .uri("/api/plugins")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let plugins = json(test::call_service(app, list).await).await;
    plugins[0]["ui"]["entry"].as_str().unwrap().to_string()
}

#[actix_web::test]
async fn the_ui_bundle_is_served_from_the_installed_package() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let token = common::create_test_token(&db.pool).await;
    let entry = entry_of(&app, &token).await;

    let resp = test::call_service(&app, test::TestRequest::get().uri(&entry).to_request()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/javascript; charset=utf-8"
    );
    let body = test::read_body(resp).await;
    assert!(String::from_utf8_lossy(&body).contains("__rustrakPlugins"));

    for uri in [
        "/api/plugin-assets/hello/0.1.0/index.js".to_string(),
        entry.replace("index.js", "../server.wasm"),
        entry.replace("index.js", "missing.js"),
        entry.replace("/hello/", "/nope/"),
    ] {
        let resp = test::call_service(&app, test::TestRequest::get().uri(&uri).to_request()).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

/// The bundle is cached for good, so its address has to change with its
/// content, not only with the version: a package rebuilt under the same
/// version would otherwise keep its old pages in every browser.
/// A plugin's logo: shown wherever the plugin is, and never a page of its
/// own a script in it could run in.
#[actix_web::test]
async fn a_plugins_logo_is_served_as_an_inert_image() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let token = common::create_test_token(&db.pool).await;
    let list = test::TestRequest::get()
        .uri("/api/plugins")
        .insert_header(crate::common::bearer(&token))
        .to_request();
    let plugins = json(test::call_service(&app, list).await).await;
    let logo = plugins[0]["logo"].as_str().expect("a logo").to_string();
    assert!(logo.starts_with("/api/plugin-assets/hello/") && logo.ends_with("/logo.svg"));

    let resp = test::call_service(&app, test::TestRequest::get().uri(&logo).to_request()).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/svg+xml");
    let csp = resp
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        csp.contains("sandbox") && csp.contains("default-src 'none'"),
        "{csp}"
    );
    assert_eq!(
        resp.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
}

/// What a plugin hands out to anyone, like a tracker script a website
/// loads: at a stable address, without a session, from any origin.
#[actix_web::test]
async fn a_plugins_public_files_are_served_to_anyone() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/public/hello/hello.js")
            .insert_header(("Origin", "https://shop.example.com"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/javascript; charset=utf-8"
    );
    // Not immutable: the address stays when the plugin is updated.
    let cache = resp
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cache.starts_with("public, max-age="), "{cache}");
    assert!(!cache.contains("immutable"));
    // Any website may load it (the global CORS echoes its origin).
    assert_eq!(
        resp.headers().get("access-control-allow-origin").unwrap(),
        "https://shop.example.com"
    );
    let body = test::read_body(resp).await;
    assert!(String::from_utf8_lossy(&body).contains("hello tracker"));

    for uri in [
        "/public/hello/missing.js",
        "/public/hello/../server.wasm",
        "/public/nope/hello.js",
    ] {
        let resp = test::call_service(&app, test::TestRequest::get().uri(uri).to_request()).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[actix_web::test]
async fn a_reinstalled_bundle_gets_a_new_address() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let token = common::create_test_token(&db.pool).await;
    let before = entry_of(&app, &token).await;

    let rebuilt = hello_rtp_with(|m| m.replace("name = \"Hello\"", "name = \"Hello again\""));
    state
        .plugins
        .install(&db.pool, rebuilt, Source::Upload)
        .await
        .unwrap();
    let after = entry_of(&app, &token).await;

    assert_ne!(before, after);
    let old = test::call_service(&app, test::TestRequest::get().uri(&before).to_request()).await;
    assert_eq!(old.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn an_owner_picks_the_projects_a_plugin_runs_in() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;
    let (web, web_key) = common::create_ingest_project(&db.pool, "web").await;
    let (api, api_key) = common::create_ingest_project(&db.pool, "api").await;

    let configure = test::TestRequest::put()
        .uri("/api/settings/plugins/hello")
        .insert_header(crate::common::bearer(&owner))
        .set_json(serde_json::json!({ "allProjects": false, "projects": [web] }))
        .to_request();
    let resp = test::call_service(&app, configure).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(json(resp).await["projects"], serde_json::json!([web]));

    let hit = |project: i32, key: &str| {
        test::TestRequest::post()
            .uri(&format!(
                "/api/projects/{project}/plugins/hello/hit?sentry_key={key}"
            ))
            .to_request()
    };
    assert_eq!(
        test::call_service(&app, hit(web, &web_key)).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        test::call_service(&app, hit(api, &api_key)).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn uninstalling_removes_the_plugin_and_purging_its_data() {
    let db = TestDb::new().await;
    let config = common::config();
    let state = common::state(&db.pool, config.clone()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;
    let data = config.plugins_dir.join("hello").join("data.sqlite");
    assert!(data.exists());
    let greeting =
        serde_json::Map::from_iter([("greeting".to_string(), serde_json::json!("Hola"))]);
    state
        .plugins
        .set_settings(&db.pool, "hello", None, greeting)
        .await
        .unwrap();
    rustrak::services::RoleService::create(
        &db.pool,
        "Hit reader",
        "",
        vec![],
        vec![rustrak::models::PluginGrant {
            resource: "hello:hits".into(),
            action: rustrak::models::rbac::Action::Read,
        }],
    )
    .await
    .unwrap();
    let role_grants = || async {
        rustrak::services::RoleService::list(&db.pool)
            .await
            .unwrap()[0]
            .plugin_permissions
            .len()
    };

    let uninstall = |purge: bool| {
        test::TestRequest::delete()
            .uri(&format!("/api/settings/plugins/hello?purge={purge}"))
            .insert_header(crate::common::bearer(&owner))
            .to_request()
    };
    assert_eq!(
        test::call_service(&app, uninstall(false)).await.status(),
        StatusCode::NO_CONTENT
    );
    assert!(state.plugins.get("hello").is_none());
    assert!(data.exists(), "kept unless purged");
    let list = test::TestRequest::get()
        .uri("/api/plugins")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    assert_eq!(
        json(test::call_service(&app, list).await).await,
        serde_json::json!([])
    );

    // Its settings and the roles granting its permissions are its data too:
    // a reinstall finds them.
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    assert_eq!(
        state.plugins.get("hello").unwrap().settings(None)["greeting"],
        "Hola"
    );
    assert_eq!(role_grants().await, 1);

    assert_eq!(
        test::call_service(&app, uninstall(true)).await.status(),
        StatusCode::NO_CONTENT
    );
    assert!(!config.plugins_dir.join("hello").exists(), "purged");
    assert_eq!(role_grants().await, 0, "purged from roles too");
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    assert_eq!(
        state.plugins.get("hello").unwrap().settings(None)["greeting"],
        "Hello"
    );
    state
        .plugins
        .uninstall(&db.pool, "hello", true)
        .await
        .unwrap();
    assert_eq!(
        test::call_service(&app, uninstall(true)).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn a_package_refused_late_leaves_nothing_behind() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    let files: Vec<_> = crate::common::plugins::files_of(&hello_rtp())
        .into_iter()
        .map(|(path, bytes)| match path.as_str() {
            "messages/en.json" => (path, b"{ not json".to_vec()),
            _ => (path, bytes),
        })
        .collect();
    let refused = state
        .plugins
        .install(
            &db.pool,
            crate::common::plugins::zip_with_manifest(&files),
            Source::Upload,
        )
        .await;
    assert!(refused.is_err());
    // Checked before its database is opened and migrated.
    let plugin_dir = state.config.plugins_dir.join("hello");
    assert!(!plugin_dir.join("data.sqlite").exists());
}

#[actix_web::test]
async fn a_package_whose_migration_fails_leaves_no_files_and_an_installed_one_keeps_its_own() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    let broken = |version: &str| {
        let files: Vec<_> = crate::common::plugins::files_of(&hello_rtp())
            .into_iter()
            .map(|(path, bytes)| match path.as_str() {
                p if p.starts_with("migrations/") => (path, b"nonsense;".to_vec()),
                "rustrak-plugin.toml" => (
                    path,
                    String::from_utf8(bytes)
                        .unwrap()
                        .replacen(
                            "version = \"0.1.0\"",
                            &format!("version = \"{version}\""),
                            1,
                        )
                        .into_bytes(),
                ),
                _ => (path, bytes),
            })
            .collect();
        crate::common::plugins::zip_with_manifest(&files)
    };
    let plugin_dir = state.config.plugins_dir.join("hello");

    // A first install refused at its migrations: nothing of it on disk.
    let refused = state
        .plugins
        .install(&db.pool, broken("0.1.0"), Source::Upload)
        .await;
    assert!(refused.is_err());
    assert!(
        !plugin_dir.exists(),
        "{} was left behind",
        plugin_dir.display()
    );

    // An upgrade refused the same way: the installed version and its data stay.
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let refused = state
        .plugins
        .install(&db.pool, broken("0.2.0"), Source::Upload)
        .await;
    assert!(refused.is_err());
    assert!(plugin_dir.join("data.sqlite").exists());
    assert!(plugin_dir.join("0.1.0").exists());
    assert!(!plugin_dir.join("0.2.0").exists());
}

#[actix_web::test]
async fn an_upgrade_removes_the_version_it_replaces() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let next = crate::common::plugins::hello_rtp_with(|m| {
        m.replacen("version = \"0.1.0\"", "version = \"0.2.0\"", 1)
    });
    state
        .plugins
        .install(&db.pool, next, Source::Upload)
        .await
        .unwrap();
    let plugin_dir = state.config.plugins_dir.join("hello");
    assert!(plugin_dir.join("0.2.0").exists());
    assert!(
        !plugin_dir.join("0.1.0").exists(),
        "the old version is gone"
    );
    assert!(plugin_dir.join("data.sqlite").exists(), "its data stays");
}
