//! A plugin's permissions in Rustrak's RBAC: listed in the catalog beside
//! Rustrak's own, put in roles and personal permissions, and what lets
//! someone through a plugin route that requires one.

use actix_web::http::StatusCode;
use actix_web::test;
use rustrak::models::UserRole;
use rustrak::plugins::host::Source;
use serde_json::json;

use crate::common::{self, plugins::hello_rtp, TestDb};

#[actix_web::test]
async fn a_role_grants_a_plugins_permission_like_rustraks_own() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
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
    let owner = common::create_test_token(&db.pool).await;
    let (web, _) = common::create_ingest_project(&db.pool, "web").await;

    // The catalog lists what the plugin declares, under its own key.
    let (_, catalog) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri("/api/access/catalog")
            .insert_header(crate::common::bearer(&owner))
    );
    let hits = catalog["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["key"] == "hello:hits")
        .expect("hello:hits in the catalog");
    assert_eq!(hits["plugin"], "hello");
    assert_eq!(hits["scope"], "project");
    assert_eq!(hits["actions"], json!(["read", "delete"]));
    assert_eq!(hits["plugin_name"], "Hello");
    assert_eq!(hits["label"]["es"], "Visitas");

    // A permission no installed plugin declares is refused.
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::post()
            .uri("/api/roles")
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({
                "name": "Bogus",
                "permissions": [],
                "plugin_permissions": [{ "resource": "hello:nope", "action": "read" }],
            }))
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, role) = crate::call_json!(
        &app,
        test::TestRequest::post()
            .uri("/api/roles")
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({
                "name": "Hit reader",
                // Only the plugin's: it alone shows the project.
                "permissions": [],
                "plugin_permissions": [{ "resource": "hello:hits", "action": "read" }],
            }))
    );
    assert_eq!(status, StatusCode::CREATED, "{role}");
    assert_eq!(
        role["plugin_permissions"],
        json!([{ "resource": "hello:hits", "action": "read" }])
    );

    let member = common::seed_user(&db.pool, "reader@example.com", UserRole::Member).await;
    rustrak::services::RoleService::assign(
        &db.pool,
        member.id,
        role["id"].as_i64().unwrap() as i32,
        Some(web),
    )
    .await
    .unwrap();
    let reader = common::token_for(&db.pool, member.id).await;

    // Through the route that requires it, and not the one requiring delete.
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{web}/plugins/hello/hits"))
            .insert_header(crate::common::bearer(&reader))
    );
    assert_eq!(status, StatusCode::OK);
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/api/projects/{web}/plugins/hello/hits"))
            .insert_header(crate::common::bearer(&reader))
    );
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{web}/plugins/hello/count"))
            .insert_header(crate::common::bearer(&reader))
    );
    assert_eq!(status, StatusCode::FORBIDDEN, "stats:read is not held");

    // What the dashboard reads to show or hide the plugin's controls.
    let (_, me) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri("/api/access/me")
            .insert_header(crate::common::bearer(&reader))
    );
    assert_eq!(
        me["plugins"]["projects"],
        json!([{ "project_id": web, "permissions": [{ "resource": "hello:hits", "action": "read" }] }])
    );

    // And they are one of the project's members.
    let (status, members) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{web}/members"))
            .insert_header(crate::common::bearer(&owner))
    );
    assert_eq!(status, StatusCode::OK, "{members}");
    assert_eq!(members[0]["email"], "reader@example.com", "{members}");
    assert_eq!(members[0]["roles"][0]["name"], "Hit reader");
}

