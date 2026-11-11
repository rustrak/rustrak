//! Unit tests for the Sentry log item-container model (OurLog).
//!
//! Logs arrive batched in an item container: `{"items":[OurLog, ...]}`.
//! These tests cover expanding that container into individual `LogItem`s.

use rustrak::ingest::envelope::EnvelopeItemKind;
use rustrak::models::log::LogContainer;

#[test]
fn test_log_does_not_require_event_id() {
    let kind = EnvelopeItemKind::Log(bytes::Bytes::from(b"{}".to_vec()));
    assert!(!kind.requires_event());
}

#[test]
fn test_parse_container_yields_logs() {
    let body = br#"{"items":[{
        "timestamp":1544719860.0,
        "trace_id":"5b8efff798038103d269b633813fc60c",
        "span_id":"eee19b7ec3c1b174",
        "level":"info",
        "body":"Example log record",
        "attributes":{"string.attribute":{"value":"some string","type":"string"}}
    }]}"#;

    let logs = LogContainer::parse(body).unwrap();

    assert_eq!(logs.len(), 1);
    let log = &logs[0];
    assert_eq!(log.trace_id, "5b8efff798038103d269b633813fc60c");
    assert_eq!(log.span_id.as_deref(), Some("eee19b7ec3c1b174"));
    assert_eq!(log.level, "info");
    assert_eq!(log.body, "Example log record");
}

#[cfg(test)]
mod level2 {
    use crate::common::TestDb;
    use chrono::Utc;
    use rustrak::digest::processors::{LogsProcessor, ProcessorCtx};
    use rustrak::models::CreateProject;
    use rustrak::services::ProjectService;
    use sqlx::Row;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_log_container_expanded_into_rows() {
        // A log container with N items must produce N rows in the `logs` table,
        // with denormalized level/body/trace_id columns populated per item.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "logs-store".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let body = br#"{"items":[
            {"timestamp":1704801600.0,"trace_id":"5b8efff798038103d269b633813fc60c","span_id":"eee19b7ec3c1b174","level":"error","body":"boom","attributes":{"k":{"value":"v","type":"string"}}},
            {"timestamp":1704801601.0,"trace_id":"5b8efff798038103d269b633813fc60c","level":"info","body":"ok"}
        ]}"#
        .to_vec();

        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
            keep_ai_spans: true,
        };

        let body = bytes::Bytes::from(body);
        LogsProcessor.process(body.clone(), &ctx).await.unwrap();
        LogsProcessor.process(body, &ctx).await.unwrap();

        const QUERY: &str =
            "SELECT level, body, trace_id FROM logs WHERE project_id = $1 ORDER BY timestamp";

        let rows = sqlx::query(QUERY)
            .bind(project.id)
            .fetch_all(&db.pool)
            .await
            .unwrap();

        assert_eq!(
            rows.len(),
            2,
            "replaying a container must not duplicate rows"
        );
        let level0: String = rows[0].get("level");
        let body0: String = rows[0].get("body");
        let trace0: String = rows[0].get("trace_id");
        assert_eq!(level0, "error");
        assert_eq!(body0, "boom");
        assert_eq!(trace0, "5b8efff798038103d269b633813fc60c");
    }

    #[tokio::test]
    async fn test_non_object_attributes_coerced_to_empty_object() {
        // A non-conforming SDK may send `attributes` as an array/primitive.
        // The stored column must always hold a JSON object so the read API and
        // client schema (z.record) never choke on one bad row.
        let db = TestDb::new().await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "logs-bad-attrs".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();

        let body = br#"{"items":[
            {"timestamp":1704801600.0,"trace_id":"aaaa","level":"info","body":"x","attributes":[1,2,3]}
        ]}"#
        .to_vec();
        let ctx = ProcessorCtx {
            pool: db.pool.clone(),
            project_id: project.id,
            event_id: Uuid::new_v4(),
            ingested_at: Utc::now(),
            remote_addr: None,
            keep_ai_spans: true,
        };
        LogsProcessor
            .process(bytes::Bytes::from(body), &ctx)
            .await
            .unwrap();

        const QUERY: &str = "SELECT attributes FROM logs WHERE project_id = $1";

        let stored: serde_json::Value = sqlx::query_scalar(QUERY)
            .bind(project.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert!(
            stored.is_object(),
            "non-object attributes must be coerced to an object, got {stored}"
        );
    }
}

/// Issue #427: sentry-go marshals `Log.Timestamp` (`time.Time`) as an RFC3339
/// string. Relay's `OurLog.timestamp` is `Annotated<Timestamp>`, which accepts
/// both strings and numbers (`relay-event-schema/src/protocol/types.rs`).
#[test]
fn test_parse_container_accepts_rfc3339_timestamp() {
    for ts in [
        "2026-10-19T09:50:34.997868399Z",
        "2026-10-19T11:49:02.434963+02:00",
    ] {
        let body = format!(
            r#"{{"items":[{{"timestamp":"{ts}","trace_id":"5b8efff798038103d269b633813fc60c","level":"info","body":"hello"}}]}}"#
        );
        let logs = LogContainer::parse(body.as_bytes()).expect(ts);
        let expected = chrono::DateTime::parse_from_rfc3339(ts).unwrap();
        assert_eq!(logs[0].timestamp, Some(expected.to_utc()));
    }
}

/// Relay rejects a timestamp at or before the epoch, not one just after it.
#[test]
fn test_parse_container_keeps_sub_millisecond_after_epoch() {
    let logs =
        LogContainer::parse(br#"{"items":[{"timestamp":0.0005},{"timestamp":0.0}]}"#).unwrap();
    assert!(logs[0].timestamp.is_some());
    assert!(logs[1].timestamp.is_none());
}

#[test]
fn test_volume_counts_a_level_it_does_not_know_as_info() {
    // Ingestion keeps levels outside the six; the chart must not drop them.
    let mut point = rustrak::models::LogVolumePoint::empty(0);
    point.add("notice", 2);
    point.add("info", 1);
    assert_eq!(point.info, 3);
}
