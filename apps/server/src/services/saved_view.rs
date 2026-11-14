use chrono::Utc;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::saved_view::{CreateSavedView, SavedView, UpdateSavedView};

pub struct SavedViewService;

impl SavedViewService {
    /// A page's views `user_id` sees: their own and the shared ones. `None` is
    /// the workspace's, for pages that span every project.
    pub async fn list(
        pool: &DbPool,
        project_id: Option<i32>,
        page: &str,
        user_id: Option<i32>,
    ) -> AppResult<Vec<SavedView>> {
        Ok(sqlx::query_as(
            "SELECT v.*, d.view_id IS NOT NULL AS is_my_default
             FROM saved_views v
             LEFT JOIN saved_view_defaults d ON d.view_id = v.id AND d.user_id = $3
             WHERE v.project_id IS NOT DISTINCT FROM $1 AND v.page = $2
               AND (v.visibility = 'project' OR v.created_by = $3)
             ORDER BY v.id",
        )
        .bind(project_id)
        .bind(page)
        .bind(user_id)
        .fetch_all(pool)
        .await?)
    }

    pub async fn create(
        pool: &DbPool,
        project_id: Option<i32>,
        user_id: i32,
        view: CreateSavedView,
    ) -> AppResult<SavedView> {
        Ok(sqlx::query_as(
            "INSERT INTO saved_views
                (project_id, created_by, page, name, query, visibility, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING *",
        )
        .bind(project_id)
        .bind(user_id)
        .bind(view.page)
        .bind(view.name)
        .bind(view.query)
        .bind(view.visibility.as_str())
        .bind(Utc::now())
        .fetch_one(pool)
        .await?)
    }

    /// One view `user_id` sees, or a 404: someone else's personal view does
    /// not exist for them.
    pub async fn get_visible(
        pool: &DbPool,
        project_id: Option<i32>,
        view_id: i32,
        user_id: Option<i32>,
    ) -> AppResult<SavedView> {
        sqlx::query_as(
            "SELECT * FROM saved_views
             WHERE id = $1 AND project_id IS NOT DISTINCT FROM $2
               AND (visibility = 'project' OR created_by = $3)",
        )
        .bind(view_id)
        .bind(project_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("View {view_id} not found")))
    }

    /// Writes the fields `changes` holds and keeps the rest as stored.
    pub async fn update(
        pool: &DbPool,
        view_id: i32,
        changes: UpdateSavedView,
    ) -> AppResult<SavedView> {
        Ok(sqlx::query_as(
            // A view no longer shared stops being the project's default.
            "UPDATE saved_views SET
                 name = COALESCE($2, name),
                 query = COALESCE($3, query),
                 visibility = COALESCE($4, visibility),
                 is_project_default = is_project_default
                     AND COALESCE($4, visibility) = 'project'
             WHERE id = $1 RETURNING *",
        )
        .bind(view_id)
        .bind(changes.name)
        .bind(changes.query)
        .bind(changes.visibility.map(|v| v.as_str()))
        .fetch_one(pool)
        .await?)
    }

    pub async fn delete(pool: &DbPool, view_id: i32) -> AppResult<()> {
        sqlx::query("DELETE FROM saved_views WHERE id = $1")
            .bind(view_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Sets or clears (`view_id` `None`) `user_id`'s default for a page.
    pub async fn set_user_default(
        pool: &DbPool,
        user_id: i32,
        project_id: Option<i32>,
        page: &str,
        view_id: Option<i32>,
    ) -> AppResult<()> {
        // Write-first: a plain `begin`, not `begin_write`. SQLite already
        // runs one writer at a time; on PostgreSQL two concurrent picks would
        // each delete nothing and both insert, so they queue on the user's row.
        let mut tx = pool.begin().await?;
        #[cfg(feature = "postgres")]
        sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM saved_view_defaults
             WHERE user_id = $1 AND view_id IN (
                 SELECT id FROM saved_views
                 WHERE project_id IS NOT DISTINCT FROM $2 AND page = $3)",
        )
        .bind(user_id)
        .bind(project_id)
        .bind(page)
        .execute(&mut *tx)
        .await?;
        if let Some(view_id) = view_id {
            sqlx::query("INSERT INTO saved_view_defaults (user_id, view_id) VALUES ($1, $2)")
                .bind(user_id)
                .bind(view_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Sets or clears (`view_id` `None`) the project's default for a page.
    pub async fn set_project_default(
        pool: &DbPool,
        project_id: Option<i32>,
        page: &str,
        view_id: Option<i32>,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE saved_views
             SET is_project_default = COALESCE(id = $3 AND visibility = 'project', FALSE)
             WHERE project_id IS NOT DISTINCT FROM $1 AND page = $2",
        )
        .bind(project_id)
        .bind(page)
        .bind(view_id)
        .execute(pool)
        .await?;
        Ok(())
    }
}
