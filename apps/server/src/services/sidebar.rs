use sqlx::types::Json;

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::sidebar::{SidebarProjects, SidebarSort};

/// A saved row: the sort as stored, then the order and the hidden ids.
type SidebarRow = (String, Json<Vec<i32>>, Json<Vec<i32>>);

pub struct SidebarService;

impl SidebarService {
    /// The reader's saved sidebar, or the default when they saved none.
    pub async fn projects(pool: &DbPool, user_id: i32) -> AppResult<SidebarProjects> {
        let row: Option<SidebarRow> = sqlx::query_as(
            "SELECT sort, project_order, hidden_projects \
             FROM user_sidebar_projects WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
        Ok(
            row.map_or_else(SidebarProjects::default, |(sort, order, hidden)| {
                SidebarProjects {
                    sort: SidebarSort::parse(&sort),
                    order: order.0,
                    hidden: hidden.0,
                }
            }),
        )
    }

    /// Replaces the reader's sidebar.
    pub async fn save_projects(
        pool: &DbPool,
        user_id: i32,
        sidebar: &SidebarProjects,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO user_sidebar_projects (user_id, sort, project_order, hidden_projects) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (user_id) DO UPDATE SET sort = EXCLUDED.sort, \
             project_order = EXCLUDED.project_order, hidden_projects = EXCLUDED.hidden_projects",
        )
        .bind(user_id)
        .bind(sidebar.sort.as_str())
        .bind(Json(&sidebar.order))
        .bind(Json(&sidebar.hidden))
        .execute(pool)
        .await?;
        Ok(())
    }
}