#[actix_web::test]
async fn a_plugin_permission_too_long_for_varchar_50_round_trips() {
    let db = TestDb::new().await;
    // `<32>:<32>`, the longest key a plugin can declare.
    let key = format!("{}:{}", "a".repeat(32), "b".repeat(32));
    let grants = vec![rustrak::models::PluginGrant {
        resource: key.clone(),
        action: rustrak::models::rbac::Action::Read,
    }];
    let role = rustrak::services::RoleService::create(&db.pool, "Long", "", vec![], grants.clone())
        .await
        .unwrap();
    assert_eq!(role.plugin_permissions, grants);
    let token = rustrak::services::AuthTokenService::create_for_user(
        &db.pool,
        rustrak::models::CreateAuthToken {
            permissions: Some(vec![]),
            plugin_permissions: grants.clone(),
            ..Default::default()
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(token.plugin_permissions, grants);
}

#[actix_web::test]
async fn personal_permissions_carry_a_plugins_permissions_too() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
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
    let owner = common::create_test_token(&db.pool).await;
    let (web, _) = common::create_ingest_project(&db.pool, "web").await;
    let member = common::seed_user(&db.pool, "own@example.com", UserRole::Member).await;

    let (status, body) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/users/{}/assignments", member.id))
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({
                "assignments": [],
                "personal": {
                    "permissions": [{ "resource": "issues", "action": "read" }],
                    "plugin_permissions": [{ "resource": "hello:stats", "action": "read" }],
                    "project_ids": [web],
                },
            }))
    );
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["personal_plugin_permissions"],
        json!([{ "resource": "hello:stats", "action": "read" }])
    );

    let token = common::token_for(&db.pool, member.id).await;
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{web}/plugins/hello/count"))
            .insert_header(crate::common::bearer(&token))
    );
    assert_eq!(status, StatusCode::OK);
}

#[actix_web::test]
async fn a_role_still_saves_while_a_plugin_it_names_is_uninstalled() {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, hello_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;
    let body = json!({
        "name": "Hit reader",
        "permissions": [],
        "plugin_permissions": [{ "resource": "hello:hits", "action": "read" }],
    });
    let (status, role) = crate::call_json!(
        &app,
        test::TestRequest::post()
            .uri("/api/roles")
            .insert_header(crate::common::bearer(&owner))
            .set_json(&body)
    );
    assert_eq!(status, StatusCode::CREATED, "{role}");
    // Kept so a reinstall brings it back.
    state
        .plugins
        .uninstall(&db.pool, "hello", false)
        .await
        .unwrap();

    // Renamed as it is, the kept grant included: still saved.
    let (status, saved) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/roles/{}", role["id"]))
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({
                "name": "Readers",
                "permissions": [],
                "plugin_permissions": [{ "resource": "hello:hits", "action": "read" }],
            }))
    );
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["plugin_permissions"], body["plugin_permissions"]);

    // A grant it did not hold, of a plugin not installed, is still refused.
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/roles/{}", role["id"]))
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({
                "name": "Readers",
                "permissions": [],
                "plugin_permissions": [
                    { "resource": "hello:hits", "action": "read" },
                    { "resource": "hello:hits", "action": "delete" },
                ],
            }))
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// The app with `hello` installed and on everywhere, an owner token, and a
/// project.
async fn hello_app() -> (TestDb, rustrak::app::AppState, String, i32) {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
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
    let owner = common::create_test_token(&db.pool).await;
    let (web, _) = common::create_ingest_project(&db.pool, "web").await;
    (db, state, owner, web)
}

#[actix_web::test]
async fn saving_a_role_without_its_plugin_grants_keeps_them() {
    let (db, state, owner, web) = hello_app().await;
    let app = common::app_with(&state).await;
    let grants = json!([{ "resource": "hello:hits", "action": "read" }]);
    let (_, role) = crate::call_json!(
        &app,
        test::TestRequest::post()
            .uri("/api/roles")
            .insert_header(crate::common::bearer(&owner))
            .set_json(
                json!({ "name": "Hit reader", "permissions": [], "plugin_permissions": grants })
            )
    );
    // What the dashboard sends: no plugin_permissions at all.
    let (status, saved) = crate::call_json!(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/roles/{}", role["id"]))
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({ "name": "Readers", "description": "", "permissions": [] }))
    );
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["plugin_permissions"], grants);

    let member = common::seed_user(&db.pool, "p@example.com", UserRole::Member).await;
    let put = |personal: serde_json::Value| {
        test::TestRequest::put()
            .uri(&format!("/api/users/{}/assignments", member.id))
            .insert_header(crate::common::bearer(&owner))
            .set_json(json!({ "assignments": [], "personal": personal }))
    };
    crate::call_json!(
        &app,
        put(json!({ "permissions": [], "plugin_permissions": grants, "project_ids": [web] }))
    );
    let (status, body) = crate::call_json!(
        &app,
        put(
            json!({ "permissions": [{ "resource": "issues", "action": "read" }], "project_ids": [web] })
        )
    );
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["personal_plugin_permissions"], grants);
}

