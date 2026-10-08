use super::support::{uri, Fixture};
use actix_web::{test, web, App};
use rustrak::models::ProjectRole;
use rustrak::services::ProjectMemberService;
use serde_json::{json, Value};
use uuid::Uuid;

#[actix_web::test]
async fn both_selectors_page_across_issues_without_payloads_or_project_leaks() {
    let f = Fixture::new().await;
    let mut expected = Vec::new();
    for index in 0..45 {
        let tags = match index % 3 {
            0 => json!({"request.id": "lookup-request"}),
            1 => json!([["request.id", "lookup-request"]]),
            _ => json!([{"key": "request.id", "value": "lookup-request"}]),
        };
        expected.push(
            f.insert(
                f.project,
                Some(f.issues[index % 2]),
                json!({
                    "user": {"id": "lookup-user"}, "tags": tags,
                    "private_payload": "not a summary",
                }),
            )
            .await,
        );
    }
    f.insert(
        f.other,
        None,
        json!({"user":{"id":"lookup-user"},"tags":{"request.id":"lookup-request"}}),
    )
    .await;
    f.insert(
        f.project,
        None,
        json!({"user":{"id":"different"},"tags":{"request.id":"different"}}),
    )
    .await;
    expected.sort_by(|a, b| b.cmp(a));
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(f.db.pool.clone()))
            .configure(rustrak::routes::events::configure)
            .configure(rustrak::routes::projects::configure),
    )
    .await;
    for (selector, value) in [("user_id", "lookup-user"), ("request_id", "lookup-request")] {
        let mut cursor: Option<String> = None;
        let mut actual = Vec::new();
        for count in [20, 20, 5] {
            let mut params = vec![(selector, value)];
            if let Some(ref cursor) = cursor {
                params.push(("cursor", cursor));
            }
            let response = test::call_service(
                &app,
                test::TestRequest::get()
                    .uri(&uri(f.project, &params))
                    .insert_header(("Authorization", format!("Bearer {}", f.token)))
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), 200);
            let page: Value = test::read_body_json(response).await;
            let items = page["items"].as_array().unwrap();
            assert_eq!(items.len(), count);
            assert_eq!(page["has_more"], count == 20);
            for item in items {
                assert!(item.get("data").is_none());
                assert!(f.issues.iter().any(|id| item["issue_id"] == id.to_string()));
                actual.push(Uuid::parse_str(item["id"].as_str().unwrap()).unwrap());
            }
            cursor = page["next_cursor"].as_str().map(str::to_owned);
        }
        assert_eq!(actual, expected);
        assert!(cursor.is_none());
    }
}

#[actix_web::test]
async fn exact_string_lookup_handles_byte_limits_and_untrusted_sdk_values() {
    let f = Fixture::new().await;
    let boundary = "é".repeat(100);
    let oversized = "é".repeat(101);
    let huge = (0..10000)
        .map(|index| format!("{index:08x}"))
        .collect::<String>();
    for data in [
        json!({"user":{"id":boundary},"tags":{"request.id":boundary}}),
        json!({"user":{"id":oversized},"tags":{"request.id":huge}}),
        json!({"user":{"id":55},"tags":{"request.id":55}}),
        json!({"user":{"id":null},"tags":{"request.id":null}}),
        json!({"user":{"id":{"nested":"55"}},"tags":{"request.id":["55"]}}),
        json!({"user":{"id":""},"tags":{"request.id":""}}),
        json!({"tags":{"request":{"id":"nested"}},"request_id":"root"}),
        json!({}),
        json!({"user":{"id":"u' OR 1=1--"},"tags":{"request.id":"literal%_"}}),
    ] {
        f.insert(f.project, None, data).await;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(f.db.pool.clone()))
            .configure(rustrak::routes::events::configure),
    )
    .await;
    for (selector, value, count) in [
        ("user_id", boundary.as_str(), 1),
        ("request_id", boundary.as_str(), 1),
        ("user_id", "55", 0),
        ("request_id", "55", 0),
        ("request_id", "nested", 0),
        ("request_id", "root", 0),
        ("user_id", "u' OR 1=1--", 1),
        ("request_id", "literal%_", 1),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri(f.project, &[(selector, value)]))
                .insert_header(("Authorization", format!("Bearer {}", f.token)))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 200);
        let page: Value = test::read_body_json(response).await;
        assert_eq!(page["items"].as_array().unwrap().len(), count);
    }
    for selector in ["user_id", "request_id"] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri(f.project, &[(selector, &oversized)]))
                .insert_header(("Authorization", format!("Bearer {}", f.token)))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 400);
    }
}

