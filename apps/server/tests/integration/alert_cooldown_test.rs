//! Disabled cooldown must not suppress distinct events or weaken replay deduplication.

use crate::common::TestDb;
use chrono::{DateTime, Duration, Utc};
use rustrak::models::{
    AlertRule, AlertRuleChannelInput, AlertType, CreateAlertIntegration, CreateAlertRule,
    CreateProject, ProviderType,
};
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{AlertService, IssueService, ProjectService};
use serde_json::json;
use uuid::Uuid;

async fn exercise_cooldown(
    minutes: i32,
    last_triggered_at: Option<DateTime<Utc>>,
    expected_skipped: i64,
) {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: format!("Cooldown proof {}", Uuid::new_v4()),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let channel = AlertService::create_channel(
        &db.pool,
        &Default::default(),
        CreateAlertIntegration {
            name: "Local-only test sink".into(),
            provider_type: ProviderType::Webhook,
            credentials: json!({"url": "http://127.0.0.1:9/unused"}),
            is_enabled: true,
        },
    )
    .await
    .unwrap();
    let rule = AlertService::create_rule(
        &db.pool,
        project.id,
        CreateAlertRule {
            name: "Cooldown proof".into(),
            alert_type: AlertType::NewIssue,
            conditions: json!({}),
            cooldown_minutes: minutes,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel.id,
                routing_override: json!({}),
            }],
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE alert_rules SET last_triggered_at = $1 WHERE id = $2")
        .bind(last_triggered_at)
        .bind(rule.id)
        .execute(&db.pool)
        .await
        .unwrap();
    for index in 0..2 {
        let issue = IssueService::create(
            &db.pool,
            project.id,
            Utc::now(),
            &DenormalizedFields {
                calculated_type: "Error".into(),
                calculated_value: format!("Distinct issue {index}"),
                transaction: "/test".into(),
                last_frame_filename: "test.rs".into(),
                last_frame_module: "test".into(),
                last_frame_function: "test".into(),
                culprit: "test".into(),
                logger: String::new(),
                release: String::new(),
            },
            Some("error"),
            Some("rust"),
        )
        .await
        .unwrap();
        let event = Uuid::new_v4();
        // Replaying the same occurrence still owns exactly one delivery record.
        for _ in 0..2 {
            AlertService::trigger_event_alert(
                &db.pool,
                &project,
                &issue,
                AlertType::NewIssue,
                event,
                Default::default(),
                "https://dashboard.example.invalid",
                &Default::default(),
            )
            .await
            .unwrap();
        }
    }
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM alert_history WHERE project_id = $1")
        .bind(project.id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let skipped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM alert_history WHERE project_id = $1 AND status = 'skipped'",
    )
    .bind(project.id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(total, 2, "replays must not create duplicate deliveries");
    assert_eq!(
        skipped, expected_skipped,
        "only an enabled cooldown may suppress an alert"
    );
}

#[tokio::test]
async fn zero_cooldown_allows_distinct_alerts_after_clock_moves_backward() {
    // A future timestamp reproduces disabled cooldown deterministically on both
    // backends, without depending on calls landing in the same SQLite second.
    exercise_cooldown(0, Some(Utc::now() + Duration::minutes(1)), 0).await;
}

#[tokio::test]
async fn zero_cooldown_allows_first_and_subsequent_alerts() {
    exercise_cooldown(0, None, 0).await;
}

#[tokio::test]
async fn negative_cooldown_behaves_as_disabled() {
    // The API does not reject negatives, and SQLite cannot parse "--5 minutes".
    exercise_cooldown(-5, Some(Utc::now()), 0).await;
}

#[tokio::test]
async fn positive_cooldown_still_suppresses_and_deduplicates_replays() {
    exercise_cooldown(60, Some(Utc::now()), 2).await;
}

#[tokio::test]
async fn expired_positive_cooldown_allows_one_alert_and_restarts() {
    exercise_cooldown(60, Some(Utc::now() - Duration::minutes(61)), 1).await;
}

async fn reservation_fixture(minutes: i32) -> (TestDb, AlertRule) {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: format!("Reservation proof {}", Uuid::new_v4()),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let rule = AlertService::create_rule(
        &db.pool,
        project.id,
        CreateAlertRule {
            name: "Reservation proof".into(),
            alert_type: AlertType::NewIssue,
            conditions: json!({}),
            cooldown_minutes: minutes,
            channels: Vec::new(),
        },
    )
    .await
    .unwrap();
    (db, rule)
}

#[tokio::test]
#[cfg(feature = "sqlite")]
async fn zero_cooldown_allows_distinct_timestamps_in_the_same_sqlite_second() {
    let (db, rule) = reservation_fixture(0).await;
    let now = DateTime::parse_from_rfc3339("2026-09-28T12:00:00.500Z")
        .unwrap()
        .with_timezone(&Utc);
    sqlx::query("UPDATE alert_rules SET last_triggered_at = $1 WHERE id = $2")
        .bind(now - Duration::milliseconds(200))
        .bind(rule.id)
        .execute(&db.pool)
        .await
        .unwrap();
    // These distinct instants always truncate to the same SQLite second.
    assert!(
        AlertService::reserve_cooldown_at_for_test(&db.pool, &rule, now)
            .await
            .unwrap(),
        "zero cooldown must allow the reservation without a timestamp gap"
    );
}

async fn exercise_changed_cooldown(previous: i32, current: i32, expected_reserved: bool) {
    let (db, stale_rule) = reservation_fixture(previous).await;
    let now = DateTime::parse_from_rfc3339("2026-09-28T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    // Model an admin edit committed after trigger_alert read the rule, but
    // before its atomic reservation, without relying on task scheduling.
    sqlx::query(
        "UPDATE alert_rules SET cooldown_minutes = $1, last_triggered_at = $2 WHERE id = $3",
    )
    .bind(current)
    .bind(now - Duration::minutes(10))
    .bind(stale_rule.id)
    .execute(&db.pool)
    .await
    .unwrap();
    let reserved = AlertService::reserve_cooldown_at_for_test(&db.pool, &stale_rule, now)
        .await
        .unwrap();
    assert_eq!(reserved, expected_reserved, "use the current cooldown row");
}

#[tokio::test]
async fn zero_changed_to_positive_cooldown_uses_current_setting() {
    exercise_changed_cooldown(0, 60, false).await;
}

#[tokio::test]
async fn increased_positive_cooldown_uses_current_setting() {
    exercise_changed_cooldown(5, 60, false).await;
}

#[tokio::test]
async fn positive_changed_to_zero_cooldown_uses_current_setting() {
    exercise_changed_cooldown(60, 0, true).await;
}
