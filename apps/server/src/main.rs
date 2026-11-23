use actix_web::HttpServer;

use rustrak::app::{app, AppState};
use rustrak::bootstrap::{self, file::BootstrapFile};
use rustrak::cli::{self, Command};
use rustrak::config;
use rustrak::db;
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

    let command = match cli::parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(e) => {
            eprintln!("error: {e}\n\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    match command {
        Command::Serve => {}
        Command::Help => {
            println!("{}", cli::USAGE);
            return Ok(());
        }
        Command::Version => {
            println!("rustrak {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Command::Healthcheck => match cli::local_address().and_then(cli::healthcheck) {
            Ok(()) => return Ok(()),
            Err(e) => fail(&e),
        },
        _ => {}
    }

    let config = config::Config::from_env().unwrap_or_else(|e| fail(&e.to_string()));
    let bootstrap_file = load_bootstrap_file(&config).unwrap_or_else(|e| fail(&e.to_string()));
    match command {
        Command::Serve => {}
        Command::CheckConfig => {
            println!("Configuration is valid");
            return Ok(());
        }
        command => return run_admin(command, config),
    }

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
    .block_on(serve(config, bootstrap_file))
}

async fn serve(
    config: config::Config,
    bootstrap_file: Option<BootstrapFile>,
) -> std::io::Result<()> {
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
    // On Postgres compacting is a tool on offer, not a state to warn about.
    #[cfg(feature = "sqlite")]
    match rustrak::db::maintenance::compaction(&db_pool).await {
        Ok(compaction) if compaction.needed => log::warn!(
            "This SQLite database was created by an earlier version of Rustrak and \
             never gives deleted data's space back to the disk ({} MiB could be freed \
             now). Compact it once from Settings > Storage; databases created by this \
             version never need it. https://rustrak.dev/configuration/database",
            compaction.reclaimable_bytes.unwrap_or(0) / (1024 * 1024)
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
    // Before anything else creates a project or a token, so the file's ids
    // and names are the ones it gets.
    if let Some(file) = &bootstrap_file {
        bootstrap::apply(&db_pool, file).await.map_err(|e| {
            log::error!("rustrak.toml: {e}");
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;
    }
    if let Some(value) = &config.bootstrap_token {
        if let Err(e) = bootstrap::bootstrap_token(&db_pool, value.expose()).await {
            log::error!("Failed to create the bootstrap token: {e}");
        }
    }
    if let Err(e) = bootstrap::create_superuser_if_needed(
        &db_pool,
        config.create_superuser.as_ref().map(config::Secret::expose),
    )
    .await
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

/// Reports a failed command and exits 1.
fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1)
}

/// The commands that need the database: `migrate` and `createuser`.
fn run_admin(command: Command, config: config::Config) -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let pool = db::create_pool(&config.database)
            .await
            .map_err(|e| format!("cannot open the database: {e}"))?;
        match command {
            Command::Migrate => {
                db::run_migrations(&pool).await.map_err(|e| e.to_string())?;
                println!("Migrations applied");
            }
            Command::CreateUser(spec) => {
                db::run_migrations(&pool).await.map_err(|e| e.to_string())?;
                let password = match &spec.password {
                    Some(password) => password.clone(),
                    None => cli::read_password().map_err(|e| e.to_string())?,
                };
                cli::create_user(&pool, &spec, &password)
                    .await
                    .map_err(|e| e.to_string())?;
                println!("User {} saved", spec.email);
            }
            _ => unreachable!("only database commands reach run_admin"),
        }
        Ok::<_, String>(())
    });
    result.unwrap_or_else(|e| fail(&e));
    Ok(())
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

/// Reads `rustrak.toml` when `RUSTRAK_CONFIG` names one, before the database
/// is opened: a mistake in it stops the server before anything is written.
fn load_bootstrap_file(config: &config::Config) -> std::io::Result<Option<BootstrapFile>> {
    let Some(path) = &config.config_file else {
        return Ok(None);
    };
    let refuse = |message: String| {
        log::error!("{message}");
        std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
    };
    let file = BootstrapFile::load(path).map_err(|e| refuse(e.to_string()))?;
    // Two first owners, and nothing says which one was meant.
    if file.superuser.is_some()
        && config
            .create_superuser
            .as_ref()
            .is_some_and(|s| !s.expose().is_empty())
    {
        return Err(refuse(format!(
            "{}: [bootstrap.superuser] and CREATE_SUPERUSER are both set; keep one",
            path.display()
        )));
    }
    log::info!("Read {}", path.display());
    Ok(Some(file))
}
