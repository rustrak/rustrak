pub mod agents;
pub mod alerts;
pub mod auth;
pub mod dashboard;
pub mod events;
pub mod github;
pub mod health;
pub mod ingest;
pub mod invitations;
pub mod issues;
pub mod logs;
pub mod mcp;
pub mod members;
pub mod metrics;
pub mod monitors;
pub mod oauth;
pub mod period;
pub mod projects;
pub mod releases;
pub mod roles;
pub mod schedules;
pub mod sentry_api;
pub mod sessions;
pub mod sourcemaps;
pub mod spans;
pub mod stats;
pub mod storage;
pub mod team;
pub mod telemetry;
pub mod tokens;
pub mod transactions;

use actix_web::web;

/// Every API route. A project's routes share the one
/// `/api/projects/{project_id}` scope and register plain resources in it, so
/// no module can shadow another's paths whatever the order.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.configure(health::configure)
        .configure(metrics::configure)
        .configure(mcp::configure)
        .configure(oauth::configure)
        .configure(auth::configure)
        .configure(projects::configure)
        .configure(issues::configure)
        .configure(tokens::configure)
        .configure(team::configure)
        .configure(invitations::configure)
        .configure(roles::configure)
        .configure(alerts::configure)
        .configure(github::configure)
        .configure(releases::configure)
        .configure(schedules::configure)
        .configure(sourcemaps::configure)
        .configure(sentry_api::configure)
        .configure(storage::configure)
        .configure(telemetry::configure)
        .configure(ingest::configure)
        .service(
            web::scope("/api/projects/{project_id}")
                .configure(projects::configure_project)
                .configure(agents::configure_project)
                .configure(alerts::configure_project)
                .configure(events::configure_project)
                .configure(github::configure_project)
                .configure(issues::configure_project)
                .configure(logs::configure_project)
                .configure(members::configure_project)
                .configure(monitors::configure_project)
                .configure(releases::configure_project)
                .configure(sessions::configure_project)
                .configure(spans::configure_project)
                .configure(stats::configure_project)
                .configure(transactions::configure_project),
        );
}
