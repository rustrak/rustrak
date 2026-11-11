//! How the server is put together: the shared state, the HTTP app and the
//! background workers. `main` and the tests both build the server from here.

use std::sync::Arc;
use std::time::Duration;

use actix_session::SessionMiddleware;
use actix_web::body::MessageBody;
use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{middleware, web, App};

use crate::auth::OidcService;
use crate::config::Config;
use crate::db::DbPool;
use crate::digest::processors::Processors;
use crate::digest::session_aggregator::SessionAggregator;
use crate::routes::dashboard::Dashboard;
use crate::routes::metrics::MetricsEndpoint;
use crate::scheduler::engine::Scheduler;
use crate::scheduler::jobs::{MonitorSweepJob, SessionPurgeJob, StorageCleanupJob};
use crate::services::oauth::{ClientMetadataSource, HttpClientMetadata};
use crate::services::sourcemap::{DbSourceMapProvider, SourceMapProvider};
use crate::services::sourcemap_store::{LocalSourceMapStore, SourceMapStore};
use crate::services::{AlertService, CleanupJob};
use crate::telemetry::{self, Counters, Reporter, TelemetryStatus};
use crate::workers::sourcemap_assembly::{AssemblyQueue, AssemblyWorker};

/// The session cookie's name. The OpenAPI spec documents it too.
pub const SESSION_COOKIE: &str = "rustrak_session";

/// Everything the handlers and workers share, built once per process.
/// Every field is public so a test can swap one before building the app.
#[derive(Clone)]
pub struct AppState {
    pub config: web::Data<Config>,
    pub pool: web::Data<DbPool>,
    pub oidc: web::Data<Option<OidcService>>,
    pub sourcemap_store: web::Data<Arc<dyn SourceMapStore>>,
    pub sourcemap_provider: web::Data<Arc<dyn SourceMapProvider>>,
    pub assembly_queue: web::Data<AssemblyQueue>,
    pub session_aggregator: Arc<SessionAggregator>,
    pub processors: web::Data<Processors>,
    /// One for the process, so a second cleanup is refused whichever worker
    /// takes the request.
    pub cleanup_job: web::Data<CleanupJob>,
    pub telemetry_reporter: web::Data<Arc<Reporter>>,
    pub telemetry_status: web::Data<TelemetryStatus>,
    pub metrics: web::Data<MetricsEndpoint>,
    pub client_metadata: web::Data<Arc<dyn ClientMetadataSource>>,
    /// The compiled dashboard, if one was shipped alongside the binary.
    /// Detected once: `HttpServer::new` builds the app once per worker, and a
    /// directory appearing halfway through would split the server.
    pub dashboard: Option<Dashboard>,
    #[cfg(feature = "openapi")]
    openapi: utoipa::openapi::OpenApi,
    #[cfg(feature = "openapi")]
    openapi_json: web::Data<String>,
}

