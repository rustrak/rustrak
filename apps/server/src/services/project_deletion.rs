//! Removing the projects marked deleted ([`ProjectService::delete`]).
//!
//! The mark is the job: a project with `deleted_at` set is one to remove, so
//! a run cut short by a restart or a failure is finished by the next one,
//! with nothing else to keep. Sentry keeps a `ScheduledDeletion` row and a
//! cron for the same end.

use std::sync::Arc;
use std::time::Duration;

use crate::db::DbPool;
use crate::error::AppResult;
use crate::services::StorageService;

/// How long a run that failed waits before it tries again: a database that
/// is down, or a table a compaction holds locked.
const RETRY_AFTER: Duration = Duration::from_secs(60);

/// The one run at a time that removes deleted projects.
#[derive(Default, Clone)]
pub struct ProjectDeletions {
    running: Arc<tokio::sync::Mutex<()>>,
}

impl ProjectDeletions {
    /// Removes the deleted projects in the background, trying again until a
    /// run gets through. Called when a project is deleted and at startup.
    pub fn start(&self, pool: DbPool) {
        let deletions = self.clone();
        tokio::spawn(async move {
            while let Err(error) = deletions.drain(&pool).await {
                log::error!("Removing deleted projects failed, will retry: {error}");
                tokio::time::sleep(RETRY_AFTER).await;
            }
        });
    }

    /// Removes every deleted project, oldest deletion first, and returns how
    /// many. Waits for a run already going, then removes what that one left:
    /// a project deleted while it finished is never missed.
    ///
    /// The rows that grow with traffic go first, in the cleanup's batches
    /// ([`StorageService::purge_project`]), each its own short transaction,
    /// so ingestion for other projects goes on between them. The project row
    /// goes last and takes the rest with it.
    pub async fn drain(&self, pool: &DbPool) -> AppResult<usize> {
        let _running = self.running.lock().await;
        let mut removed = 0;
        loop {
            let next: Option<i32> = sqlx::query_scalar(
                "SELECT id FROM projects WHERE deleted_at IS NOT NULL ORDER BY deleted_at LIMIT 1",
            )
            .fetch_optional(pool)
            .await?;
            let Some(project_id) = next else {
                return Ok(removed);
            };
            let counts = StorageService::purge_project(pool, project_id).await?;
            sqlx::query("DELETE FROM projects WHERE id = $1 AND deleted_at IS NOT NULL")
                .bind(project_id)
                .execute(pool)
                .await?;
            log::info!(
                "Removed project {project_id}: {} events, {} transactions, {} spans, {} logs",
                counts.events,
                counts.transactions,
                counts.spans,
                counts.logs
            );
            removed += 1;
        }
    }
}
