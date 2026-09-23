//! Integration tests module
//!
//! Contains tests that require a database and test the full API.

mod agents_api_test;
mod alerts_api_test;
mod auth_test;
mod bootstrap_test;
mod concurrency_test;
mod digest_test;
mod envelope_v2_test;
mod event_lookup_test;
mod events_api_test;
mod field_errors_test;
mod health_test;
mod ingest_test;
mod issues_api_test;
mod logs_api_test;
mod projects_api_test;
mod rate_limit_test;
mod releases_api_test;
mod sessions_api_test;
mod sourcemaps_api_test;
mod span_v2_ingest_test;
mod spans_api_test;
mod stats_api_test;
mod storage_api_test;
mod team_rbac_test;
mod tokens_api_test;
mod transactions_api_test;