impl AppState {
    /// Prepares the ingest directory and, with SSO configured, discovers the
    /// provider; nothing is spawned.
    pub async fn build(config: Config, pool: DbPool) -> std::io::Result<Self> {
        crate::ingest::prepare_ingest_dir(&config.ingest_dir)
            .await
            .map_err(|e| std::io::Error::other(format!("Ingest directory error: {e}")))?;

        let oidc = match config.oidc.clone() {
            Some(oidc) => {
                if oidc.admits_any_provider_account() {
                    log::warn!(
                        "OIDC_AUTO_PROVISION is on with no OIDC_ALLOWED_DOMAINS: anyone with an account at {} can sign in",
                        oidc.issuer_url
                    );
                }
                let service = OidcService::discover(oidc).await.map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("OIDC configuration error: {e}"),
                    )
                })?;
                log::info!("OpenID Connect SSO enabled ({})", service.provider_name());
                Some(service)
            }
            None => None,
        };

        let sourcemap_store: Arc<dyn SourceMapStore> =
            Arc::new(LocalSourceMapStore::new(&config.sourcemap_storage_path));
        let sourcemap_provider: Arc<dyn SourceMapProvider> = Arc::new(
            DbSourceMapProvider::new(pool.clone(), Arc::clone(&sourcemap_store))
                .with_cache_budget(config.sourcemap_cache_bytes),
        );
        let session_aggregator = SessionAggregator::new(
            pool.clone(),
            config.session_flush_interval_secs,
            config.session_cardinality_cap,
        );
        let processors = Processors::new(
            &config,
            Arc::clone(&sourcemap_provider),
            session_aggregator.clone(),
        );

        let dashboard = Dashboard::from_config(&config.dashboard);
        let telemetry_status =
            match telemetry::decide(&config.telemetry, telemetry::posthog::compiled_key()) {
                Ok(()) => TelemetryStatus::Enabled {
                    sink: telemetry::posthog::SINK_NAME,
                },
                Err(why) => TelemetryStatus::Disabled(why),
            };
        let telemetry_reporter = Arc::new(Reporter::new(
            pool.clone(),
            Arc::new(telemetry::posthog::PostHogSink::new(
                telemetry::posthog::ENDPOINT,
                telemetry::posthog::compiled_key().unwrap_or_default(),
            )),
            Counters::global(),
            telemetry::Context {
                dashboard_served: dashboard.is_some(),
                config: telemetry::ConfigFacts {
                    ssl_proxy: config.security.ssl_proxy,
                    public_url_set: config.public_url.is_some(),
                    smtp_configured: config.smtp.host.is_some(),
                    session_secret_set: config.security.session_secret_set,
                    alert_providers: Vec::new(),
                    github_connected: false,
                    quota_customized: config.rate_limit.is_customized(),
                },
                sqlite_path: telemetry::sqlite_path_from_url(&config.database.url),
                ingest_dir: config.ingest_dir.clone(),
            },
        ));
        let metrics = MetricsEndpoint::new(
            config.metrics_enabled,
            config.ingest_dir.clone(),
            Counters::global(),
        );

        #[cfg(feature = "openapi")]
        let openapi = <crate::openapi::ApiDoc as utoipa::OpenApi>::openapi();

        Ok(Self {
            #[cfg(feature = "openapi")]
            openapi_json: web::Data::new(openapi.to_pretty_json().expect("valid openapi spec")),
            #[cfg(feature = "openapi")]
            openapi,
            config: web::Data::new(config),
            pool: web::Data::new(pool),
            oidc: web::Data::new(oidc),
            sourcemap_store: web::Data::new(sourcemap_store),
            sourcemap_provider: web::Data::new(sourcemap_provider),
            assembly_queue: web::Data::new(AssemblyQueue::default()),
            session_aggregator,
            processors: web::Data::new(processors),
            cleanup_job: web::Data::new(CleanupJob::default()),
            telemetry_reporter: web::Data::new(telemetry_reporter),
            telemetry_status: web::Data::new(telemetry_status),
            metrics: web::Data::new(metrics),
            client_metadata: web::Data::new(Arc::new(HttpClientMetadata)),
            dashboard,
        })
    }

    /// Starts the source map assembly worker, which first assembles what a
    /// previous run left queued.
    pub fn spawn_assembly_worker(&self) {
        tokio::spawn(
            AssemblyWorker::new(
                self.pool.get_ref().clone(),
                Arc::clone(&self.sourcemap_store),
                self.config.max_bundle_size_bytes(),
            )
            .run(self.assembly_queue.clone().into_inner()),
        );
    }

    /// Starts every background task: source map assembly, session flushes,
    /// digest recovery, the scheduler, alert retries and, when it is on, the
    /// telemetry reporter.
    pub fn spawn_workers(&self) {
        let pool = self.pool.get_ref().clone();

        self.spawn_assembly_worker();
        tokio::spawn(Arc::clone(&self.session_aggregator).run());

        tokio::spawn({
            let job = self.cleanup_job.clone();
            let pool = pool.clone();
            async move { job.warm_snapshot(&pool).await }
        });

        // A large backlog or a database that is down for a while must not
        // keep the HTTP listener from binding.
        tokio::spawn(crate::ingest::pipeline::recover_pending_events(
            pool.clone(),
            self.processors.clone().into_inner(),
            self.config.ingest_dir.clone(),
        ));

        // One tick a minute, the granularity of every schedule.
        let scheduler = Scheduler::new(vec![
            Arc::new(MonitorSweepJob::new(self.processors.clone().into_inner())),
            Arc::new(SessionPurgeJob),
            Arc::new(StorageCleanupJob::new(
                self.cleanup_job.clone().into_inner(),
                Arc::clone(&self.sourcemap_store),
            )),
        ]);
        tokio::spawn({
            let pool = pool.clone();
            async move {
                let mut tick = tokio::time::interval(Duration::from_secs(60));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    tick.tick().await;
                    if let Err(e) = scheduler.tick(&pool, chrono::Utc::now()).await {
                        log::error!("Scheduler tick failed: {:?}", e);
                    }
                }
            }
        });

        tokio::spawn({
            let pool = pool.clone();
            let smtp = self.config.smtp.clone();
            async move {
                loop {
                    if let Err(e) = AlertService::process_retry_queue(
                        &pool,
                        &smtp,
                        crate::services::alert::MAX_ALERT_RETRIES,
                    )
                    .await
                    {
                        log::error!("Alert retry worker error: {:?}", e);
                    }
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            }
        });

        if let TelemetryStatus::Enabled { .. } = **self.telemetry_status {
            tokio::spawn(Arc::clone(self.telemetry_reporter.get_ref()).run());
        }
    }
}

