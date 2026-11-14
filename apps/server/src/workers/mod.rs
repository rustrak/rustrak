pub mod sourcemap_assembly;

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Duration;

/// Background workers restarted after a panic since the process started.
static RESTARTS: AtomicU64 = AtomicU64::new(0);

/// Pause before a panicked worker runs again, so one that panics on every
/// start does not spin.
const RESTART_BACKOFF: Duration = Duration::from_secs(1);

/// How many times [`supervise`] has restarted a worker.
pub fn restarts() -> u64 {
    RESTARTS.load(AtomicOrdering::Relaxed)
}

/// Spawns the worker `start` builds and starts it again whenever it panics:
/// a bare `tokio::spawn` swallows the panic and the worker is gone for the
/// life of the process. A worker that returns is done. `start` is called once
/// per run, so each restart begins from a fresh future.
pub fn supervise<F, Fut>(name: &'static str, mut start: F) -> tokio::task::JoinHandle<()>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match tokio::spawn(start()).await {
                Err(e) if e.is_panic() => {
                    RESTARTS.fetch_add(1, AtomicOrdering::Relaxed);
                    log::error!("Background worker '{name}' panicked; restarting it");
                    tokio::time::sleep(RESTART_BACKOFF).await;
                }
                // Returned, or cancelled because the runtime is shutting down.
                _ => return,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A worker that panics is started again and the restart is counted; one
    /// that returns is done.
    #[tokio::test]
    async fn a_panicking_worker_is_restarted_and_counted() {
        let runs = Arc::new(AtomicUsize::new(0));
        let before = restarts();

        let handle = supervise("test worker", {
            let runs = Arc::clone(&runs);
            move || {
                let runs = Arc::clone(&runs);
                async move {
                    if runs.fetch_add(1, Ordering::SeqCst) == 0 {
                        panic!("first run fails");
                    }
                }
            }
        });
        handle.await.unwrap();

        assert_eq!(runs.load(Ordering::SeqCst), 2);
        assert!(restarts() > before);
    }
}
