use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{middleware, web, App, HttpServer};

use std::sync::Arc;

use rustrak::bootstrap;
use rustrak::config;
use rustrak::db;
use rustrak::middleware::auth::RequireAuth;
use rustrak::models;
use rustrak::routes;
use rustrak::services::sourcemap::{DbSourceMapProvider, SourceMapProvider};
use rustrak::services::sourcemap_store::LocalSourceMapStore;
use rustrak::services::AuthTokenService;
use rustrak::workers::session_aggregator::SessionAggregator;
use rustrak::workers::sourcemap_assembly::AssemblyWorker;

#[cfg(feature = "openapi")]
use rustrak::openapi;
#[cfg(feature = "openapi")]
use utoipa::OpenApi;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Initialize logging. RUSTRAK_LOG_TIMEZONE (IANA name)
    // optionally converts log timestamps for display; falls back to UTC if unset
    // or unrecognized. Display-only — event/issue timestamps are unaffected.
    let log_timezone = rustrak::logging::resolve_log_timezone();
    env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("info"))
        .format(move |buf, record| {
            use std::io::Write;
            let level_style = buf.default_level_style(record.level());
            writeln!(
                buf,
                "[{} {level_style}{}{level_style:#} {}] {}",
                rustrak::logging::format_log_timestamp(chrono::Utc::now(), log_timezone),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();

    if let Ok(tz_name) = std::env::var("RUSTRAK_LOG_TIMEZONE") {
        if log_timezone.is_none() {
            log::warn!(
                "RUSTRAK_LOG_TIMEZONE=\"{tz_name}\" is not a recognized IANA timezone name; falling back to UTC"
            );
        }
    }

    // Load configuration
    let config = config::Config::from_env().map_err(|e| {
        log::error!("Configuration error: {}", e);
        std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string())
    })?;

    log::info!("Starting Rustrak server on {}:{}", config.host, config.port);

    // Built before any I/O: an unusable secret must stop the process here, not
    // after migrations have run and the workers are up.
    if config.security.session_secret_key.is_none() {
        log::warn!(
            "SESSION_SECRET_KEY not set, using random key (sessions won't persist across restarts)"
        );
    }
    let key = config.security.session_key().map_err(|e| {
        log::error!("Configuration error: {}", e);
        std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string())
    })?;

    let ingest_dir = rustrak::ingest::get_ingest_dir(config.ingest_dir.as_deref());
    rustrak::ingest::prepare_ingest_dir(&ingest_dir)
        .await
        .map_err(|e| {
            log::error!("Ingest directory error: {}", e);
            std::io::Error::other(e.to_string())
        })?;

    // Create database pool
    let db_pool = db::create_pool(&config.database).await.map_err(|e| {
        log::error!("Database pool error: {}", e);
        std::io::Error::other(e.to_string())
    })?;

    // Run migrations
    db::run_migrations(&db_pool).await.map_err(|e| {
        log::error!("Migration error: {}", e);
        std::io::Error::other(e.to_string())
    })?;

    // Create source map store and provider
    let sourcemap_store: Arc<dyn rustrak::services::sourcemap_store::SourceMapStore> =
        Arc::new(LocalSourceMapStore::new(&config.sourcemap_storage_path));
    let sourcemap_provider: Arc<dyn SourceMapProvider> = Arc::new(
        DbSourceMapProvider::new(db_pool.clone(), Arc::clone(&sourcemap_store))
            .with_cache_budget(config.sourcemap_cache_bytes),
    );

    // Spawn assembly worker
    {
        let worker = AssemblyWorker::new(
            db_pool.clone(),
            Arc::clone(&sourcemap_store),
            config.max_chunk_size_bytes * 64,
        );
        tokio::spawn(worker.run());
    }

    // Spawn session aggregator
    let session_aggregator = SessionAggregator::new(
        db_pool.clone(),
        config.session_flush_interval_secs,
        config.session_cardinality_cap,
    );
    {
        let handle = session_aggregator.clone();
        tokio::spawn(SessionAggregator::run(handle));
    }

    // Anonymous telemetry. Counted always (the preview endpoint shows the
    // operator what would leave), sent only when every switch agrees.
    rustrak::telemetry::install_panic_hook(rustrak::telemetry::Counters::global());
    let telemetry_status = match rustrak::telemetry::decide(
        &config.telemetry,
        rustrak::telemetry::posthog::compiled_key(),
    ) {
        Ok(()) => rustrak::telemetry::TelemetryStatus::Enabled {
            sink: rustrak::telemetry::posthog::SINK_NAME,
        },
        Err(why) => rustrak::telemetry::TelemetryStatus::Disabled(why),
    };

    // Bootstrap: create initial token if none exist
    bootstrap_token(&db_pool).await;

    // Bootstrap: create superuser if CREATE_SUPERUSER is set
    if let Err(e) = bootstrap::create_superuser_if_needed(&db_pool).await {
        log::error!("Failed to create superuser: {}", e);
    }

    // Clone values for the closure
    let host = config.host.clone();
    let port = config.port;

    // Pre-compute OpenAPI spec once per process (not once per worker)
    #[cfg(feature = "openapi")]
    let openapi_spec = web::Data::new(
        openapi::ApiDoc::openapi()
            .to_pretty_json()
            .expect("valid openapi spec"),
    );
    #[cfg(feature = "openapi")]
    let openapi_scalar_doc = openapi::ApiDoc::openapi();

    // The compiled dashboard, if one was shipped alongside the binary.
    //
    // Detected once here rather than per worker: the answer cannot change
    // while the process runs, and `HttpServer::new` calls its factory once per
    // worker thread, which would otherwise mean one stat of the filesystem per
    // core at startup and a server that disagrees with itself if the directory
    // appears halfway through.
    let dashboard = routes::dashboard::Dashboard::from_config(&config.dashboard);
    match &dashboard {
        Some(found) => log::info!("Serving the dashboard from {}", found.root().display()),
        None if !config.dashboard.enabled => {
            log::info!("Dashboard switched off by RUSTRAK_DASHBOARD — serving the API only")
        }
        None => log::info!(
            "No dashboard build at {} — serving the API only",
            config.dashboard.dir
        ),
    }
    let serve_dashboard = dashboard.is_some();

    let telemetry_reporter = Arc::new(rustrak::telemetry::Reporter::new(
        db_pool.clone(),
        Arc::new(rustrak::telemetry::posthog::PostHogSink::new(
            rustrak::telemetry::posthog::ENDPOINT,
            rustrak::telemetry::posthog::compiled_key().unwrap_or_default(),
        )),
        rustrak::telemetry::Counters::global(),
        rustrak::telemetry::Context {
            dashboard_served: serve_dashboard,
            config: rustrak::telemetry::ConfigFacts {
                ssl_proxy: config.security.ssl_proxy,
                public_url_set: config.public_url.is_some(),
                smtp_configured: std::env::var("SMTP_HOST").is_ok_and(|h| !h.trim().is_empty()),
                session_secret_set: config.security.session_secret_key.is_some(),
                alert_providers: Vec::new(),
                quota_customized: config.rate_limit.is_customized(),
            },
            sqlite_path: rustrak::telemetry::sqlite_path_from_url(&config.database.url),
            ingest_dir: ingest_dir.clone(),
        },
    ));
    match telemetry_status {
        rustrak::telemetry::TelemetryStatus::Enabled { .. } => {
            let instance = rustrak::telemetry::instance_id(&db_pool)
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            log::info!(
                "Anonymous telemetry is on (instance {instance}). RUSTRAK_TELEMETRY=off disables it. \
                 https://rustrak.github.io/rustrak/configuration/telemetry"
            );
            tokio::spawn(Arc::clone(&telemetry_reporter).run());
        }
        rustrak::telemetry::TelemetryStatus::Disabled(why) => {
            log::info!("Telemetry is off: {why}.");
        }
    }
    let telemetry_reporter_data = web::Data::new(telemetry_reporter);
    let telemetry_status_data = web::Data::new(telemetry_status);

    let session_aggregator_data = web::Data::new(session_aggregator.clone());

    // Processor registry — single dispatch surface for the ingest pipeline.
    // Built once; each processor owns the deps it needs.
    let processors_data = web::Data::new(
        rustrak::digest::processors::Processors::new(
            ingest_dir.clone(),
            config.rate_limit.clone(),
            Arc::clone(&sourcemap_provider),
            Some(session_aggregator.clone()),
        )
        .with_dashboard_url(config.dashboard_url()),
    );

    // Recovery is a background worker: a large backlog or a temporarily
    // unavailable database must not prevent the HTTP listener from binding.
    tokio::spawn(routes::ingest::recover_pending_events(
        db_pool.clone(),
        processors_data.clone(),
        ingest_dir,
    ));

    let alert_pool = db_pool.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = rustrak::services::AlertService::process_retry_queue(
                &alert_pool,
                rustrak::services::alert::MAX_ALERT_RETRIES,
            )
            .await
            {
                log::error!("Alert retry worker error: {:?}", e);
            }
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });

    let server = HttpServer::new(move || {
        let cors = rustrak::middleware::cors::cors();

        let sourcemap_provider_data = web::Data::new(Arc::clone(&sourcemap_provider));
        let sourcemap_store_data = web::Data::new(Arc::clone(&sourcemap_store));

        let app = App::new()
            // Share database pool and config with all handlers
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::new(config.clone()))
            .app_data(sourcemap_provider_data)
            .app_data(sourcemap_store_data)
            .app_data(session_aggregator_data.clone())
            .app_data(processors_data.clone())
            .app_data(telemetry_reporter_data.clone())
            .app_data(telemetry_status_data.clone())
            // Middleware
            // `Logger::default()`'s format, plus the incident id a 5xx echoes
            // in `INCIDENT_ID_HEADER`. `error_response` never sees the request
            // and so cannot name the route in its own log line; this is the
            // line that has the method and path, so the id has to appear here
            // for the two to be greppable as one incident. Reads `-` on every
            // response that is not a 5xx.
            .wrap(middleware::Logger::new(&format!(
                "%a \"%r\" %s %b \"%{{Referer}}i\" \"%{{User-Agent}}i\" %T incident=%{{{}}}o",
                rustrak::error::INCIDENT_ID_HEADER
            )))
            .wrap(middleware::Compress::default())
            .wrap(rustrak::middleware::telemetry::TelemetryMiddleware::new(
                rustrak::telemetry::Counters::global(),
            ))
            .wrap(cors) // CORS must be before SessionMiddleware
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), key.clone())
                    .cookie_name("rustrak_session".to_string())
                    .cookie_secure(config.security.ssl_proxy)
                    .cookie_http_only(true)
                    .cookie_same_site(actix_web::cookie::SameSite::Lax)
                    .build(),
            )
            // Authentication middleware (must be after SessionMiddleware)
            .wrap(RequireAuth::new(serve_dashboard))
            // Health check routes (no auth required)
            .service(
                web::scope("/health")
                    .route("", web::get().to(routes::health::liveness))
                    .route("/version", web::get().to(routes::health::version))
                    .route("/ready", web::get().to(routes::health::readiness)),
            )
            // Root health check alias
            .route("/health", web::get().to(routes::health::liveness))
            // Auth routes (public - no Bearer auth required)
            .configure(routes::auth::configure)
            // API routes (auth required)
            // More specific routes first: events > issues > alert-rules > projects
            .configure(routes::events::configure)
            .configure(routes::issues::configure)
            .configure(routes::alerts::configure_rules)
            .configure(routes::alerts::configure_history)
            // Project members (more specific than generic projects scope)
            .configure(routes::members::configure)
            // Session stats routes (more specific than generic projects scope)
            .configure(routes::sessions::configure)
            // Project-wide stats for the overview (more specific than generic projects scope)
            .configure(routes::stats::configure)
            // Releases API (more specific than generic projects scope)
            .configure(routes::releases::configure)
            // Transactions API (more specific than generic projects scope)
            .configure(routes::transactions::configure)
            // Logs API (more specific than generic projects scope)
            .configure(routes::logs::configure)
            // Spans API (more specific than generic projects scope)
            .configure(routes::spans::configure)
            // AI Agent Monitoring dashboard API (more specific than generic projects scope)
            .configure(routes::agents::configure)
            // Then generic projects/tokens routes
            .configure(routes::projects::configure)
            .configure(routes::tokens::configure)
            // Team & invitations (global admin)
            .configure(routes::team::configure)
            .configure(routes::invitations::configure)
            // Alert channels (global, not nested under projects)
            .configure(routes::alerts::configure_channels)
            // Source map upload routes (Bearer auth, Sentry-cli compatible)
            .configure(routes::sourcemaps::configure)
            // Storage usage + retention/cleanup (admin only)
            .configure(routes::storage::configure)
            // What the anonymous telemetry would send (admin only)
            .configure(routes::telemetry::configure)
            // Ingest routes (Sentry SDK auth)
            .configure(routes::ingest::configure);

        #[cfg(feature = "openapi")]
        let app = {
            use utoipa_scalar::{Scalar, Servable};
            app.app_data(openapi_spec.clone())
                .service(Scalar::with_url("/docs", openapi_scalar_doc.clone()))
                .route(
                    "/api-docs/openapi.json",
                    web::get().to(|s: web::Data<String>| async move {
                        actix_web::HttpResponse::Ok()
                            .content_type("application/json")
                            .body(s.get_ref().clone())
                    }),
                )
        };

        // The dashboard goes last, and has to: it ends in a catch-all that
        // answers every unclaimed path with the application shell, so anything
        // registered after it would never be reached.
        match &dashboard {
            Some(dashboard) => app.configure(dashboard.configure()),
            None => app,
        }
    })
    .bind((host.as_str(), port))?
    .shutdown_timeout(30)
    .run();

    // Spawn graceful shutdown handler
    let server_handle = server.handle();
    let agg_for_shutdown = session_aggregator.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        log::info!("Shutdown signal received, stopping server...");
        // Stop accepting new requests first, then flush remaining buckets
        server_handle.stop(true).await;
        if let Err(e) = agg_for_shutdown.flush().await {
            log::error!(
                "Failed to flush session aggregator during shutdown: {:?}",
                e
            );
        }
    });

    server.await
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {}
            Err(e) => {
                log::error!("Failed to install Ctrl+C handler: {}", e);
                // Wait forever if signal handler fails
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                log::error!("Failed to install SIGTERM handler: {}", e);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

/// Bootstrap: create initial token if none exist and RUSTRAK_BOOTSTRAP_TOKEN is set
async fn bootstrap_token(pool: &db::DbPool) {
    // Check if bootstrap is requested via env var
    if std::env::var("RUSTRAK_BOOTSTRAP_TOKEN").is_err() {
        return;
    }

    // Check if any tokens exist
    match AuthTokenService::has_any_token(pool).await {
        Ok(true) => {
            log::info!("Auth tokens already exist, skipping bootstrap");
        }
        Ok(false) => {
            // Create bootstrap token
            let input = models::CreateAuthToken {
                description: Some("Bootstrap token (created automatically)".to_string()),
            };

            match AuthTokenService::create(pool, input).await {
                Ok(token) => {
                    // Print to stderr directly (not logs) to avoid token in log aggregators
                    eprintln!();
                    eprintln!("==============================================");
                    eprintln!("BOOTSTRAP TOKEN CREATED - SAVE THIS NOW!");
                    eprintln!("Token: {}", token.token);
                    eprintln!("This token will NOT be shown again.");
                    eprintln!("==============================================");
                    eprintln!();
                    log::info!("Bootstrap token created successfully");
                }
                Err(e) => {
                    log::error!("Failed to create bootstrap token: {}", e);
                }
            }
        }
        Err(e) => {
            log::error!("Failed to check for existing tokens: {}", e);
        }
    }
}