/// The HTTP app: shared state, middleware and every route, the dashboard last.
pub fn app(
    state: &AppState,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<impl MessageBody>,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    let app = App::new()
        .app_data(state.pool.clone())
        .app_data(state.config.clone())
        .app_data(state.oidc.clone())
        .app_data(state.sourcemap_provider.clone())
        .app_data(state.sourcemap_store.clone())
        .app_data(state.assembly_queue.clone())
        .app_data(state.processors.clone())
        .app_data(state.telemetry_reporter.clone())
        .app_data(state.telemetry_status.clone())
        .app_data(state.metrics.clone())
        .app_data(state.cleanup_job.clone())
        .app_data(state.client_metadata.clone())
        // `Logger::default()`'s format, plus the incident id a 5xx echoes
        // in `INCIDENT_ID_HEADER`. `error_response` never sees the request
        // and so cannot name the route in its own log line; this is the
        // line that has the method and path, so the id has to appear here
        // for the two to be greppable as one incident. Reads `-` on every
        // response that is not a 5xx.
        //
        // `%{r}a` instead of `%a`: behind a reverse proxy the peer is
        // always the proxy, so `%a` logs its address on every line. The
        // real-IP form reads `Forwarded` / `X-Forwarded-For` and falls
        // back to the peer without them, the same address ingest already
        // stores on each event (`realip_remote_addr` in routes/ingest.rs).
        .wrap(middleware::Logger::new(&format!(
            "%{{r}}a \"%r\" %s %b \"%{{Referer}}i\" \"%{{User-Agent}}i\" %T incident=%{{{}}}o",
            crate::error::INCIDENT_ID_HEADER
        )))
        .wrap(middleware::Compress::default())
        .wrap(crate::middleware::telemetry::TelemetryMiddleware::new(
            Counters::global(),
        ))
        // Before the session middleware, so a preflight never touches it.
        .wrap(crate::middleware::cors::cors())
        .wrap(
            SessionMiddleware::builder(
                crate::services::DbSessionStore::new(state.pool.get_ref().clone()),
                state.config.security.session_key.clone(),
            )
            .cookie_name(SESSION_COOKIE.to_string())
            .cookie_secure(state.config.security.ssl_proxy)
            .cookie_http_only(true)
            .cookie_same_site(actix_web::cookie::SameSite::Lax)
            .build(),
        )
        .configure(crate::routes::configure);

    #[cfg(feature = "openapi")]
    let app = {
        use utoipa_scalar::{Scalar, Servable};
        app.app_data(state.openapi_json.clone())
            .service(Scalar::with_url("/docs", state.openapi.clone()))
            .route(
                "/api-docs/openapi.json",
                web::get().to(|s: web::Data<String>| async move {
                    actix_web::HttpResponse::Ok()
                        .content_type("application/json")
                        .body(s.get_ref().clone())
                }),
            )
    };

    // Last, and it has to be: it ends in a catch-all that answers every
    // unclaimed path with the application shell.
    match &state.dashboard {
        Some(dashboard) => app.configure(dashboard.configure()),
        None => app,
    }
}
