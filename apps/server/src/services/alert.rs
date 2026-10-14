//! Alert service for managing integrations, rules, and dispatching alerts.
//!
//! This service handles:
//! - CRUD operations for alert integrations (global credentials)
//! - CRUD operations for alert rules (per-project)
//! - Alert triggering and dispatching

use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::{
    AlertConditions, AlertHistory, AlertIntegration, AlertPayload, AlertRule, AlertRuleChannel,
    AlertRuleChannelInput, AlertType, CreateAlertIntegration, CreateAlertRule, Issue, IssueInfo,
    Project, ProjectInfo, UpdateAlertIntegration, UpdateAlertRule,
};
use crate::services::notification::create_dispatcher;

fn event_alert_id(project_id: i32, event_id: Uuid, alert_type: &AlertType) -> String {
    format!("event-{project_id}-{event_id}-{alert_type}")
}

pub struct AlertService;

pub const MAX_ALERT_RETRIES: i32 = 5;

/// How long a claimed pending row stays invisible to other claimers. Must
/// exceed the longest single dispatch (30s webhook timeout plus DB writes) so
/// an in-flight delivery is never re-claimed and double-sent; a crashed
/// claimer's rows become eligible again once the lease expires.
const RETRY_CLAIM_LEASE_SECS: i64 = 120;

impl AlertService {
    // =========================================================================
    // Alert Integration CRUD (replaces Notification Channel CRUD)
    // =========================================================================

    /// Lists all alert integrations
    pub async fn list_channels(pool: &DbPool) -> AppResult<Vec<AlertIntegration>> {
        let integrations = sqlx::query_as::<_, AlertIntegration>(
            r#"
            SELECT id, name, provider_type, credentials, is_enabled, failure_count,
                   last_failure_at, last_failure_message, last_success_at,
                   created_at, updated_at
            FROM alert_integrations
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(pool)
        .await?;

        Ok(integrations)
    }

    /// Gets an alert integration by ID
    pub async fn get_channel(pool: &DbPool, id: i32) -> AppResult<AlertIntegration> {
        sqlx::query_as::<_, AlertIntegration>(
            r#"
            SELECT id, name, provider_type, credentials, is_enabled, failure_count,
                   last_failure_at, last_failure_message, last_success_at,
                   created_at, updated_at
            FROM alert_integrations
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Integration {} not found", id)))
    }

    /// Creates an alert integration
    pub async fn create_channel(
        pool: &DbPool,
        input: CreateAlertIntegration,
    ) -> AppResult<AlertIntegration> {
        // Validate credentials based on provider type
        let dispatcher = create_dispatcher(input.provider_type);
        dispatcher.validate_config(&input.credentials)?;

        let integration = sqlx::query_as::<_, AlertIntegration>(
            r#"
            INSERT INTO alert_integrations (name, provider_type, credentials, is_enabled)
            VALUES ($1, $2, $3, $4)
            RETURNING id, name, provider_type, credentials, is_enabled, failure_count,
                      last_failure_at, last_failure_message, last_success_at,
                      created_at, updated_at
            "#,
        )
        .bind(&input.name)
        .bind(input.provider_type.to_string())
        .bind(&input.credentials)
        .bind(input.is_enabled)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.is_unique_violation() {
                    return AppError::Conflict(format!(
                        "Integration '{}' already exists",
                        input.name
                    ))
                    .with_field("name", FieldErrorCode::AlreadyExists);
                }
            }
            AppError::Database(e)
        })?;

