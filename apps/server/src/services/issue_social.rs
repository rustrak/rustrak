use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::AppResult;

/// An entry in an issue's activity log. Comments are stored as activity rows of
/// type `note` with `{"text": ...}` in `data` (Sentry models notes as activity).
#[derive(Debug, Clone, Serialize, FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActivityEntry {
    pub id: Uuid,
    pub issue_id: Uuid,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub user_id: Option<i32>,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub activity_type: String,
    pub data: String,
    pub created_at: DateTime<Utc>,
}

/// A user feedback report attached to a project/issue/event.
#[derive(Debug, Clone, Serialize, FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserReport {
    pub id: Uuid,
    pub project_id: i32,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub issue_id: Option<Uuid>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub event_id: Option<Uuid>,
    pub name: String,
    pub email: String,
    pub comments: String,
    pub created_at: DateTime<Utc>,
}

pub struct IssueSocialService;

impl IssueSocialService {
    /// Records an activity entry. `data` is a serialized JSON string.
    pub async fn add_activity(
        pool: &DbPool,
        issue_id: Uuid,
        user_id: Option<i32>,
        activity_type: &str,
        data: &str,
    ) -> AppResult<ActivityEntry> {
        let id = Uuid::new_v4();
        // Set created_at in-app for sub-second precision so the activity log has
        // a reliable chronological order (SQLite's datetime('now') default is
        // only second-granular and would tie entries created in the same second).
        let entry = sqlx::query_as::<_, ActivityEntry>(
            r#"
            INSERT INTO issue_activity (id, issue_id, user_id, type, data, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, issue_id, user_id, type, data, created_at
            "#,
        )
        .bind(id)
        .bind(issue_id)
        .bind(user_id)
        .bind(activity_type)
        .bind(data)
        .bind(Utc::now())
        .fetch_one(pool)
        .await?;
        Ok(entry)
    }

    /// Records an activity entry on the caller's connection, so it can share a
    /// transaction with the change it describes.
    pub async fn add_activity_on(
        executor: &mut <crate::db::Db as sqlx::Database>::Connection,
        issue_id: Uuid,
        user_id: Option<i32>,
        activity_type: &str,
        data: &str,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO issue_activity (id, issue_id, user_id, type, data, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(issue_id)
        .bind(user_id)
        .bind(activity_type)
        .bind(data)
        .bind(Utc::now())
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn record_status_change(
        pool: &DbPool,
        issue_id: Uuid,
        user_id: Option<i32>,
        status: &str,
    ) -> AppResult<()> {
        Self::record_status_change_on(&mut *pool.acquire().await?, issue_id, user_id, status).await
    }

    /// [`Self::record_status_change`] on the caller's connection.
    pub async fn record_status_change_on(
        executor: &mut <crate::db::Db as sqlx::Database>::Connection,
        issue_id: Uuid,
        user_id: Option<i32>,
        status: &str,
    ) -> AppResult<()> {
        let data = serde_json::json!({ "status": status }).to_string();
        Self::add_activity_on(executor, issue_id, user_id, "set_status", &data).await
    }

    /// Adds a comment (note) to an issue.
    pub async fn add_comment(
        pool: &DbPool,
        issue_id: Uuid,
        user_id: Option<i32>,
        text: &str,
    ) -> AppResult<ActivityEntry> {
        let data = serde_json::json!({ "text": text }).to_string();
        Self::add_activity(pool, issue_id, user_id, "note", &data).await
    }

    /// Lists an issue's activity, newest first.
    pub async fn list_activity(pool: &DbPool, issue_id: Uuid) -> AppResult<Vec<ActivityEntry>> {
        let rows = sqlx::query_as::<_, ActivityEntry>(
            r#"
            SELECT id, issue_id, user_id, type, data, created_at
            FROM issue_activity
            WHERE issue_id = $1
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .bind(issue_id)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// Adds or removes a bookmark for a user.
    pub async fn set_bookmark(
        pool: &DbPool,
        issue_id: Uuid,
        user_id: i32,
        bookmarked: bool,
    ) -> AppResult<()> {
        if bookmarked {
            sqlx::query(
                r#"
                INSERT INTO issue_bookmarks (issue_id, user_id) VALUES ($1, $2)
                ON CONFLICT (issue_id, user_id) DO NOTHING
                "#,
            )
            .bind(issue_id)
            .bind(user_id)
            .execute(pool)
            .await?;
        } else {
            sqlx::query("DELETE FROM issue_bookmarks WHERE issue_id = $1 AND user_id = $2")
                .bind(issue_id)
                .bind(user_id)
                .execute(pool)
                .await?;
        }
        Ok(())
    }

    pub async fn is_bookmarked(pool: &DbPool, issue_id: Uuid, user_id: i32) -> AppResult<bool> {
        let row: Option<(i32,)> =
            sqlx::query_as("SELECT 1 FROM issue_bookmarks WHERE issue_id = $1 AND user_id = $2")
                .bind(issue_id)
                .bind(user_id)
                .fetch_optional(pool)
                .await?;
        Ok(row.is_some())
    }

    /// Subscribes or unsubscribes a user from an issue.
    pub async fn set_subscription(
        pool: &DbPool,
        issue_id: Uuid,
        user_id: i32,
        subscribed: bool,
        reason: &str,
    ) -> AppResult<()> {
        if subscribed {
            sqlx::query(
                r#"
                INSERT INTO issue_subscriptions (issue_id, user_id, reason) VALUES ($1, $2, $3)
                ON CONFLICT (issue_id, user_id) DO UPDATE SET reason = EXCLUDED.reason
                "#,
            )
            .bind(issue_id)
            .bind(user_id)
            .bind(reason)
            .execute(pool)
            .await?;
        } else {
            sqlx::query("DELETE FROM issue_subscriptions WHERE issue_id = $1 AND user_id = $2")
                .bind(issue_id)
                .bind(user_id)
                .execute(pool)
                .await?;
        }
        Ok(())
    }

    pub async fn is_subscribed(pool: &DbPool, issue_id: Uuid, user_id: i32) -> AppResult<bool> {
        let row: Option<(i32,)> = sqlx::query_as(
            "SELECT 1 FROM issue_subscriptions WHERE issue_id = $1 AND user_id = $2",
        )
        .bind(issue_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.is_some())
    }

    /// Marks an issue as seen by a user (upsert last_seen_at = now).
    pub async fn mark_seen(pool: &DbPool, issue_id: Uuid, user_id: i32) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO issue_seen (issue_id, user_id, last_seen_at) VALUES ($1, $2, $3)
            ON CONFLICT (issue_id, user_id) DO UPDATE SET last_seen_at = EXCLUDED.last_seen_at
            "#,
        )
        .bind(issue_id)
        .bind(user_id)
        .bind(Utc::now())
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Whether the user has seen the issue since its last event.
    pub async fn has_seen(pool: &DbPool, issue_id: Uuid, user_id: i32) -> AppResult<bool> {
        let row: Option<(DateTime<Utc>,)> = sqlx::query_as(
            r#"
            SELECT s.last_seen_at
            FROM issue_seen s
            JOIN issues i ON i.id = s.issue_id
            WHERE s.issue_id = $1 AND s.user_id = $2 AND s.last_seen_at >= i.last_seen
            "#,
        )
        .bind(issue_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
        Ok(row.is_some())
    }

    /// Creates a user feedback report.
    pub async fn create_user_report(
        pool: &DbPool,
        project_id: i32,
        issue_id: Option<Uuid>,
        event_id: Option<Uuid>,
        name: &str,
        email: &str,
        comments: &str,
    ) -> AppResult<UserReport> {
        let id = Uuid::new_v4();
        let report = sqlx::query_as::<_, UserReport>(
            r#"
            INSERT INTO user_reports (id, project_id, issue_id, event_id, name, email, comments, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, project_id, issue_id, event_id, name, email, comments, created_at
            "#,
        )
        .bind(id)
        .bind(project_id)
        .bind(issue_id)
        .bind(event_id)
        .bind(name)
        .bind(email)
        .bind(comments)
        .bind(Utc::now())
        .fetch_one(pool)
        .await?;
        Ok(report)
    }

    /// Lists user reports for an issue.
    pub async fn list_user_reports(pool: &DbPool, issue_id: Uuid) -> AppResult<Vec<UserReport>> {
        let rows = sqlx::query_as::<_, UserReport>(
            r#"
            SELECT id, project_id, issue_id, event_id, name, email, comments, created_at
            FROM user_reports
            WHERE issue_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(issue_id)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// One page of an issue's user reports, newest first.
    pub async fn list_user_reports_page(
        pool: &DbPool,
        issue_id: Uuid,
        page: i64,
        per_page: i64,
    ) -> AppResult<Vec<UserReport>> {
        let rows = sqlx::query_as::<_, UserReport>(
            r#"
            SELECT id, project_id, issue_id, event_id, name, email, comments, created_at
            FROM user_reports
            WHERE issue_id = $1
            ORDER BY created_at DESC, id
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(issue_id)
        .bind(per_page)
        .bind(crate::pagination::offset(page, per_page))
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// Counts user reports for an issue.
    pub async fn user_report_count(pool: &DbPool, issue_id: Uuid) -> AppResult<i64> {
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_reports WHERE issue_id = $1")
                .bind(issue_id)
                .fetch_one(pool)
                .await?;
        Ok(count)
    }
}
