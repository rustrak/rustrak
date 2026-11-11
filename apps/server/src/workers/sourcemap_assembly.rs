use std::sync::Arc;

use tokio::sync::Notify;

use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::source_file::{AssemblyJob, DIF_JOB_KIND};
use crate::services::debug_file::assemble_dif_for_job;
use crate::services::sourcemap::{assemble_bundle_for_job, job_chunks, release_job_chunks};
use crate::services::sourcemap_store::SourceMapStore;

/// Wakes the [`AssemblyWorker`] when a job is queued.
#[derive(Default)]
pub struct AssemblyQueue(Notify);

impl AssemblyQueue {
    pub fn wake(&self) {
        self.0.notify_one();
    }
}

/// Assembles queued jobs one at a time. There is one per process, so a job
/// left `assembling` was abandoned by a previous run and is taken again.
pub struct AssemblyWorker {
    pool: DbPool,
    store: Arc<dyn SourceMapStore>,
    max_bundle_size_bytes: usize,
}

impl AssemblyWorker {
    pub fn new(pool: DbPool, store: Arc<dyn SourceMapStore>, max_bundle_size_bytes: usize) -> Self {
        Self {
            pool,
            store,
            max_bundle_size_bytes,
        }
    }

    /// Assembles what is queued, then again each time `queue` is woken.
    pub async fn run(self, queue: Arc<AssemblyQueue>) {
        loop {
            loop {
                match self.run_once().await {
                    Ok(true) => {}
                    Ok(false) => break,
                    // ponytail: the job waits for the next wake or restart;
                    // a retry timer if a database outage ever strands one.
                    Err(e) => {
                        log::error!("Assembly worker: {e:?}");
                        break;
                    }
                }
            }
            queue.0.notified().await;
        }
    }

    /// Assembles the oldest queued job. `false` when there was none.
    pub async fn run_once(&self) -> AppResult<bool> {
        let job: Option<AssemblyJob> = sqlx::query_as(
            r#"
            UPDATE assembly_jobs SET state = 'assembling', updated_at = CURRENT_TIMESTAMP
            WHERE id = (
                SELECT id FROM assembly_jobs
                WHERE state IN ('created', 'assembling') AND retry_count < max_retries
                ORDER BY created_at, id LIMIT 1
            )
            RETURNING *
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some(job) = job else {
            return Ok(false);
        };

        log::info!(
            "Assembly worker: processing job {} (bundle {})",
            job.id,
            job.bundle_checksum
        );
        let chunks = job_chunks(&self.pool, job.id).await?;
        let store = self.store.as_ref();
        let result = if job.kind == DIF_JOB_KIND {
            assemble_dif_for_job(&self.pool, store, &job, &chunks, self.max_bundle_size_bytes).await
        } else {
            assemble_bundle_for_job(
                &self.pool,
                store,
                job.project_id,
                &job.bundle_checksum,
                &chunks,
                self.max_bundle_size_bytes,
                job.id,
            )
            .await
        };

        match result {
            Ok(()) => log::info!("Assembly worker: job {} completed successfully", job.id),
            Err(e) => {
                let detail = format!("{e:?}");
                log::error!("Assembly worker: job {} failed: {}", job.id, detail);
                self.mark_failure(job.id, &detail).await?;
            }
        }
        Ok(true)
    }

    /// Queues the job again, or after its last retry fails it and lets its
    /// chunks go.
    async fn mark_failure(&self, job_id: i64, detail: &str) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        let failed: bool = sqlx::query_scalar(
            r#"
            UPDATE assembly_jobs
            SET state = CASE WHEN retry_count + 1 >= max_retries THEN 'error' ELSE 'created' END,
                retry_count = retry_count + 1,
                detail = $2,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING state = 'error'
            "#,
        )
        .bind(job_id)
        .bind(detail)
        .fetch_one(&mut *tx)
        .await?;
        if failed {
            release_job_chunks(&mut tx, job_id).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
