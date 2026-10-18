use super::indexes::{create_lookups, drop_lookups};
use super::support::{timestamp, uri, Fixture};
use actix_web::{test, web, App};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

#[actix_web::test]
async fn request_tag_shapes_match_exact_strings_once_without_coercion() {
    let f = Fixture::new().await;
    let boundary = "é".repeat(100);
    let mut expected = Vec::new();
    for tags in [
        json!({"request.id":"request-1"}),
        json!([["other", "noise"], ["request.id", "request-1"]]),
        json!([{"key":"request.id", "value":"request-1"}]),
        json!([["request.id", "request-1"], {"key":"request.id","value":"request-1"}, ["request.id", "second-id"]]),
        json!([null, false, 5, "malformed JSON text", [], ["request.id"], ["request.id", "wrong-length", "extra"], ["request.id", 55], {"key":"request.id","value":"request-1"}]),
    ] {
        expected.push(f.insert(f.project, None, json!({"tags":tags})).await);
    }
    for tags in [
        json!([["request.id", boundary]]),
        json!([{"key":"request.id","value":boundary}]),
        json!([["request.id", "é".repeat(101)]]),
        json!([["request.id", ""]]),
        json!([
            ["request.id", 55],
            ["request.id", true],
            ["request.id", null]
        ]),
        json!([{"key":"request.id","value":55}]),
        json!([["Request.id", "request-1"]]),
        json!([{"key":"request","value":{"id":"request-1"}}]),
        json!("request-1"),
        json!(["request.id", "request-1"]),
    ] {
        f.insert(f.project, None, json!({"tags":tags})).await;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(f.db.pool.clone()))
            .configure(rustrak::routes::events::configure),
    )
    .await;
    for (value, count) in [
        ("request-1", 5),
        ("second-id", 1),
        (boundary.as_str(), 2),
        ("55", 0),
        ("true", 0),
        ("wrong-length", 0),
    ] {
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri(f.project, &[("request_id", value)]))
                .insert_header(("Authorization", format!("Bearer {}", f.token)))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 200);
        let page: Value = test::read_body_json(response).await;
        let items = page["items"].as_array().unwrap();
        assert_eq!(items.len(), count, "selector: {value}");
        if value == "request-1" {
            let mut actual = items
                .iter()
                .map(|item| Uuid::parse_str(item["id"].as_str().unwrap()).unwrap())
                .collect::<Vec<_>>();
            actual.sort();
            expected.sort();
            assert_eq!(actual, expected);
        }
    }
}

#[actix_web::test]
async fn historical_array_tags_backfill_without_rewriting_payloads() {
    let f = Fixture::new().await;
    drop_lookups(&f).await;
    let mut historical = Vec::new();
    for tags in [
        json!({"request.id":"historical"}),
        json!([["request.id", "historical"], ["request.id", "historical"]]),
        json!([{"key":"request.id","value":"historical"}]),
        json!([null, "noise", ["request.id", "historical"]]),
    ] {
        let data = json!({"tags":tags, "private_payload":"preserve me"});
        let id = f.insert(f.project, None, data.clone()).await;
        historical.push((id, data));
    }
    create_lookups(&f).await;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event_request_identities WHERE request_id = 'historical'",
    )
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert_eq!(count, 4);
    // Migration rollback must remove derived storage, not the original events.
    drop_lookups(&f).await;
    for (id, data) in historical {
        let stored: Value = sqlx::query_scalar("SELECT data FROM events WHERE id = $1")
            .bind(id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
        assert_eq!(stored, data);
    }
    create_lookups(&f).await;
}

#[actix_web::test]
async fn request_identities_follow_updates_deletes_and_transaction_rollback() {
    let f = Fixture::new().await;
    let original = json!({"tags":[["request.id","original"]]});
    let id = f.insert(f.project, None, original.clone()).await;
    let mut tx = f.db.pool.begin().await.unwrap();
    sqlx::query("UPDATE events SET data = $1 WHERE id = $2")
        .bind(json!({"tags":[{"key":"request.id","value":"rolled-back"}]}))
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    let tag: String =
        sqlx::query_scalar("SELECT request_id FROM event_request_identities WHERE event_id = $1")
            .bind(id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(tag, "original");
    let new_id = Uuid::new_v4();
    let later = timestamp() + chrono::Duration::seconds(1);
    sqlx::query(
        "UPDATE events SET id = $1, project_id = $2, timestamp = $3, data = $4 WHERE id = $5",
    )
    .bind(new_id)
    .bind(f.other)
    .bind(later)
    .bind(json!({"tags":[["request.id","new"], {"key":"request.id","value":"new"}]}))
    .bind(id)
    .execute(&f.db.pool)
    .await
    .unwrap();
    let rows: Vec<(Uuid, String, i32, DateTime<Utc>)> = sqlx::query_as(
        "SELECT event_id, request_id, project_id, timestamp FROM event_request_identities",
    )
    .fetch_all(&f.db.pool)
    .await
    .unwrap();
    assert_eq!(rows, vec![(new_id, "new".into(), f.other, later)]);
    sqlx::query("DELETE FROM events WHERE id = $1")
        .bind(new_id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM event_request_identities")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