#[actix_web::test]
async fn invalid_selectors_and_cursors_fail_explicitly() {
    let f = Fixture::new().await;
    for _ in 0..21 {
        f.insert(f.project, None, json!({"user":{"id":"u"}})).await;
    }
    ProjectMemberService::upsert(&f.db.pool, f.other, f.user, ProjectRole::Viewer)
        .await
        .unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(f.db.pool.clone()))
            .configure(rustrak::routes::events::configure),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&uri(f.project, &[("user_id", "u")]))
            .insert_header(("Authorization", format!("Bearer {}", f.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let page: Value = test::read_body_json(response).await;
    let cursor = page["next_cursor"].as_str().unwrap();
    for params in [
        vec![],
        vec![("user_id", "")],
        vec![("request_id", "")],
        vec![("user_id", "u"), ("request_id", "r")],
        vec![("user_id", " u")],
        vec![("user_id", "u\n")],
        vec![("user_id", "u"), ("unknown", "x")],
        vec![("user_id", "u"), ("user_id", "v")],
        vec![("user_id", "u"), ("cursor", "invalid")],
        vec![("user_id", "v"), ("cursor", cursor)],
        vec![("request_id", "u"), ("cursor", cursor)],
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri(f.project, &params))
                .insert_header(("Authorization", format!("Bearer {}", f.token)))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 400, "params: {params:?}");
    }
    for (project, params) in [
        (f.other, vec![("user_id", "u"), ("cursor", cursor)]),
        (0, vec![("user_id", "u")]),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri(project, &params))
                .insert_header(("Authorization", format!("Bearer {}", f.token)))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 400);
    }
    let huge_cursor = "a".repeat(2049);
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&uri(
                f.project,
                &[("user_id", "u"), ("cursor", &huge_cursor)],
            ))
            .insert_header(("Authorization", format!("Bearer {}", f.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 400);
}

#[actix_web::test]
async fn viewer_access_and_project_existence_are_enforced() {
    let f = Fixture::new().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(f.db.pool.clone()))
            .configure(rustrak::routes::events::configure),
    )
    .await;
    let url = uri(f.project, &[("user_id", "missing")]);
    let response = test::call_service(&app, test::TestRequest::get().uri(&url).to_request()).await;
    assert_eq!(response.status(), 401);
    for (project, token, status) in [
        (f.project, &f.token, 200),
        (f.other, &f.token, 404),
        (i32::MAX, &f.token, 404),
        (i32::MAX, &f.admin_token, 404),
        (f.project, &f.admin_token, 200),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri(project, &[("user_id", "missing")]))
                .insert_header(("Authorization", format!("Bearer {token}")))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), status);
        if status == 200 {
            let page: Value = test::read_body_json(response).await;
            assert_eq!(page["items"], json!([]));
            assert_eq!(page["has_more"], false);
        }
    }
    sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2")
        .bind(false)
        .bind(f.user)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&url)
            .insert_header(("Authorization", format!("Bearer {}", f.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 401);
    sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2")
        .bind(true)
        .bind(f.user)
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM project_members WHERE project_id = $1 AND user_id = $2")
        .bind(f.project)
        .bind(f.user)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&url)
            .insert_header(("Authorization", format!("Bearer {}", f.token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 404);
}
