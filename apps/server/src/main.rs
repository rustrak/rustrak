use actix_web::HttpServer;

use rustrak::app::{app, AppState};
use rustrak::bootstrap;
use rustrak::config;
use rustrak::db;
use rustrak::models;
use rustrak::services::AuthTokenService;
use rustrak::telemetry::TelemetryStatus;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    rustrak::allocator::tune();

    let log_timezone = config::log_timezone();
    let tz = log_timezone.clone().ok().flatten();
    env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("info"))
        .format(move |buf, record| {
            use std::io::Write;
            let level_style = buf.default_level_style(record.level());
            writeln!(
                buf,
                "[{} {level_style}{}{level_style:#} {}] {}",
                rustrak::logging::format_log_timestamp(chrono::Utc::now(), tz),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();
    if let Err(name) = log_timezone {
        log::warn!(
            "RUSTRAK_LOG_TIMEZONE=\"{name}\" is not a recognized IANA timezone name; falling back to UTC"
        );
    }

    let config = config::Config::from_env().map_err(|e| {
        log::error!("Configuration error: {}", e);
        std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string())
    })?;

    // Capped so Tokio does not start a thread whenever none is idle, each
    // holding an allocator heap. Room for the live digest's and recovery's
    // preps, plus file removal and symbolication.
    let blocking = 2 * config.digest_prep_workers + 4;
    actix_web::rt::System::with_tokio_rt(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(blocking)
            .build()
            .expect("cannot start the main runtime")
    })
    .block_on(serve(config))
}

