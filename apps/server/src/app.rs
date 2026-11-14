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
use crate::workers::supervise;

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
        // A backlog a previous process left in the spool is pending from the
        // first request on; the recovery worker digests it later. Only file
        // names are read: the filter sees each event's identity before its
        // file is opened, and declining every one lists nothing.
        crate::ingest::storage::list_pending_event_metadata_except(
            &config.ingest_dir,
            |project_id, event_id| {
                processors.errors.pending().insert(project_id, event_id);
                true
            },
        )
        .await
        .map_err(|e| std::io::Error::other(format!("Ingest directory error: {e}")))?;

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
        let pool = self.pool.get_ref().clone();
        let store = Arc::clone(&self.sourcemap_store);
        let max_bundle = self.config.max_bundle_size_bytes();
        let queue = self.assembly_queue.clone().into_inner();
        supervise("source map assembly", move || {
            AssemblyWorker::new(pool.clone(), Arc::clone(&store), max_bundle).run(queue.clone())
        });
    }

    /// Starts every background task: source map assembly, session flushes,
    /// digest recovery, the scheduler, alert retries and, when it is on, the
    /// telemetry reporter. Each long-lived one is [`supervise`]d.
    pub fn spawn_workers(&self) {
        let pool = self.pool.get_ref().clone();

        self.spawn_assembly_worker();
        supervise("session flusher", {
            let sessions = Arc::clone(&self.session_aggregator);
            move || Arc::clone(&sessions).run()
        });

        tokio::spawn({
            let job = self.cleanup_job.clone();
            let pool = pool.clone();
            async move { job.warm_snapshot(&pool).await }
        });

        // A large backlog or a database that is down for a while must not
        // keep the HTTP listener from binding.
        supervise("digest recovery", {
            let pool = pool.clone();
            let processors = self.processors.clone().into_inner();
            let ingest_dir = self.config.ingest_dir.clone();
            move || {
                crate::ingest::pipeline::recover_pending_events(
                    pool.clone(),
                    Arc::clone(&processors),
                    ingest_dir.clone(),
                )
            }
        });

        // One tick a minute, the granularity of every schedule.
        let scheduler = Arc::new(Scheduler::new(vec![
            Arc::new(MonitorSweepJob::new(self.processors.clone().into_inner())),
            Arc::new(SessionPurgeJob),
            Arc::new(StorageCleanupJob::new(
                self.cleanup_job.clone().into_inner(),
                Arc::clone(&self.sourcemap_store),
            )),
        ]));
        supervise("scheduler", {
            let pool = pool.clone();
            move || {
                let pool = pool.clone();
                let scheduler = Arc::clone(&scheduler);
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
            }
        });

        supervise("alert retries", {
            let pool = pool.clone();
            let smtp = self.config.smtp.clone();
            move || {
                let pool = pool.clone();
                let smtp = smtp.clone();
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
            }
        });

        // Ingest reads the spool volume's free space from here, never per
        // request.
        supervise("spool disk sampler", {
            let processors = self.processors.clone().into_inner();
            move || {
                let processors = Arc::clone(&processors);
                async move {
                    loop {
                        processors.errors.sample_free_disk().await;
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        // SQLite's checkpoints outside the digest's own batches: pool
        // connections never run one, so this keeps the log bounded when
        // only transactions, logs or spans arrive. Every second keeps each
        // checkpoint small enough not to stall ingest's fsyncs.
        #[cfg(feature = "sqlite")]
        supervise("wal checkpointer", {
            let pool = pool.clone();
            let processors = self.processors.clone().into_inner();
            move || {
                let pool = pool.clone();
                let processors = Arc::clone(&processors);
                async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        if let Err(e) = processors.errors.checkpoint_when_caught_up(&pool).await {
                            if !processors.errors.is_draining() {
                                log::warn!("WAL checkpoint failed: {e:?}");
                            }
                        }
                    }
                }
            }
        });

        // The spool size gauges, read on a timer so a scrape never walks the
        // spool.
        if self.metrics.enabled() {
            let metrics = self.metrics.clone().into_inner();
            supervise("spool metrics sampler", move || {
                let metrics = Arc::clone(&metrics);
                async move {
                    loop {
                        metrics.sample_spool().await;
                        tokio::time::sleep(crate::routes::metrics::SPOOL_SAMPLE_INTERVAL).await;
                    }
                }
            });
        }

        if let TelemetryStatus::Enabled { .. } = **self.telemetry_status {
            let reporter = Arc::clone(self.telemetry_reporter.get_ref());
            supervise("telemetry reporter", move || Arc::clone(&reporter).run());
        }
    }

    /// The first step of a shutdown, while the HTTP workers still run: the
    /// session counts are written first (only memory holds them, and an
    /// orchestrator's SIGKILL can land before [`Self::shutdown`]), ingest
    /// answers 503 from here on, and the digests already running get up to
    /// `grace` to finish. Those still waiting for a slot do not start: their
    /// events stay in the spool. It has to come before the server stops,
    /// because a digest runs on the HTTP worker that accepted its event, and
    /// a stopped worker drops its tasks. A digest still running at the
    /// deadline keeps its spool file, which the next start recovers: the
    /// spool, not this wait, is what keeps an acknowledged event safe.
    pub async fn drain(&self, grace: Duration) {
        self.flush_sessions().await;
        self.processors.errors.start_draining();
        let in_flight = self.processors.errors.in_flight();
        let deadline = tokio::time::Instant::now() + grace;
        while !in_flight.is_empty() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        if !in_flight.is_empty() {
            log::warn!(
                "{} digests did not finish before shutdown; their events stay in the spool for the next start",
                in_flight.len()
            );
        }
    }

    /// Writes the session counts held in memory. Ingest acknowledges
    /// sessions before they reach the database (#390). Idempotent.
    pub async fn flush_sessions(&self) {
        if let Err(e) = self.session_aggregator.flush().await {
            log::error!("Failed to flush session counts during shutdown: {e:?}");
        }
    }

    /// The last step, once the HTTP server has stopped: flush the session
    /// counts, checkpoint SQLite and close the pool.
    pub async fn shutdown(&self) {
        // Again: sessions acknowledged during the drain.
        self.flush_sessions().await;
        let pool = self.pool.get_ref();
        // After the batch it may be committing; bounded like `close` below.
        if tokio::time::timeout(
            Duration::from_secs(5),
            self.processors.errors.close_committer(),
        )
        .await
        .is_err()
        {
            log::warn!("The digest's connection was still busy at shutdown");
        }
        #[cfg(feature = "sqlite")]
        if let Err(e) = crate::db::checkpoint_full(pool).await {
            log::warn!("Final SQLite checkpoint failed: {e}");
        }
        // Bounded: `close` waits for every connection to come back, and a
        // stuck query must not keep the process from exiting.
        if tokio::time::timeout(Duration::from_secs(5), pool.close())
            .await
            .is_err()
        {
            log::warn!("Database connections were still busy at shutdown");
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
