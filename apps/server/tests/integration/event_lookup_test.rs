//! The SDK UUID is distinct from the stored row UUID and is scoped to a project.
use crate::common::TestDb;
use actix_web::{test, web, App};
use chrono::Utc;
use rustrak::models::{CreateAuthToken, CreateProject, CreateUserRequest, ProjectRole, UserRole};
use rustrak::routes;
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{
    AuthTokenService, EventService, IssueService, ProjectMemberService, ProjectService,
    UsersService,
};
use serde_json::{json, Value};
use uuid::Uuid;

#[actix_web::test]
async fn sdk_event_lookup_preserves_project_scope_and_viewer_authorization() {
    let db = TestDb::new().await;
    let viewer = UsersService::create_user(
        &db.pool,
        &CreateUserRequest {
            email: "lookup@example.test".into(),
            password: "lookup-test-password".into(),
        },
        UserRole::Member,
    )
    .await
    .unwrap();
    let token = AuthTokenService::create_for_user(
        &db.pool,
        CreateAuthToken { description: None },
        Some(viewer.id),
    )
    .await
    .unwrap()
    .token;
    let fields = DenormalizedFields {
        calculated_type: "TypeError".into(),
        calculated_value: "lookup proof".into(),
        transaction: String::new(),
        last_frame_filename: String::new(),
        last_frame_module: String::new(),
        last_frame_function: String::new(),
        culprit: String::new(),
        logger: String::new(),
        release: String::new(),
    };
    let sdk_id = Uuid::new_v4();
    let mut records = Vec::new();
    for name in ["Accessible", "Other"] {
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: name.into(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();
        ProjectMemberService::upsert(&db.pool, project.id, viewer.id, ProjectRole::Viewer)
            .await
            .unwrap();
        let issue = IssueService::create(
            &db.pool,
            project.id,
            Utc::now(),
            &fields,
            Some("error"),
            Some("javascript"),
        )
        .await
        .unwrap();
        let grouping_id: i32 = sqlx::query_scalar("INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash) VALUES ($1, $2, $3, $4) RETURNING id")
            .bind(project.id).bind(issue.id).bind("lookup").bind("0".repeat(64)).fetch_one(&db.pool).await.unwrap();
        let row_id = EventService::create(
            &db.pool,
            sdk_id,
            project.id,
            issue.id,
            grouping_id,
            &json!({"message":"lookup proof","marker":name}),
            Utc::now(),
            &fields,
            None,
            None,
        )
        .await
        .unwrap();
        records.push((project.id, issue.id, row_id, name));
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .configure(routes::events::configure),
    )
    .await;
    for (project_id, issue_id, row_id, name) in &records {
        for id in [sdk_id.to_string(), sdk_id.simple().to_string()] {
            let req = test::TestRequest::get()
                .uri(&format!("/api/projects/{project_id}/events/sentry/{id}"))
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request();
            let response = test::call_service(&app, req).await;
            assert_eq!(response.status(), 200);
            let body: Value = test::read_body_json(response).await;
            assert_eq!(body["event_id"], sdk_id.to_string());
            assert_eq!(body["id"], row_id.to_string());
            assert_eq!(body["issue_id"], issue_id.to_string());
            assert_eq!(body["data"]["marker"], *name);
        }
    }
    let project_id = records[0].0;
    for (id, status) in [
        (Uuid::new_v4().to_string(), 404),
        (records[0].2.to_string(), 404),
        ("invalid".into(), 404),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/projects/{project_id}/events/sentry/{id}"))
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), status);
    }
    let uri = format!("/api/projects/{project_id}/events/sentry/{sdk_id}");
    let response = test::call_service(&app, test::TestRequest::get().uri(&uri).to_request()).await;
    assert_eq!(response.status(), 401);
    // Removing membership must make even a known event UUID inaccessible.
    sqlx::query("DELETE FROM project_members WHERE project_id = $1 AND user_id = $2")
        .bind(project_id)
        .bind(viewer.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 404);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/2147483647/events/sentry/{sdk_id}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 404);
    sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2")
        .bind(false)
        .bind(viewer.id)
        .execute(&db.pool)
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{}/events/sentry/{sdk_id}",
                records[1].0
            ))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 401);
}