async fn serve(config: config::Config) -> std::io::Result<()> {
    log::info!("Starting Rustrak server on {}:{}", config.host, config.port);
    rustrak::services::notification::allow_private_destinations(
        config.alerts_allow_private_networks,
    );
    if !config.security.session_secret_set {
        log::warn!(
            "SESSION_SECRET_KEY not set, using random key (sessions won't persist across restarts)"
        );
    }

    let db_pool = db::create_pool(&config.database).await.map_err(|e| {
        log::error!("Database pool error: {}", e);
        std::io::Error::other(e.to_string())
    })?;
    db::run_migrations(&db_pool).await.map_err(|e| {
        log::error!("Migration error: {}", e);
        std::io::Error::other(e.to_string())
    })?;

    // A SQLite file from before incremental mode never shrinks; say so once.
    match rustrak::db::maintenance::compaction(&db_pool).await {
        Ok(compaction) if compaction.needed => log::warn!(
            "This SQLite database was created by an earlier version of Rustrak and \
             never gives deleted data's space back to the disk ({} MiB could be freed \
             now). Compact it once from Settings > Storage; databases created by this \
             version never need it. https://rustrak.dev/configuration/database",
            compaction.reclaimable_bytes / (1024 * 1024)
        ),
        Ok(_) => {}
        Err(e) => log::warn!("Could not read the database's compaction state: {e}"),
    }

    // Anonymous telemetry. Counted always (the preview endpoint shows the
    // operator what would leave), sent only when every switch agrees.
    rustrak::telemetry::install_panic_hook(rustrak::telemetry::Counters::global());

    // A one-off migration of tokens stored before they were hashed.
    match AuthTokenService::hash_plaintext_tokens(&db_pool).await {
        Ok(0) => {}
        Ok(n) => log::info!("Hashed {n} API token(s) stored before tokens were hashed"),
        Err(e) => log::error!("Failed to hash stored API tokens: {e}"),
    }
    if config.bootstrap_token {
        bootstrap_token(&db_pool).await;
    }
    if let Err(e) =
        bootstrap::create_superuser_if_needed(&db_pool, config.create_superuser.as_deref()).await
    {
        log::error!("Failed to create superuser: {}", e);
    }

    let host = config.host.clone();
    let port = config.port;
    let metrics_enabled = config.metrics_enabled;
    let dashboard_dir = config.dashboard.dir.clone();
    let dashboard_enabled = config.dashboard.enabled;

    let state = AppState::build(config, db_pool.clone())
        .await
        .map_err(|e| {
            log::error!("{e}");
            e
        })?;

    match &state.dashboard {
        Some(found) => log::info!("Serving the dashboard from {}", found.root().display()),
        None if !dashboard_enabled => {
            log::info!("Dashboard switched off by RUSTRAK_DASHBOARD — serving the API only")
        }
        None => log::info!("No dashboard build at {dashboard_dir} — serving the API only"),
    }
    match **state.telemetry_status {
        TelemetryStatus::Enabled { .. } => {
            let instance = rustrak::telemetry::instance_id(&db_pool)
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            log::info!(
                "Anonymous telemetry is on (instance {instance}). RUSTRAK_TELEMETRY=off disables it. \
                 https://rustrak.dev/configuration/telemetry"
            );
        }
        TelemetryStatus::Disabled(why) => log::info!("Telemetry is off: {why}."),
    }
    if metrics_enabled {
        log::info!("Prometheus metrics enabled at /metrics");
    }

    state.spawn_workers();

    // On SIGTERM or SIGINT the digests drain first, while the HTTP workers
    // they run on are still alive; only then does Actix stop, finishing
    // in-flight requests, and `server.await` return. The rest of the
    // shutdown runs here, on main, so the runtime cannot exit before it
    // commits.
    let server = HttpServer::new({
        let state = state.clone();
        move || app(&state)
    })
    .shutdown_signal({
        let state = state.clone();
        async move {
            stop_signal().await;
            log::info!("Shutting down: draining in-flight digests (signal again to stop now)");
            // With a shutdown future Actix installs no signal handlers, so a
            // second signal is handled here. What is not digested stays in
            // the spool for the next start.
            tokio::spawn({
                let state = state.clone();
                async move {
                    let code = stop_signal().await;
                    log::warn!(
                        "Second stop signal: exiting now; undigested events stay in the spool"
                    );
                    let flush = state.flush_sessions();
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), flush).await;
                    std::process::exit(code);
                }
            });
            // 5 s, so the drain, Actix's stop and the final checkpoint fit in
            // Docker's default 10 s stop grace when the digests are healthy.
            state.drain(std::time::Duration::from_secs(5)).await;
        }
    })
    // Spool writes and large decodes: without a cap a slow fsync makes each
    // waiting write start a thread (and an allocator heap), and they all
    // wait on the same disk anyway.
    .worker_max_blocking_threads(4)
    .bind((host.as_str(), port))?
    .shutdown_timeout(30)
    .run();

    let result = server.await;
    state.shutdown().await;
    result
}

/// SIGTERM (what an orchestrator sends) or SIGINT (Ctrl-C), whichever comes
/// first. The first stops the server gracefully, a second at once. Returns
/// the conventional exit code for it: 128 + the signal number.
async fn stop_signal() -> i32 {
    const SIGINT_EXIT: i32 = 130;
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = term.recv() => 143,
                    _ = tokio::signal::ctrl_c() => SIGINT_EXIT,
                }
            }
            Err(e) => {
                log::warn!("Cannot listen for SIGTERM ({e}); only Ctrl-C stops the server");
                let _ = tokio::signal::ctrl_c().await;
                SIGINT_EXIT
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        SIGINT_EXIT
    }
}

/// Creates a first API token when none exists.
async fn bootstrap_token(pool: &db::DbPool) {
    match AuthTokenService::has_any_token(pool).await {
        Ok(true) => {
            log::info!("Auth tokens already exist, skipping bootstrap");
        }
        Ok(false) => {
            let input = models::CreateAuthToken {
                description: Some("Bootstrap token (created automatically)".to_string()),
                ..Default::default()
            };

            match AuthTokenService::create(pool, input).await {
                Ok(token) => {
                    // To stderr, not the log: log aggregators must never see it.
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