/// A member holding `hello:hits:read` (and issues:read, api-tokens:create)
/// on `projects`.
async fn hit_reader(
    db: &TestDb,
    owner: &str,
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    plugin: bool,
    projects: &[i32],
) -> rustrak::models::User {
    let (status, role) = crate::call_json!(
        app,
        test::TestRequest::post()
            .uri("/api/roles")
            .insert_header(crate::common::bearer(owner))
            .set_json(json!({
                "name": format!("Hit reader {plugin}"),
                "permissions": [
                    { "resource": "issues", "action": "read" },
                    { "resource": "api-tokens", "action": "create" },
                ],
                "plugin_permissions": if plugin { json!([{ "resource": "hello:hits", "action": "read" }]) } else { json!([]) },
            }))
    );
    assert_eq!(status, StatusCode::CREATED, "{role}");
    let member = common::seed_user(
        &db.pool,
        &format!("r{plugin}@example.com"),
        UserRole::Member,
    )
    .await;
    for &p in projects {
        rustrak::services::RoleService::assign(
            &db.pool,
            member.id,
            role["id"].as_i64().unwrap() as i32,
            Some(p),
        )
        .await
        .unwrap();
    }
    member
}

#[actix_web::test]
async fn a_project_limited_token_lists_plugin_grants_only_on_its_projects() {
    let (db, state, owner, web) = hello_app().await;
    let (api, _) = common::create_ingest_project(&db.pool, "api").await;
    let app = common::app_with(&state).await;
    let member = hit_reader(&db, &owner, &app, true, &[web, api]).await;
    let token = rustrak::services::AuthTokenService::create_for_user(
        &db.pool,
        rustrak::models::CreateAuthToken {
            project_ids: Some(vec![web]),
            ..Default::default()
        },
        Some(member.id),
    )
    .await
    .unwrap()
    .token;
    let (_, me) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri("/api/access/me")
            .insert_header(crate::common::bearer(&token))
    );
    assert_eq!(
        me["plugins"]["projects"],
        json!([{ "project_id": web, "permissions": [{ "resource": "hello:hits", "action": "read" }] }])
    );
}

#[actix_web::test]
async fn a_token_can_be_limited_to_a_plugins_permission() {
    let (db, state, owner, web) = hello_app().await;
    let app = common::app_with(&state).await;
    let member = hit_reader(&db, &owner, &app, true, &[web]).await;
    let session = common::login(&app, &member.email).await;
    let (status, created) = crate::call_json!(
        &app,
        test::TestRequest::post()
            .uri("/api/tokens")
            .cookie(session.clone())
            .set_json(json!({
                "permissions": [],
                "plugin_permissions": [{ "resource": "hello:hits", "action": "read" }],
            }))
    );
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(
        created["plugin_permissions"],
        json!([{ "resource": "hello:hits", "action": "read" }])
    );
    let token = created["token"].as_str().unwrap();

    // The plugin's permission, and none of Rustrak's.
    let (_, me) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri("/api/access/me")
            .insert_header(crate::common::bearer(token))
    );
    assert_eq!(
        me["plugins"]["projects"],
        json!([{ "project_id": web, "permissions": [{ "resource": "hello:hits", "action": "read" }] }])
    );
    assert_eq!(me["projects"], json!([]));
    assert_eq!(me["instance"], json!([]));

    // It reaches the plugin's route, and none of Rustrak's on the project.
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{web}/plugins/hello/hits"))
            .insert_header(crate::common::bearer(token))
    );
    assert_eq!(status, StatusCode::OK);
    let (status, _) = crate::call_json!(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{web}/issues"))
            .insert_header(crate::common::bearer(token))
    );
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn a_token_cannot_be_given_a_plugin_permission_its_creator_lacks() {
    let (db, state, owner, web) = hello_app().await;
    let app = common::app_with(&state).await;
    let member = hit_reader(&db, &owner, &app, false, &[web]).await;
    let session = common::login(&app, &member.email).await;
    let (status, body) = crate::call_json!(
        &app,
        test::TestRequest::post()
            .uri("/api/tokens")
            .cookie(session)
            .set_json(json!({
                "plugin_permissions": [{ "resource": "hello:hits", "action": "read" }],
            }))
    );
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}
