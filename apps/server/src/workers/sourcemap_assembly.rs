use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::source_file::{AssemblyJob, DIF_JOB_KIND};
use crate::services::debug_file::assemble_dif_for_job;
use crate::services::sourcemap::assemble_bundle_for_job;
use crate::services::sourcemap_store::SourceMapStore;

pub struct AssemblyWorker {
    pool: DbPool,
    store: Arc<dyn SourceMapStore>,
    worker_id: String,
    max_bundle_size_bytes: usize,
}

impl AssemblyWorker {
    pub fn new(pool: DbPool, store: Arc<dyn SourceMapStore>, max_bundle_size_bytes: usize) -> Self {
        Self {
            pool,
            store,
            worker_id: Uuid::new_v4().to_string(),
            max_bundle_size_bytes,
        }
    }

    /// Run the worker indefinitely.
    ///
    /// On startup: reset any `assembling` jobs whose `locked_until` is in the past
    /// (handles crash recovery). Then poll every 1 second for new `created` jobs.
    pub async fn run(self) {
        log::info!("Assembly worker {} starting up", self.worker_id);

        // Startup recovery: reset stuck assembling jobs
        if let Err(e) = self.reset_stuck_jobs().await {
            log::error!("Assembly worker: failed to reset stuck jobs: {:?}", e);
        }

        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if let Err(e) = self.run_once().await {
                log::error!("Assembly worker poll error: {:?}", e);
            }
        }
    }

    pub async fn run_once(&self) -> AppResult<()> {
        self.poll_once().await
    }

    /// Reset assembling jobs whose lock has expired (crash recovery).
    async fn reset_stuck_jobs(&self) -> Result<(), sqlx::Error> {
        #[cfg(feature = "postgres")]
        sqlx::query(
            r#"
            UPDATE assembly_jobs
            SET state = 'created', locked_until = NULL, worker_id = NULL, updated_at = NOW()
            WHERE state = 'assembling' AND locked_until < NOW()
            "#,
        )
        .execute(&self.pool)
        .await?;

        #[cfg(not(feature = "postgres"))]
        sqlx::query(
            r#"
            UPDATE assembly_jobs
            SET state = 'created', locked_until = NULL, worker_id = NULL,
                updated_at = datetime('now')
            WHERE state = 'assembling' AND locked_until < datetime('now')
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Claim and process one job, if available.
    async fn poll_once(&self) -> AppResult<()> {
        #[cfg(feature = "postgres")]
        let job: Option<AssemblyJob> = {
            sqlx::query_as(
                r#"
                WITH candidate AS (
                    SELECT id FROM assembly_jobs
                    WHERE state = 'created' AND retry_count < max_retries
                    ORDER BY created_at LIMIT 1
                    FOR UPDATE SKIP LOCKED
                )
                UPDATE assembly_jobs
                SET state = 'assembling',
                    worker_id = $1,
                    locked_until = NOW() + INTERVAL '2 minutes',
                    updated_at = NOW()
                FROM candidate
                WHERE assembly_jobs.id = candidate.id
                RETURNING assembly_jobs.*
                "#,
            )
            .bind(&self.worker_id)
            .fetch_optional(&self.pool)
            .await?
        };

        #[cfg(not(feature = "postgres"))]
        let job: Option<AssemblyJob> = {
            // SQLite: use mutex to serialize worker claims (single-worker mode).
            static SQLITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
            let _guard = SQLITE_LOCK.lock().await;

            let candidate: Option<AssemblyJob> = sqlx::query_as(
                r#"
                SELECT * FROM assembly_jobs
                WHERE state = 'created' AND retry_count < max_retries
                ORDER BY created_at LIMIT 1
                "#,
            )
            .fetch_optional(&self.pool)
            .await?;

            match candidate {
                None => None,
                Some(j) => {
                    sqlx::query(
                        r#"
                        UPDATE assembly_jobs
                        SET state = 'assembling',
                            worker_id = $1,
                            locked_until = datetime('now', '+2 minutes'),
                            updated_at = datetime('now')
                        WHERE id = $2
                        "#,
                    )
                    .bind(&self.worker_id)
                    .bind(j.id)
                    .execute(&self.pool)
                    .await?;

                    // Re-fetch to get the updated row
                    sqlx::query_as("SELECT * FROM assembly_jobs WHERE id = $1")
                        .bind(j.id)
                        .fetch_optional(&self.pool)
                        .await?
                }
            }
        };

        let job = match job {
            Some(j) => j,
            None => return Ok(()),
        };

        log::info!(
            "Assembly worker {}: processing job {} (bundle {})",
            self.worker_id,
            job.id,
            job.bundle_checksum
        );

        let result = if job.kind == DIF_JOB_KIND {
            assemble_dif_for_job(
                &self.pool,
                self.store.as_ref(),
                &job,
                self.max_bundle_size_bytes,
            )
            .await
        } else {
            assemble_bundle_for_job(
                &self.pool,
                self.store.as_ref(),
                job.project_id,
                &job.bundle_checksum,
                job.chunk_list(),
                self.max_bundle_size_bytes,
                job.id,
            )
            .await
        };

        match result {
            Ok(()) => {
                log::info!("Assembly worker: job {} completed successfully", job.id);
            }
            Err(e) => {
                let detail = format!("{:?}", e);
                log::error!("Assembly worker: job {} failed: {}", job.id, detail);
                self.mark_failure(job.id, &detail).await?;
            }
        }

        Ok(())
    }

    async fn mark_failure(&self, job_id: i64, detail: &str) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        #[cfg(feature = "postgres")]
        sqlx::query(
            r#"
            UPDATE assembly_jobs
            SET state = CASE WHEN retry_count + 1 >= max_retries THEN 'error' ELSE 'created' END,
                retry_count = retry_count + 1,
                detail = $2,
                locked_until = NULL,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(detail)
        .execute(&mut *tx)
        .await?;

        #[cfg(not(feature = "postgres"))]
        sqlx::query(
            r#"
            UPDATE assembly_jobs
            SET state = CASE WHEN retry_count + 1 >= max_retries THEN 'error' ELSE 'created' END,
                retry_count = retry_count + 1,
                detail = $2,
                locked_until = NULL,
                updated_at = datetime('now')
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(detail)
        .execute(&mut *tx)
        .await?;

        let (retry_count, max_retries): (i32, i32) =
            sqlx::query_as("SELECT retry_count, max_retries FROM assembly_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(&mut *tx)
                .await?;
        if retry_count >= max_retries {
            let owned_checksums: Vec<String> =
                sqlx::query_scalar("SELECT checksum FROM assembly_job_chunks WHERE job_id = $1")
                    .bind(job_id)
                    .fetch_all(&mut *tx)
                    .await?;
            sqlx::query("DELETE FROM assembly_job_chunks WHERE job_id = $1")
                .bind(job_id)
                .execute(&mut *tx)
                .await?;
            for checksum in owned_checksums {
                sqlx::query(
                    "DELETE FROM chunk WHERE checksum = $1 AND NOT EXISTS (SELECT 1 FROM assembly_job_chunks WHERE checksum = $1)",
                )
                .bind(checksum)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await
    }
}
