//! Runs plugin components: compiles them once and caches the machine code,
//! gives each call a fresh instance with a memory cap and a CPU deadline, and
//! implements the host side of `wit/plugin.wit`.
//!
//! Calls are synchronous and CPU-bound: callers run them on the blocking pool.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use super::store::{self, PluginDb};

wasmtime::component::bindgen!({ path: "wit", world: "plugin" });

use self::rustrak::plugin::{log as plugin_log, settings, sql};

/// Linear memory one instance may grow to.
pub(super) const MEMORY_LIMIT: usize = 32 << 20;
/// One epoch tick. Deadlines are whole ticks.
const TICK: Duration = Duration::from_millis(10);

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error("the plugin ran past its time budget")]
    Timeout,
    #[error("the plugin crashed: {0}")]
    Trap(String),
}

/// One per process: the engine, the linker, and the ticker that drives
/// every deadline.
pub struct Runtime {
    engine: Engine,
    linker: Linker<Host>,
}

/// A plugin ready to be called: its compiled code and its own database.
pub struct Loaded {
    pre: PluginPre<Host>,
    id: Arc<str>,
    db: Option<Arc<PluginDb>>,
}

impl Loaded {
    /// Its own database, if it asked for one.
    pub fn db(&self) -> Option<&Arc<PluginDb>> {
        self.db.as_ref()
    }
}

/// The state one call's instance sees.
pub struct Host {
    wasi: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
    plugin: Arc<str>,
    db: Option<Arc<PluginDb>>,
    deadline: Instant,
    settings: Option<SettingsSource>,
}

/// The plugin's settings as JSON, for the project given or the instance.
pub type SettingsSource = Arc<dyn Fn(Option<u64>) -> String + Send + Sync>;

impl WasiView for Host {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl Runtime {
    pub fn new() -> wasmtime::Result<Arc<Self>> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config)?;

        let mut linker = Linker::<Host>::new(&engine);
        // Rust's std imports these; the context behind them is empty.
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        Plugin::add_to_linker::<Host, HasSelf<Host>>(&mut linker, |h| h)?;

        let ticker = engine.weak();
        std::thread::Builder::new()
            .name("plugin-epoch".into())
            .spawn(move || {
                while let Some(engine) = ticker.upgrade() {
                    engine.increment_epoch();
                    drop(engine);
                    std::thread::sleep(TICK);
                }
            })?;

        Ok(Arc::new(Self { engine, linker }))
    }

    /// Compiles `wasm` to machine code, or reuses what an earlier call left
    /// in `cache_dir`. The cache is named after the component's hash and
    /// rebuilt when this engine cannot load it.
    pub fn load(
        &self,
        id: &str,
        wasm: &[u8],
        cache_dir: &Path,
        db: Option<Arc<PluginDb>>,
    ) -> wasmtime::Result<Loaded> {
        let hash = hex::encode(Sha256::digest(wasm));
        let cache = cache_dir.join(format!("server-{}.cwasm", &hash[..16]));
        // SAFETY: the file is one this host wrote into its own plugin
        // directory; wasmtime checks it was built by a compatible engine and
        // refuses it otherwise, which recompiles below.
        let cached = cache
            .exists()
            .then(|| unsafe { Component::deserialize_file(&self.engine, &cache) }.ok())
            .flatten();
        let component = match cached {
            Some(c) => c,
            None => {
                let c = Component::from_binary(&self.engine, wasm)?;
                std::fs::create_dir_all(cache_dir)?;
                let tmp = cache.with_extension("tmp");
                std::fs::write(&tmp, c.serialize()?)?;
                std::fs::rename(&tmp, &cache)?;
                c
            }
        };
        let pre = PluginPre::new(self.linker.instantiate_pre(&component)?)?;
        Ok(Loaded {
            pre,
            id: id.into(),
            db,
        })
    }

    fn store(
        &self,
        plugin: &Loaded,
        budget: Duration,
        settings: Option<SettingsSource>,
    ) -> Store<Host> {
        let mut store = Store::new(
            &self.engine,
            Host {
                wasi: WasiCtx::builder().build(),
                table: ResourceTable::new(),
                limits: StoreLimitsBuilder::new()
                    .memory_size(MEMORY_LIMIT)
                    .memories(1)
                    .build(),
                plugin: plugin.id.clone(),
                db: plugin.db.clone(),
                deadline: Instant::now() + budget,
                settings,
            },
        );
        store.limiter(|h| &mut h.limits);
        store.set_epoch_deadline((budget.as_millis() / TICK.as_millis()).max(1) as u64);
        store
    }

    /// One HTTP request, on a fresh instance.
    pub fn handle_http(
        &self,
        plugin: &Loaded,
        req: Request,
        budget: Duration,
        settings: SettingsSource,
    ) -> Result<Response, CallError> {
        let mut store = self.store(plugin, budget, Some(settings));
        let instance = plugin.pre.instantiate(&mut store).map_err(trap)?;
        instance.call_handle_http(&mut store, &req).map_err(trap)
    }
}

