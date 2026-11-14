//! Rustrak Server Library
//!
//! This module exposes the server components for testing purposes.

/// Crash-test hook. In a test build (`test-seams`), aborts the process when
/// `RUSTRAK_FAILPOINT` names this point, on its `RUSTRAK_FAILPOINT_HIT`th pass
/// (default the first). In any other build it expands to nothing.
#[cfg(feature = "test-seams")]
#[macro_export]
macro_rules! failpoint {
    ($name:literal) => {{
        static HITS: ::std::sync::atomic::AtomicU32 = ::std::sync::atomic::AtomicU32::new(0);
        if ::std::env::var_os("RUSTRAK_FAILPOINT").is_some_and(|v| v == $name) {
            let at: u32 = ::std::env::var("RUSTRAK_FAILPOINT_HIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            if HITS.fetch_add(1, ::std::sync::atomic::Ordering::SeqCst) + 1 == at {
                ::log::error!("failpoint {} hit, aborting", $name);
                ::std::process::abort();
            }
        }
    }};
}

#[cfg(not(feature = "test-seams"))]
#[macro_export]
macro_rules! failpoint {
    ($name:literal) => {};
}

pub mod allocator;
pub mod app;
pub mod auth;
pub mod bootstrap;
pub mod config;
pub mod db;
pub mod digest;
pub mod error;
pub mod ingest;
pub mod logging;
pub mod mcp;
pub mod middleware;
pub mod models;
#[cfg(feature = "openapi")]
pub mod openapi;
pub mod pagination;
pub mod protocol;
pub mod routes;
pub mod scheduler;
pub mod services;
pub mod telemetry;
pub mod workers;
