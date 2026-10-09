//! A rule with `min_level` alerts only for issues at or above that level, without
//! consuming its cooldown for the issues it ignores.
mod common;

use chrono::Utc;
use common::TestDb;
use rustrak::models::{
    AlertRule, AlertRuleChannelInput, AlertType, ChannelType, CreateAlertRule,
    CreateNotificationChannel, CreateProject, Issue, Project, UpdateAlertRule,
};
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{AlertService, IssueService, ProjectService};
use serde_json::json;
use uuid::Uuid;

struct Setup {
    db: TestDb,
    project: Project,
    rule: AlertRule,
}

async fn setup(conditions: serde_json::Value, cooldown_minutes: i32) -> Setup {
    let db = TestDb::new().await;
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: format!("Level proof {}", Uuid::new_v4()),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let channel = AlertService::create_channel(
        &db.pool,
        CreateNotificationChannel {
            name: "Local-only test sink".into(),
            provider_type: ChannelType::Webhook,
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
            name: "Level proof".into(),
            alert_type: AlertType::NewIssue,
            conditions,
            cooldown_minutes,
            channels: vec![AlertRuleChannelInput {
                integration_id: channel.id,
                routing_override: json!({}),
            }],
        },
    )
    .await
    .unwrap();
    Setup { db, project, rule }
}

async fn issue_at(setup: &Setup, level: Option<&str>, label: &str) -> Issue {
    IssueService::create(
        &setup.db.pool,
        setup.project.id,
        Utc::now(),
        &DenormalizedFields {
            calculated_type: "Error".into(),
            calculated_value: format!("Level proof {label}"),
            transaction: "/test".into(),
            last_frame_filename: "test.rs".into(),
            last_frame_function: "test".into(),
            last_frame_module: "test".into(),
            culprit: "test".into(),
            logger: String::new(),
            release: String::new(),
        },
        level,
        Some("rust"),
    )
    .await
    .unwrap()
}

async fn alert_new_issue(setup: &Setup, issue: &Issue) {
    AlertService::trigger_new_issue_alert(
        &setup.db.pool,
        &setup.project,
        issue,
        "https://dashboard.example.invalid",
    )
    .await
    .unwrap();
}

async fn history_rows(setup: &Setup) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM alert_history WHERE project_id = $1")
        .bind(setup.project.id)
        .fetch_one(&setup.db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn min_level_alerts_only_at_or_above_the_level() {
    let setup = setup(json!({"min_level": "error"}), 0).await;
    for (level, label) in [
        (Some("debug"), "debug"),
        (Some("info"), "info"),
        (Some("warning"), "warning"),
    ] {
        let issue = issue_at(&setup, level, label).await;
        alert_new_issue(&setup, &issue).await;
    }
    assert_eq!(
        history_rows(&setup).await,
        0,
        "ignored levels leave no trace"
    );

    for (level, label) in [
        (Some("error"), "error"),
        (Some("fatal"), "fatal"),
        (None, "missing"),
        (Some("critical"), "unrecognized"),
    ] {
        let issue = issue_at(&setup, level, label).await;
        alert_new_issue(&setup, &issue).await;
    }
    assert_eq!(
        history_rows(&setup).await,
        4,
        "error, fatal, a missing level and an unrecognized level all alert"
    );
}

#[tokio::test]
async fn a_rule_without_conditions_alerts_on_every_level() {
    let setup = setup(json!({}), 0).await;
    let issue = issue_at(&setup, Some("debug"), "debug").await;
    alert_new_issue(&setup, &issue).await;
    assert_eq!(history_rows(&setup).await, 1);
}

#[tokio::test]
async fn an_ignored_issue_does_not_start_the_cooldown() {
    let setup = setup(json!({"min_level": "error"}), 60).await;
    let warning = issue_at(&setup, Some("warning"), "warning").await;
    alert_new_issue(&setup, &warning).await;
    let error = issue_at(&setup, Some("error"), "error").await;
    alert_new_issue(&setup, &error).await;
    let skipped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM alert_history WHERE project_id = $1 AND status = 'skipped'",
    )
    .bind(setup.project.id)
    .fetch_one(&setup.db.pool)
    .await
    .unwrap();
    assert_eq!(history_rows(&setup).await, 1);
    assert_eq!(skipped, 0, "the warning must not make the error alert wait");
}

#[tokio::test]
async fn conditions_are_validated_on_create_and_update() {
    let setup = setup(json!({}), 0).await;
    for conditions in [
        json!({"minlevel": "error"}),
        json!({"min_level": "high"}),
        json!([]),
    ] {
        let created = AlertService::create_rule(
            &setup.db.pool,
            setup.project.id,
            CreateAlertRule {
                name: "Rejected".into(),
                alert_type: AlertType::Regression,
                conditions: conditions.clone(),
                cooldown_minutes: 0,
                channels: vec![],
            },
        )
        .await;
        assert!(created.is_err(), "create must reject {conditions}");

        let updated = AlertService::update_rule(
            &setup.db.pool,
            setup.rule.id,
            UpdateAlertRule {
                name: None,
                is_enabled: None,
                conditions: Some(conditions.clone()),
                cooldown_minutes: None,
                channels: None,
            },
        )
        .await;
        assert!(updated.is_err(), "update must reject {conditions}");
    }
    let unchanged = AlertService::get_rule(&setup.db.pool, setup.rule.id)
        .await
        .unwrap();
    assert_eq!(unchanged.conditions, json!({}));
}