        Ok(integration)
    }

    /// Updates an alert integration
    pub async fn update_channel(
        pool: &DbPool,
        id: i32,
        input: UpdateAlertIntegration,
    ) -> AppResult<AlertIntegration> {
        let existing = Self::get_channel(pool, id).await?;

        // If credentials are being updated, validate them
        if let Some(ref credentials) = input.credentials {
            let dispatcher = create_dispatcher(existing.provider_type);
            dispatcher.validate_config(credentials)?;
        }

        let integration = sqlx::query_as::<_, AlertIntegration>(
            r#"
            UPDATE alert_integrations
            SET name = COALESCE($2, name),
                credentials = COALESCE($3, credentials),
                is_enabled = COALESCE($4, is_enabled),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id, name, provider_type, credentials, is_enabled, failure_count,
                      last_failure_at, last_failure_message, last_success_at,
                      created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(&input.name)
        .bind(&input.credentials)
        .bind(input.is_enabled)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.is_unique_violation() {
                    return AppError::Conflict("Integration name already exists".to_string())
                        .with_field("name", FieldErrorCode::AlreadyExists);
                }
            }
            AppError::Database(e)
        })?;

        Ok(integration)
    }

    /// Deletes an alert integration
    pub async fn delete_channel(pool: &DbPool, id: i32) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM alert_integrations WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Integration {} not found", id)));
        }

        Ok(())
    }

    // =========================================================================
    // Alert Rule CRUD
    // =========================================================================

    /// Lists alert rules for a project
    pub async fn list_rules(pool: &DbPool, project_id: i32) -> AppResult<Vec<AlertRule>> {
        let rules = sqlx::query_as::<_, AlertRule>(
            r#"
            SELECT id, project_id, name, alert_type, is_enabled, conditions,
                   cooldown_minutes, last_triggered_at, created_at, updated_at
            FROM alert_rules
            WHERE project_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;

        Ok(rules)
    }

    /// Gets an alert rule by ID
    pub async fn get_rule(pool: &DbPool, id: i32) -> AppResult<AlertRule> {
        sqlx::query_as::<_, AlertRule>(
            r#"
            SELECT id, project_id, name, alert_type, is_enabled, conditions,
                   cooldown_minutes, last_triggered_at, created_at, updated_at
            FROM alert_rules
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Alert rule {} not found", id)))
    }

    /// Gets all integration_ids linked to a rule (regardless of is_enabled).
    ///
    /// Returns all links so clients can round-trip without silently losing
    /// disabled integrations. Dispatch uses get_rule_channel_records which
    /// still filters by is_enabled.
    pub async fn get_rule_channels(pool: &DbPool, rule_id: i32) -> AppResult<Vec<i32>> {
        let rows: Vec<(i32,)> = sqlx::query_as(
            "SELECT integration_id FROM alert_rule_channels WHERE alert_rule_id = $1",
        )
        .bind(rule_id)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Gets full AlertRuleChannel records for dispatch (includes routing_override).
    ///
    /// Filters disabled integrations (SCL-1).
    pub async fn get_rule_channel_records(
        pool: &DbPool,
        rule_id: i32,
    ) -> AppResult<Vec<AlertRuleChannel>> {
        let channels: Vec<AlertRuleChannel> = sqlx::query_as(
            r#"
            SELECT arc.alert_rule_id, arc.integration_id, arc.routing_override
            FROM alert_rule_channels arc
            INNER JOIN alert_integrations i ON arc.integration_id = i.id
            WHERE arc.alert_rule_id = $1 AND i.is_enabled = TRUE
            "#,
        )
        .bind(rule_id)
        .fetch_all(pool)
        .await?;

        Ok(channels)
    }

    /// Gets all AlertRuleChannel records for a rule regardless of is_enabled.
    ///
    /// Used for API responses so clients can round-trip without losing links to
    /// disabled integrations. Dispatch uses get_rule_channel_records (SCL-1).
    pub async fn get_all_rule_channel_records(
        pool: &DbPool,
        rule_id: i32,
    ) -> AppResult<Vec<AlertRuleChannel>> {
        let channels: Vec<AlertRuleChannel> = sqlx::query_as(
            "SELECT alert_rule_id, integration_id, routing_override FROM alert_rule_channels WHERE alert_rule_id = $1",
        )
        .bind(rule_id)
        .fetch_all(pool)
        .await?;

        Ok(channels)
    }

    /// Creates an alert rule
    pub async fn create_rule(
        pool: &DbPool,
        project_id: i32,
        input: CreateAlertRule,
    ) -> AppResult<AlertRule> {
        AlertConditions::validate(&input.conditions)?;
        // Write-first (INSERT opens the tx) — deferred BEGIN deliberate, see db::begin_write.
        let mut tx = pool.begin().await?;

        let rule = sqlx::query_as::<_, AlertRule>(
            r#"
            INSERT INTO alert_rules (project_id, name, alert_type, conditions, cooldown_minutes)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, project_id, name, alert_type, is_enabled, conditions,
                      cooldown_minutes, last_triggered_at, created_at, updated_at
            "#,
        )
        .bind(project_id)
        .bind(&input.name)
        .bind(input.alert_type.to_string())
        .bind(&input.conditions)
        .bind(input.cooldown_minutes)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.is_unique_violation() {
                    return AppError::Conflict(format!(
                        "Alert rule for type '{}' already exists in this project",
                        input.alert_type
                    ))
                    .with_field("alert_type", FieldErrorCode::AlreadyExists);
                }
            }
            AppError::Database(e)
        })?;

        // Dedup by integration_id to prevent PK constraint violation (ECH-4).
        let channels_to_link: Vec<AlertRuleChannelInput> = {
            let mut seen = std::collections::HashSet::new();
            input
                .channels
                .clone()
                .into_iter()
                .filter(|ch| seen.insert(ch.integration_id))
                .collect()
        };

        for ch in &channels_to_link {
            sqlx::query(
                r#"
                INSERT INTO alert_rule_channels (alert_rule_id, integration_id, routing_override)
                VALUES ($1, $2, $3)
                "#,
            )
            .bind(rule.id)
            .bind(ch.integration_id)
            .bind(&ch.routing_override)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                if let sqlx::Error::Database(ref db_err) = e {
                    if db_err.is_foreign_key_violation() {
                        return AppError::NotFound(format!(
                            "Integration {} not found",
                            ch.integration_id
                        ));
                    }
                }
                AppError::Database(e)
            })?;
        }

        tx.commit().await?;

        Ok(rule)
    }

    /// Updates an alert rule
    pub async fn update_rule(
        pool: &DbPool,
        id: i32,
        input: UpdateAlertRule,
    ) -> AppResult<AlertRule> {
        if let Some(conditions) = &input.conditions {
            AlertConditions::validate(conditions)?;
        }
        // Write-first (UPDATE opens the tx) — deferred BEGIN deliberate, see db::begin_write.
        let mut tx = pool.begin().await?;

        let rule = sqlx::query_as::<_, AlertRule>(
            r#"
            UPDATE alert_rules
            SET name = COALESCE($2, name),
                is_enabled = COALESCE($3, is_enabled),
                conditions = COALESCE($4, conditions),
                cooldown_minutes = COALESCE($5, cooldown_minutes),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id, project_id, name, alert_type, is_enabled, conditions,
                      cooldown_minutes, last_triggered_at, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(&input.name)
        .bind(input.is_enabled)
        .bind(&input.conditions)
        .bind(input.cooldown_minutes)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Alert rule {} not found", id)))?;

        let channels_update = input.channels;

        if let Some(ref channels) = channels_update {
            // Remove existing links
            sqlx::query("DELETE FROM alert_rule_channels WHERE alert_rule_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;

            // Dedup by integration_id before inserting (ECH-4)
            let mut seen = std::collections::HashSet::new();
            let channels: Vec<_> = channels
                .iter()
                .filter(|ch| seen.insert(ch.integration_id))
                .collect();

            // Add new links with routing_override
            for ch in channels {
                sqlx::query(
                    r#"
                    INSERT INTO alert_rule_channels (alert_rule_id, integration_id, routing_override)
                    VALUES ($1, $2, $3)
                    "#,
                )
                .bind(id)
                .bind(ch.integration_id)
                .bind(&ch.routing_override)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    if let sqlx::Error::Database(ref db_err) = e {
                        if db_err.is_foreign_key_violation() {
                            return AppError::NotFound(format!(
                                "Integration {} not found",
                                ch.integration_id
                            ));
                        }
                    }
                    AppError::Database(e)
                })?;
            }
        }

        tx.commit().await?;

        Ok(rule)
    }

    /// Deletes an alert rule
    pub async fn delete_rule(pool: &DbPool, id: i32) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM alert_rules WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Alert rule {} not found", id)));
        }

        Ok(())
    }

    // =========================================================================
    // Alert Triggering
    // =========================================================================

    /// Triggers an alert for a new issue
    pub async fn trigger_new_issue_alert(
        pool: &DbPool,
        project: &Project,
        issue: &Issue,
        dashboard_url: &str,
    ) -> AppResult<()> {
        Self::trigger_alert(
            pool,
            project,
            issue,
            AlertType::NewIssue,
            dashboard_url,
            None,
        )
        .await
    }

    /// Triggers an alert for a regression
    pub async fn trigger_regression_alert(
        pool: &DbPool,
        project: &Project,
        issue: &Issue,
        dashboard_url: &str,
    ) -> AppResult<()> {
        Self::trigger_alert(
            pool,
            project,
            issue,
            AlertType::Regression,
            dashboard_url,
            None,
        )
        .await
    }

    /// Enqueues the alert for one durable event with a retry-stable identity.
    pub async fn trigger_event_alert(
        pool: &DbPool,
        project: &Project,
        issue: &Issue,
        alert_type: AlertType,
        event_id: Uuid,
        dashboard_url: &str,
    ) -> AppResult<()> {
        Self::trigger_alert(
            pool,
            project,
            issue,
            alert_type,
            dashboard_url,
            Some(event_alert_id(project.id, event_id, &alert_type)),
        )
        .await
    }

    /// Triggers an alert for an unmute
    #[allow(dead_code)]
    pub async fn trigger_unmute_alert(
        pool: &DbPool,
        project: &Project,
        issue: &Issue,
        dashboard_url: &str,
    ) -> AppResult<()> {
        Self::trigger_alert(pool, project, issue, AlertType::Unmute, dashboard_url, None).await
    }

    /// Builds the dashboard URL for viewing an issue.
    pub(crate) fn build_issue_url(
        dashboard_url: &str,
        project_id: i32,
        issue_id: uuid::Uuid,
    ) -> String {
        format!(
            "{}/projects/{}/issues/{}",
            dashboard_url, project_id, issue_id
        )
    }

    // Shared by the production transaction and deterministic database tests.
    async fn reserve_cooldown(
        tx: &mut sqlx::Transaction<'_, crate::db::Db>,
        rule: &AlertRule,
        now: chrono::DateTime<Utc>,
    ) -> AppResult<bool> {
        // The row owns both the zero bypass and expiry: an administrator may
        // have changed the cooldown since trigger_alert loaded the rule.
        #[cfg(feature = "postgres")]
        let updated = sqlx::query(
            r#"
            UPDATE alert_rules
            SET last_triggered_at = CURRENT_TIMESTAMP
            WHERE id = $1
              AND (cooldown_minutes <= 0 OR last_triggered_at IS NULL
                   OR last_triggered_at < $2::timestamptz - make_interval(mins => cooldown_minutes))
            "#,
        )
        .bind(rule.id)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        #[cfg(feature = "sqlite")]
        let updated = sqlx::query(
            r#"
            UPDATE alert_rules
            SET last_triggered_at = datetime('now')
            WHERE id = $1
              AND (cooldown_minutes <= 0 OR last_triggered_at IS NULL
                   OR datetime(last_triggered_at) < datetime($2, '-' || cooldown_minutes || ' minutes'))
            "#,
        )
        .bind(rule.id)
        .bind(now.naive_utc())
        .execute(&mut **tx)
        .await?;

        Ok(updated.rows_affected() != 0)
    }

    /// Runs the production reservation with a controlled clock and rule snapshot.
    #[cfg(feature = "test-seams")]
    #[doc(hidden)]
    pub async fn reserve_cooldown_at_for_test(
        pool: &DbPool,
        rule: &AlertRule,
        now: chrono::DateTime<Utc>,
    ) -> AppResult<bool> {
        let mut tx = pool.begin().await?;
        let reserved = Self::reserve_cooldown(&mut tx, rule, now).await?;
        tx.commit().await?;
        Ok(reserved)
    }

    /// Core alert triggering logic
    async fn trigger_alert(
        pool: &DbPool,
        project: &Project,
        issue: &Issue,
        alert_type: AlertType,
        dashboard_url: &str,
        stable_alert_id: Option<String>,
    ) -> AppResult<()> {
        // 1. Find enabled rule for this project and alert type
        let rule: Option<AlertRule> = sqlx::query_as(
            r#"
            SELECT id, project_id, name, alert_type, is_enabled, conditions,
                   cooldown_minutes, last_triggered_at, created_at, updated_at
            FROM alert_rules
            WHERE project_id = $1 AND alert_type = $2 AND is_enabled = TRUE
            "#,
        )
        .bind(project.id)
        .bind(alert_type.to_string())
        .fetch_optional(pool)
        .await?;

        let rule = match rule {
            Some(r) => r,
            None => {
                log::debug!(
                    "No enabled alert rule for {:?} in project {}",
                    alert_type,
                    project.id
                );
                return Ok(());
            }
        };

        // A rule can restrict itself to issues at or above a level. This runs before the
        // cooldown reservation on purpose: an issue the rule never considers must not start
        // a cooldown that would then suppress an alert it does send. Nothing is recorded in
        // the history for it either; the rule's own conditions explain the silence.
        if !AlertConditions::from_stored(&rule.conditions, rule.id).admits(issue.level.as_deref()) {
            log::debug!(
                "Alert rule {} does not alert on level {:?}",
                rule.id,
                issue.level
            );
            return Ok(());
        }

        // 2. Get associated rule channels (only enabled integrations — SCL-1)
        let rule_channels: Vec<AlertRuleChannel> = sqlx::query_as(
            r#"
            SELECT arc.alert_rule_id, arc.integration_id, arc.routing_override
            FROM alert_rule_channels arc
            INNER JOIN alert_integrations i ON arc.integration_id = i.id
            WHERE arc.alert_rule_id = $1 AND i.is_enabled = TRUE
            "#,
        )
        .bind(rule.id)
        .fetch_all(pool)
        .await?;

        if rule_channels.is_empty() {
            log::debug!("No enabled integrations for alert rule {}", rule.id);
            return Ok(());
        }

        let mut channels = Vec::with_capacity(rule_channels.len());
        for rule_channel in rule_channels {
            let integration = Self::get_channel(pool, rule_channel.integration_id).await?;
            channels.push((rule_channel, integration));
        }

        // 3. Build payload
        let payload = AlertPayload {
            alert_id: stable_alert_id.unwrap_or_else(|| {
                format!(
                    "{}-{}-{}",
                    project.id,
                    issue.id,
                    Utc::now().timestamp_millis()
                )
            }),
            alert_type: alert_type.to_string(),
            triggered_at: Utc::now(),
            project: ProjectInfo {
                id: project.id,
                name: project.name.clone(),
                slug: project.slug.clone(),
            },
            issue: IssueInfo {
                id: issue.id.to_string(),
                short_id: issue.short_id(&project.slug),
                title: issue.title(),
                level: issue.level.clone(),
                first_seen: issue.first_seen,
                last_seen: issue.last_seen,
                event_count: issue.digested_event_count,
            },
            issue_url: Self::build_issue_url(dashboard_url, project.id, issue.id),
            actor: "Rustrak".to_string(),
        };

        log::info!(
            "Triggering {} alert for issue {} in project {}",
            alert_type,
            issue.id,
            project.name
        );

        // 4. Reserve the cooldown and record every delivery atomically.
        // Zero disables suppression, including same-second SQLite timestamps or
        // a backward clock adjustment. Event idempotency still prevents replays.
        let mut tx = pool.begin().await?;
        let cooldown_suppressed = !Self::reserve_cooldown(&mut tx, &rule, Utc::now()).await?;
        if cooldown_suppressed {
            log::debug!("Alert rule {} not found or in cooldown period", rule.id);
        }
        let history_status = if cooldown_suppressed {
            "skipped"
        } else {
            "pending"
        };

        // Pending rows are born leased to the dispatch task spawned below:
        // the retry worker only claims rows whose next_retry_at has passed, so
        // it cannot double-send a delivery that is still in flight. Skipped
        // (cooldown) rows never dispatch and carry no lease.
        let dispatch_lease =
            (!cooldown_suppressed).then(|| Utc::now() + Duration::seconds(RETRY_CLAIM_LEASE_SECS));

        let mut history_ids = Vec::with_capacity(channels.len());
        for (rule_channel, integration) in &channels {
            let idempotency_key = format!("{}-{}", payload.alert_id, integration.id);
            let issue_uuid = Uuid::parse_str(&payload.issue.id).ok();
            let history_id: Option<(i64,)> = sqlx::query_as(
                r#"
                INSERT INTO alert_history (
                    alert_rule_id, integration_id, issue_id, project_id,
                    alert_type, channel_type, channel_name, payload,
                    status, idempotency_key, next_retry_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                ON CONFLICT (idempotency_key) DO NOTHING
                RETURNING id
                "#,
            )
            .bind(rule.id)
            .bind(rule_channel.integration_id)
            .bind(issue_uuid)
            .bind(payload.project.id)
            .bind(&payload.alert_type)
            .bind(integration.provider_type.to_string())
            .bind(&integration.name)
            .bind(serde_json::to_value(&payload).map_err(|e| AppError::Internal(e.to_string()))?)
            .bind(history_status)
            .bind(&idempotency_key)
            .bind(dispatch_lease)
            .fetch_optional(&mut *tx)
            .await?;

            if !cooldown_suppressed {
                if let Some(history_id) = history_id {
                    history_ids.push((history_id.0, rule_channel.clone()));
                }
            }
        }
        tx.commit().await?;

        if cooldown_suppressed {
            return Ok(());
        }

        // 5. Deliver off the caller's path. The committed pending rows are the
        // durability guarantee (they survive a crash and the retry worker picks
        // them up once the lease expires); the caller — the digest pipeline —
        // must not stall on notification I/O, matching Relay, which never
        // couples ingestion to delivery.
        let dispatch_pool = pool.clone();
        tokio::spawn(async move {
            for (history_id, rule_channel) in history_ids {
                if let Err(error) = Self::dispatch_history(
                    &dispatch_pool,
                    history_id,
                    0,
                    &payload,
                    &rule_channel,
                    MAX_ALERT_RETRIES,
                )
                .await
                {
                    log::error!(
                        "Alert dispatch for history {history_id} failed; the retry worker owns it after the lease expires: {error}"
                    );
                }
            }
        });

        Ok(())
    }

    /// Returns whether durable history already exists for an event alert.
    pub async fn event_alert_exists(
        pool: &DbPool,
        project_id: i32,
        event_id: Uuid,
        alert_type: AlertType,
    ) -> AppResult<bool> {
        let prefix = format!("event-{project_id}-{event_id}-{alert_type}-%");
        let legacy_prefix = format!("event-{event_id}-{alert_type}-%");
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM alert_history WHERE project_id = $1 AND (idempotency_key LIKE $2 OR idempotency_key LIKE $3))",
        )
        .bind(project_id)
        .bind(prefix)
        .bind(legacy_prefix)
        .fetch_one(pool)
        .await?;
        Ok(exists)
    }

    async fn dispatch_existing_history(
        pool: &DbPool,
        history: &AlertHistory,
        rule_channel: &AlertRuleChannel,
        max_retries: i32,
    ) -> AppResult<()> {
        let payload: AlertPayload = match serde_json::from_value(history.payload.clone()) {
            Ok(payload) => payload,
            Err(error) => {
                let message = format!("invalid alert payload: {error}");
                sqlx::query(
                    "UPDATE alert_history SET status = 'failed', next_retry_at = NULL, error_message = $2 WHERE id = $1",
                )
                .bind(history.id)
                .bind(message)
                .execute(pool)
                .await?;
                return Ok(());
            }
        };
        Self::dispatch_history(
            pool,
            history.id,
            history.attempt_count,
            &payload,
            rule_channel,
            max_retries,
        )
        .await
    }

    async fn dispatch_history(
        pool: &DbPool,
        history_id: i64,
        previous_attempt_count: i32,
        payload: &AlertPayload,
        rule_channel: &AlertRuleChannel,
        max_retries: i32,
    ) -> AppResult<()> {
        let integration = Self::get_channel(pool, rule_channel.integration_id).await?;
        let result = create_dispatcher(integration.provider_type)
            .send(&integration, &rule_channel.routing_override, payload)
            .await;
        let attempt_count = next_attempt_count(previous_attempt_count);

        if !result.success {
            crate::telemetry::Counters::global()
                .alert_failed(&integration.provider_type.to_string());
        }
        if result.success {
            sqlx::query(
                "UPDATE alert_history SET status = 'sent', sent_at = CURRENT_TIMESTAMP, http_status_code = $2 WHERE id = $1",
            )
            .bind(history_id)
            .bind(result.http_status.map(|s| s as i32))
            .execute(pool)
            .await?;
            sqlx::query(
                "UPDATE alert_integrations SET last_success_at = CURRENT_TIMESTAMP, failure_count = 0 WHERE id = $1",
            )
            .bind(integration.id)
            .execute(pool)
            .await?;
        } else if attempt_count >= max_retries {
            sqlx::query(
                "UPDATE alert_history SET status = 'failed', attempt_count = $2, error_message = $3, http_status_code = $4 WHERE id = $1",
            )
            .bind(history_id)
            .bind(attempt_count)
            .bind(&result.error_message)
            .bind(result.http_status.map(|s| s as i32))
            .execute(pool)
            .await?;
        } else {
            let delay_secs = retry_delay(attempt_count, history_id);
            let next_retry = Utc::now() + Duration::seconds(delay_secs);
            sqlx::query(
                "UPDATE alert_history SET status = 'pending', attempt_count = $2, error_message = $3, http_status_code = $4, next_retry_at = $5 WHERE id = $1",
            )
            .bind(history_id)
            .bind(attempt_count)
            .bind(&result.error_message)
            .bind(result.http_status.map(|s| s as i32))
            .bind(next_retry)
            .execute(pool)
            .await?;
        }

        if !result.success {
            sqlx::query(
                "UPDATE alert_integrations SET last_failure_at = CURRENT_TIMESTAMP, last_failure_message = $2, failure_count = failure_count + 1 WHERE id = $1",
            )
            .bind(integration.id)
            .bind(&result.error_message)
            .execute(pool)
            .await?;
        }

        Ok(())
    }

    // =========================================================================
    // Alert History
    // =========================================================================

    /// Lists alert history for a project
    pub async fn list_history(
        pool: &DbPool,
        project_id: i32,
        limit: i64,
    ) -> AppResult<Vec<AlertHistory>> {
        let history = sqlx::query_as::<_, AlertHistory>(
            r#"
            SELECT id, alert_rule_id, integration_id, issue_id, project_id,
                   alert_type, channel_type, channel_name, payload, status,
                   attempt_count, next_retry_at, error_message,
                   http_status_code, idempotency_key, created_at, sent_at
            FROM alert_history
            WHERE project_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(project_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        Ok(history)
    }

    /// Atomically leases due pending history rows for dispatch.
    ///
    /// Each returned row had its `next_retry_at` pushed into the future in the
    /// same statement that selected it, so a concurrent claimer (another worker
    /// tick, or another server process sharing the database) cannot pick up the
    /// same row and double-deliver the alert.
    pub async fn claim_due_retries(
        pool: &DbPool,
        max_retries: i32,
        limit: i64,
    ) -> AppResult<Vec<AlertHistory>> {
        #[cfg(feature = "postgres")]
        let claimed: Vec<AlertHistory> = sqlx::query_as(
            r#"
            UPDATE alert_history
            SET next_retry_at = CURRENT_TIMESTAMP + ($3 * INTERVAL '1 second')
            WHERE id IN (
                SELECT id FROM alert_history
                WHERE status = 'pending'
                  AND (next_retry_at IS NULL OR next_retry_at <= CURRENT_TIMESTAMP)
                  AND attempt_count < $1
                ORDER BY next_retry_at
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
              AND status = 'pending'
              AND (next_retry_at IS NULL OR next_retry_at <= CURRENT_TIMESTAMP)
            RETURNING id, alert_rule_id, integration_id, issue_id, project_id,
                      alert_type, channel_type, channel_name, payload, status,
                      attempt_count, next_retry_at, error_message,
                      http_status_code, idempotency_key, created_at, sent_at
            "#,
        )
        .bind(max_retries)
        .bind(limit)
        .bind(RETRY_CLAIM_LEASE_SECS)
        .fetch_all(pool)
        .await?;

        #[cfg(feature = "sqlite")]
        let claimed: Vec<AlertHistory> = sqlx::query_as(
            r#"
            UPDATE alert_history
            SET next_retry_at = datetime('now', '+' || $3 || ' seconds')
            WHERE id IN (
                SELECT id FROM alert_history
                WHERE status = 'pending'
                  AND (next_retry_at IS NULL OR datetime(next_retry_at) <= datetime('now'))
                  AND attempt_count < $1
                ORDER BY next_retry_at
                LIMIT $2
            )
            RETURNING id, alert_rule_id, integration_id, issue_id, project_id,
                      alert_type, channel_type, channel_name, payload, status,
                      attempt_count, next_retry_at, error_message,
                      http_status_code, idempotency_key, created_at, sent_at
            "#,
        )
        .bind(max_retries)
        .bind(limit)
        .bind(RETRY_CLAIM_LEASE_SECS)
        .fetch_all(pool)
        .await?;

        Ok(claimed)
    }

    /// Processes pending retries (for background worker)
    #[allow(dead_code)]
    pub async fn process_retry_queue(pool: &DbPool, max_retries: i32) -> AppResult<u32> {
        let pending = Self::claim_due_retries(pool, max_retries, 100).await?;

        let mut processed = 0u32;

        for history in pending {
            // Mark as failed if integration deleted
            if history.integration_id.is_none() {
                sqlx::query(
                    "UPDATE alert_history SET status = 'failed', error_message = 'Integration deleted' WHERE id = $1",
                )
                .bind(history.id)
                .execute(pool)
                .await?;
                processed += 1;
                continue;
            }

            let (Some(rule_id), Some(integration_id)) =
                (history.alert_rule_id, history.integration_id)
            else {
                sqlx::query(
                    "UPDATE alert_history SET status = 'failed', error_message = 'Alert rule deleted' WHERE id = $1",
                )
                .bind(history.id)
                .execute(pool)
                .await?;
                processed += 1;
                continue;
            };

            let channel: Option<AlertRuleChannel> = sqlx::query_as(
                "SELECT alert_rule_id, integration_id, routing_override FROM alert_rule_channels WHERE alert_rule_id = $1 AND integration_id = $2",
            )
            .bind(rule_id)
            .bind(integration_id)
            .fetch_optional(pool)
            .await?;
            let Some(channel) = channel else {
                sqlx::query(
                    "UPDATE alert_history SET status = 'failed', error_message = 'Alert channel deleted' WHERE id = $1",
                )
                .bind(history.id)
                .execute(pool)
                .await?;
                processed += 1;
                continue;
            };

            if let Err(e) =
                Self::dispatch_existing_history(pool, &history, &channel, max_retries).await
            {
                log::error!("Failed to retry alert history {}: {}", history.id, e);
            }

            processed += 1;
        }

        Ok(processed)
    }
}

fn retry_delay(attempt_count: i32, history_id: i64) -> i64 {
    let exponent = attempt_count.saturating_sub(1).clamp(0, 6) as u32;
    let base = 60 * 2_i64.pow(exponent);
    let jitter = history_id.rem_euclid(30);
    std::cmp::min(base + jitter, 3600)
}

fn next_attempt_count(previous: i32) -> i32 {
    previous.max(0).saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_issue_url_uses_numeric_project_id_not_slug() {
        let url = AlertService::build_issue_url("http://localhost:3000", 42, uuid::Uuid::nil());
        assert!(
            url.contains("/projects/42/"),
            "URL must use numeric project id, got: {}",
            url
        );
    }

    #[test]
    fn event_alert_id_is_stable_across_retries() {
        let event_id = Uuid::nil();
        assert_eq!(
            event_alert_id(1, event_id, &AlertType::NewIssue),
            event_alert_id(1, event_id, &AlertType::NewIssue)
        );
        assert_ne!(
            event_alert_id(1, event_id, &AlertType::NewIssue),
            event_alert_id(1, event_id, &AlertType::Regression)
        );
        assert_ne!(
            event_alert_id(1, event_id, &AlertType::NewIssue),
            event_alert_id(2, event_id, &AlertType::NewIssue)
        );
    }

    #[test]
    fn alert_payload_round_trips_for_retry_storage() {
        let stored = json!({
            "alert_id": "alert-1", "alert_type": "new_issue",
            "triggered_at": "2026-08-21T10:00:00Z",
            "project": {"id": 1, "name": "Project", "slug": "project"},
            "issue": {"id": Uuid::new_v4().to_string(), "short_id": "PROJECT-1",
                "title": "Issue", "level": "error", "first_seen": "2026-08-21T10:00:00Z",
                "last_seen": "2026-08-21T10:00:00Z", "event_count": 1},
            "issue_url": "http://localhost/issues/1", "actor": "Rustrak"
        });
        let payload: AlertPayload = serde_json::from_value(stored.clone()).unwrap();
        assert_eq!(serde_json::to_value(payload).unwrap(), stored);
    }

    #[test]
    fn retry_delay_is_bounded_for_large_attempt_counts() {
        assert_eq!(retry_delay(1, 0), 60);
        assert_eq!(retry_delay(1, 29), 89);
        assert_eq!(retry_delay(-1, 0), 60);
        assert_eq!(retry_delay(i32::MAX, i64::MAX), 3600);
    }

    #[test]
    fn next_attempt_count_is_bounded() {
        assert_eq!(next_attempt_count(-1), 1);
        assert_eq!(next_attempt_count(i32::MAX), i32::MAX);
    }
}
