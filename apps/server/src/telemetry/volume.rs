//! How much the installation holds. Nine counts, each blurred before it
//! leaves, run once per window off the request path.

use crate::db::DbPool;
use crate::error::AppResult;

use super::{blur_count, Plugins, Volume};

impl Volume {
    pub async fn collect(pool: &DbPool) -> AppResult<Self> {
        Ok(Self {
            projects: count(
                pool,
                "SELECT COUNT(*) FROM projects WHERE deleted_at IS NULL",
            )
            .await?,
            users: count(pool, "SELECT COUNT(*) FROM users").await?,
            issues_open: count(
                pool,
                "SELECT COUNT(*) FROM issues WHERE status = 'unresolved'",
            )
            .await?,
            events_24h: count(pool, &last_day("events", "ingested_at")).await?,
            transactions_24h: count(pool, &last_day("transactions", "timestamp")).await?,
            sessions_24h: count(
                pool,
                &format!(
                    "SELECT CAST(COALESCE(SUM(total), 0) AS BIGINT) FROM session_counts WHERE {}",
                    since_last_day("bucket")
                ),
            )
            .await?,
            logs_24h: count(pool, &last_day("logs", "ingested_at")).await?,
            github_projects: count(pool, "SELECT COUNT(*) FROM project_repositories").await?,
            github_auto_create_projects: count(
                pool,
                "SELECT COUNT(*) FROM project_repositories WHERE auto_create_issues = TRUE",
            )
            .await?,
        })
    }
}

impl Plugins {
    pub async fn collect(pool: &DbPool) -> AppResult<Self> {
        let enabled = "SELECT COUNT(*) FROM plugins WHERE state = 'enabled'";
        let registry: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, version FROM plugins WHERE state = 'enabled' AND source = 'registry' ORDER BY id",
        )
        .fetch_all(pool)
        .await?;
        Ok(Self {
            installed: count(pool, enabled).await?,
            from_registry: count(pool, &format!("{enabled} AND source = 'registry'")).await?,
            from_upload: count(pool, &format!("{enabled} AND source = 'upload'")).await?,
            all_projects: count(pool, &format!("{enabled} AND all_projects = TRUE")).await?,
            project_activations: count(
                pool,
                "SELECT COUNT(*) FROM plugin_projects pp JOIN plugins p ON p.id = pp.plugin_id \
                 WHERE p.state = 'enabled'",
            )
            .await?,
            uninstalled: count(
                pool,
                "SELECT COUNT(*) FROM plugins WHERE state = 'uninstalled'",
            )
            .await?,
            registry: registry
                .into_iter()
                .map(|(id, version)| format!("{id}@{version}"))
                .collect(),
        })
    }
}

async fn count(pool: &DbPool, sql: &str) -> AppResult<u64> {
    // Every string here is assembled from literals above; nothing user-supplied.
    let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await?;
    Ok(blur_count(n.max(0) as u64))
}

fn last_day(table: &str, column: &str) -> String {
    format!(
        "SELECT COUNT(*) FROM {table} WHERE {}",
        since_last_day(column)
    )
}

/// In the last 24 hours, as SQL.
fn since_last_day(column: &str) -> String {
    crate::db::dialect::since(column, Some(24))
}

/// The kinds of alert channel switched on, sorted and deduplicated.
pub async fn alert_providers(pool: &DbPool) -> AppResult<Vec<String>> {
    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT provider_type FROM alert_integrations WHERE is_enabled = TRUE ORDER BY provider_type",
    )
    .fetch_all(pool)
    .await?;
    Ok(kinds)
}

/// Whether a GitHub App is connected to this server.
pub async fn github_connected(pool: &DbPool) -> AppResult<bool> {
    let apps: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM github_app")
        .fetch_one(pool)
        .await?;
    Ok(apps > 0)
}