fn trap(e: wasmtime::Error) -> CallError {
    match e.downcast_ref::<Trap>() {
        Some(Trap::Interrupt) => CallError::Timeout,
        _ => CallError::Trap(format!("{e:#}")),
    }
}

impl Host {
    fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    fn db(&self) -> Result<&PluginDb, sql::SqlError> {
        self.db
            .as_deref()
            .ok_or_else(|| sql::SqlError::Other("the sql capability was not granted".into()))
    }
}

impl self::rustrak::plugin::types::Host for Host {}

fn to_store(v: sql::Value) -> store::Value {
    match v {
        sql::Value::Null => store::Value::Null,
        sql::Value::Integer(i) => store::Value::Integer(i),
        sql::Value::Real(f) => store::Value::Real(f),
        sql::Value::Text(s) => store::Value::Text(s),
        sql::Value::Blob(b) => store::Value::Blob(b),
    }
}

fn from_store(v: store::Value) -> sql::Value {
    match v {
        store::Value::Null => sql::Value::Null,
        store::Value::Integer(i) => sql::Value::Integer(i),
        store::Value::Real(f) => sql::Value::Real(f),
        store::Value::Text(s) => sql::Value::Text(s),
        store::Value::Blob(b) => sql::Value::Blob(b),
    }
}

impl From<store::SqlError> for sql::SqlError {
    fn from(e: store::SqlError) -> Self {
        match e {
            store::SqlError::Constraint(m) => sql::SqlError::Constraint(m),
            store::SqlError::DatabaseFull => sql::SqlError::DatabaseFull,
            store::SqlError::Busy => sql::SqlError::Busy,
            store::SqlError::Other(m) => sql::SqlError::Other(m),
        }
    }
}

impl sql::Host for Host {
    fn execute(&mut self, sql: String, params: Vec<sql::Value>) -> Result<u64, sql::SqlError> {
        let params: Vec<_> = params.into_iter().map(to_store).collect();
        Ok(self.db()?.execute(&sql, &params, self.remaining())?)
    }

    fn query(&mut self, sql: String, params: Vec<sql::Value>) -> Result<sql::Rows, sql::SqlError> {
        let params: Vec<_> = params.into_iter().map(to_store).collect();
        let rows = self.db()?.query(&sql, &params, self.remaining())?;
        Ok(sql::Rows {
            columns: rows.columns,
            rows: rows
                .rows
                .into_iter()
                .map(|r| r.into_iter().map(from_store).collect())
                .collect(),
        })
    }

    fn batch(&mut self, statements: Vec<(String, Vec<sql::Value>)>) -> Result<(), sql::SqlError> {
        let statements: Vec<_> = statements
            .into_iter()
            .map(|(s, p)| (s, p.into_iter().map(to_store).collect()))
            .collect();
        Ok(self.db()?.batch(&statements, self.remaining())?)
    }
}

impl settings::Host for Host {
    fn instance(&mut self) -> String {
        self.settings
            .as_ref()
            .map_or_else(|| "{}".into(), |s| s(None))
    }

    fn project(&mut self, project_id: u64) -> String {
        self.settings
            .as_ref()
            .map_or_else(|| "{}".into(), |s| s(Some(project_id)))
    }
}

impl plugin_log::Host for Host {
    fn write(&mut self, level: plugin_log::Level, message: String) {
        let level = match level {
            plugin_log::Level::Debug => log::Level::Debug,
            plugin_log::Level::Info => log::Level::Info,
            plugin_log::Level::Warn => log::Level::Warn,
            plugin_log::Level::Error => log::Level::Error,
        };
        log::log!(target: "rustrak::plugin", level, "[{}] {message}", self.plugin);
    }
}
