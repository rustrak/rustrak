//! End-to-end tests module
//!
//! Contains tests that use the real Sentry SDK to send events to Rustrak.

#[cfg(all(feature = "sqlite", unix))]
mod crash_test;
mod sentry_sdk_test;
