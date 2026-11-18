//! The JSON payloads of `events`, `transactions` and `spans`, compressed by
//! the application (issue #532): Postgres TOAST leaves rows under ~2 KB alone
//! and SQLite compresses nothing.
//!
//! A stored payload is a one-byte format, then:
//!
//! | format | then |
//! |---|---|
//! | 1 | the JSON itself, when compressing did not make it smaller |
//! | 2 | a zstd frame |
//! | 3 | the dictionary's id (4 bytes, little endian), then a zstd frame made with it |
//!
//! Frames are level 3, record their size, and carry neither a checksum nor
//! the dictionary id (the header has it). Anything else is an error, never a
//! guess.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};

use sqlx::Row;
use zstd::dict::{DecoderDictionary, EncoderDictionary};
use zstd::zstd_safe::CParameter;

use crate::db::{Db, DbPool};
use crate::error::{AppError, AppResult};

const RAW: u8 = 1;
const ZSTD: u8 = 2;
const ZSTD_DICTIONARY: u8 = 3;

const LEVEL: i32 = 3;

/// The largest JSON compressed; a bigger one is stored raw. A frame never
/// claims more, so a corrupt one cannot make a read allocate past it.
const MAX_COMPRESSED: usize = 64 * 1024 * 1024;

/// `json` as it is stored, compressed with `dictionary` (its id and its
/// prepared form) when one is given.
pub fn encode(json: &[u8], dictionary: Option<(u32, &EncoderDictionary<'_>)>) -> Vec<u8> {
    Compress::new(dictionary).encode(json)
}

/// One zstd context for many payloads: a 1,000-span transaction makes one,
/// not 1,000.
pub struct Compress<'a> {
    id: Option<u32>,
    /// None when zstd could not make one: every payload is then stored raw.
    compressor: Option<zstd::bulk::Compressor<'a>>,
}

impl<'a> Compress<'a> {
    fn new(dictionary: Option<(u32, &'a EncoderDictionary<'a>)>) -> Self {
        let compressor = || -> std::io::Result<_> {
            let mut compressor = match dictionary {
                Some((_, d)) => zstd::bulk::Compressor::with_prepared_dictionary(d)?,
                None => zstd::bulk::Compressor::new(LEVEL)?,
            };
            compressor.set_parameter(CParameter::ChecksumFlag(false))?;
            compressor.set_parameter(CParameter::ContentSizeFlag(true))?;
            compressor.set_parameter(CParameter::DictIdFlag(false))?;
            Ok(compressor)
        };
        Self {
            id: dictionary.map(|(id, _)| id),
            compressor: compressor().ok(),
        }
    }

    /// `json` as it is stored.
    pub fn encode(&mut self, json: &[u8]) -> Vec<u8> {
        let compressed = match &mut self.compressor {
            Some(c) if json.len() <= MAX_COMPRESSED => c.compress(json).ok(),
            _ => None,
        };
        let mut stored = Vec::new();
        if let Some(frame) = compressed {
            match self.id {
                Some(id) => {
                    stored.push(ZSTD_DICTIONARY);
                    stored.extend_from_slice(&id.to_le_bytes());
                }
                None => stored.push(ZSTD),
            }
            stored.extend_from_slice(&frame);
        }
        if stored.is_empty() || stored.len() > json.len() {
            stored.clear();
            stored.push(RAW);
            stored.extend_from_slice(json);
        }
        stored
    }
}

/// The id of the dictionary `stored` was compressed with, if any.
pub fn dictionary_id(stored: &[u8]) -> AppResult<Option<u32>> {
    Ok(split(stored)?.0)
}

/// The header's dictionary id and the bytes after the header.
fn split(stored: &[u8]) -> AppResult<(Option<u32>, &[u8])> {
    match stored.split_first() {
        Some((&RAW | &ZSTD, rest)) => Ok((None, rest)),
        Some((&ZSTD_DICTIONARY, rest)) if rest.len() >= 4 => {
            let (id, frame) = rest.split_at(4);
            Ok((
                Some(u32::from_le_bytes(id.try_into().expect("4 bytes"))),
                frame,
            ))
        }
        Some((&ZSTD_DICTIONARY, _)) => Err(corrupt("cut short in its header")),
        Some((format, _)) => Err(corrupt(&format!("in unknown format {format}"))),
        None => Err(corrupt("empty")),
    }
}

/// The JSON `stored` holds. `dictionary` is the one its header names, when
/// it names one.
pub fn decode_with(
    stored: &[u8],
    dictionary: Option<&DecoderDictionary<'_>>,
) -> AppResult<Vec<u8>> {
    let (id, body) = split(stored)?;
    if stored[0] == RAW {
        return Ok(body.to_vec());
    }
    let mut decompressor = match (id, dictionary) {
        (None, _) => zstd::bulk::Decompressor::new(),
        (Some(_), Some(d)) => zstd::bulk::Decompressor::with_prepared_dictionary(d),
        (Some(id), None) => return Err(corrupt(&format!("made with unknown dictionary {id}"))),
    }
    .map_err(|e| corrupt(&e.to_string()))?;
    decompressor
        .decompress(body, MAX_COMPRESSED)
        .map_err(|e| corrupt(&e.to_string()))
}

fn corrupt(why: &str) -> AppError {
    AppError::Internal(format!("Stored payload {why}"))
}

/// What a row whose JSON is in `payload` holds in `data`: nothing. NULL on
/// Postgres; SQLite cannot drop a column's NOT NULL without rewriting the
/// table, so there it is JSON `null`, which every JSON operator reads as
/// nothing.
#[cfg(feature = "postgres")]
pub const NO_DATA: Option<sqlx::types::Json<()>> = None;
#[cfg(not(feature = "postgres"))]
pub const NO_DATA: &str = "null";

/// A table whose payloads are stored this way. Each trains its own
/// dictionaries: one trained on another kind of payload keeps little.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Events,
    Transactions,
    Spans,
}

impl Column {
    pub const ALL: [Column; 3] = [Column::Events, Column::Transactions, Column::Spans];

    /// Its table, which is also its `kind` in `payload_dictionaries`.
    pub fn table(self) -> &'static str {
        match self {
            Column::Events => "events",
            Column::Transactions => "transactions",
            Column::Spans => "spans",
        }
    }

    /// Rows in the current format the first dictionary waits for, and the
    /// newest rows a training reads at most. Measured on real SDK payloads:
    /// more events cost RAM (about 25 KB each) and gain nothing past 500.
    fn rows(self) -> (i64, i64) {
        match self {
            Column::Events => (500, 500),
            Column::Transactions => (1_000, 2_000),
            Column::Spans => (5_000, 20_000),
        }
    }

    fn dictionary_size(self) -> usize {
        match self {
            Column::Events => 64 * 1024,
            Column::Transactions | Column::Spans => 32 * 1024,
        }
    }
}

type DbRow = <Db as sqlx::Database>::Row;
type DbConnection = <Db as sqlx::Database>::Connection;

/// Every dictionary decoded with since the process started (about 0.1 MB
/// each), loaded on first use. An id is a hash of the dictionary's bytes
/// (zstd's own), so it names the same dictionary in any database.
static DECODERS: LazyLock<Mutex<HashMap<u32, Arc<DecoderDictionary<'static>>>>> =
    LazyLock::new(Default::default);

/// A dictionary's id and its form prepared for compressing.
type Prepared = (u32, Arc<EncoderDictionary<'static>>);

type ConnectOptions = <DbConnection as sqlx::Connection>::Options;

/// Each column's newest dictionary (about 0.5 MB each, `None` when it has
/// none), per database: read once, then replaced only by [`train`] once its
/// dictionary is committed. One process owns a database, so nothing else
/// adds one; a write never reads `payload_dictionaries` inside its
/// transaction, where a read before the first write could fail with
/// `SQLITE_BUSY_SNAPSHOT` (see `db::begin_write`).
///
/// A database is its pool, known by the connect options every clone of the
/// pool shares. The `Weak` keeps that allocation's address from being reused
/// by another pool (tests open many databases in one process); entries of a
/// dropped pool are pruned.
static ENCODERS: LazyLock<Mutex<Vec<Encoders>>> = LazyLock::new(Default::default);

type Encoders = (Weak<ConnectOptions>, HashMap<Column, Option<Prepared>>);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `pool`'s encoders, created empty.
fn encoders_of<'a>(
    all: &'a mut Vec<Encoders>,
    pool: &DbPool,
) -> &'a mut HashMap<Column, Option<Prepared>> {
    let options = Arc::downgrade(&pool.connect_options());
    all.retain(|(weak, _)| weak.strong_count() > 0);
    let at = match all.iter().position(|(weak, _)| weak.ptr_eq(&options)) {
        Some(at) => at,
        None => {
            all.push((options, HashMap::new()));
            all.len() - 1
        }
    };
    &mut all[at].1
}

/// Empties the registry, as a new process starts with it.
#[cfg(feature = "test-seams")]
pub fn forget_dictionaries() {
    lock(&DECODERS).clear();
    lock(&ENCODERS).clear();
}

async fn dictionary<'e>(db: impl sqlx::Executor<'e, Database = Db>, id: u32) -> AppResult<Vec<u8>> {
    sqlx::query_scalar("SELECT dictionary FROM payload_dictionaries WHERE id = $1")
        .bind(i64::from(id))
        .fetch_optional(db)
        .await?
        .ok_or_else(|| corrupt(&format!("made with unknown dictionary {id}")))
}

async fn decoder(pool: &DbPool, id: u32) -> AppResult<Arc<DecoderDictionary<'static>>> {
    let cached = lock(&DECODERS).get(&id).cloned();
    if let Some(decoder) = cached {
        return Ok(decoder);
    }
    let decoder = Arc::new(DecoderDictionary::copy(&dictionary(pool, id).await?));
    lock(&DECODERS).insert(id, Arc::clone(&decoder));
    Ok(decoder)
}

/// The JSON `stored` holds, loading its dictionary on first use.
pub async fn decode(pool: &DbPool, stored: &[u8]) -> AppResult<Vec<u8>> {
    let dictionary = match dictionary_id(stored)? {
        Some(id) => Some(decoder(pool, id).await?),
        None => None,
    };
    decode_with(stored, dictionary.as_deref())
}

/// Every dictionary of `column`, loaded before its rows are streamed: a
/// stream holds its connection, and a dictionary loaded on first use would
/// wait for another (a pool of one connection waits on itself).
pub async fn decoders(pool: &DbPool, column: Column) -> AppResult<Decoders> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM payload_dictionaries WHERE kind = $1")
        .bind(column.table())
        .fetch_all(pool)
        .await?;
    let mut all = HashMap::new();
    for id in ids {
        if let Ok(id) = u32::try_from(id) {
            all.insert(id, decoder(pool, id).await?);
        }
    }
    Ok(all)
}

pub type Decoders = HashMap<u32, Arc<DecoderDictionary<'static>>>;

/// One zstd context for many stored payloads, decoded one at a time as rows
/// stream in, with the dictionaries [`decoders`] loaded.
pub struct Decompress<'a> {
    dictionaries: &'a Decoders,
    decompressor: zstd::bulk::Decompressor<'a>,
}

impl<'a> Decompress<'a> {
    pub fn new(dictionaries: &'a Decoders) -> AppResult<Self> {
        Ok(Self {
            dictionaries,
            decompressor: zstd::bulk::Decompressor::new().map_err(|e| corrupt(&e.to_string()))?,
        })
    }

    /// The payload `stored` holds, or the part of it `T` names.
    pub fn read<T: serde::de::DeserializeOwned>(&mut self, stored: &[u8]) -> AppResult<T> {
        let (id, body) = split(stored)?;
        let json;
        let bytes = if stored[0] == RAW {
            body
        } else {
            let dictionaries = self.dictionaries;
            match id {
                Some(id) => self.decompressor.set_prepared_dictionary(
                    dictionaries
                        .get(&id)
                        .ok_or_else(|| corrupt(&format!("made with unknown dictionary {id}")))?,
                ),
                // An empty dictionary is none (`ZSTD_DCtx_loadDictionary`).
                None => self.decompressor.set_dictionary(&[]),
            }
            .map_err(|e| corrupt(&e.to_string()))?;
            json = self
                .decompressor
                .decompress(body, MAX_COMPRESSED)
                .map_err(|e| corrupt(&e.to_string()))?;
            &json[..]
        };
        serde_json::from_slice(bytes)
            .map_err(|e| corrupt(&format!("is not the JSON expected: {e}")))
    }
}

/// A column's newest dictionary, if it has one, ready to compress with.
#[derive(Default)]
pub struct Encoder(Option<Prepared>);

impl Encoder {
    /// From memory; read from `pool` the first time only, on a connection
    /// of its own. Take it before the write's transaction begins.
    pub async fn current(pool: &DbPool, column: Column) -> AppResult<Self> {
        let cached = encoders_of(&mut lock(&ENCODERS), pool)
            .get(&column)
            .cloned();
        if let Some(prepared) = cached {
            return Ok(Self(prepared));
        }
        let row: Option<(i64, Vec<u8>)> = sqlx::query_as(
            "SELECT id, dictionary FROM payload_dictionaries WHERE kind = $1 \
             ORDER BY row_count DESC LIMIT 1",
        )
        .bind(column.table())
        .fetch_optional(pool)
        .await?;
        let prepared = row
            .and_then(|(id, bytes)| Some((u32::try_from(id).ok()?, bytes)))
            .map(|(id, bytes)| (id, Arc::new(EncoderDictionary::copy(&bytes, LEVEL))));
        // A training that committed meanwhile already put the newer one.
        let prepared = encoders_of(&mut lock(&ENCODERS), pool)
            .entry(column)
            .or_insert(prepared)
            .clone();
        Ok(Self(prepared))
    }

    /// A context to encode one batch's payloads with.
    pub fn compressor(&self) -> Compress<'_> {
        Compress::new(self.0.as_ref().map(|(id, d)| (*id, &**d)))
    }

    pub fn encode(&self, json: &[u8]) -> Vec<u8> {
        self.compressor().encode(json)
    }
}

/// A payload as its row holds it: compressed in `payload`, or, in a row
/// written before compression, JSON in `data`. Selected with both columns.
#[derive(Debug, Clone)]
pub enum Stored {
    Json(serde_json::Value),
    Encoded(Vec<u8>),
}

impl Default for Stored {
    fn default() -> Self {
        Self::Json(serde_json::Value::Null)
    }
}

impl<'r> sqlx::FromRow<'r, DbRow> for Stored {
    fn from_row(row: &'r DbRow) -> sqlx::Result<Self> {
        Ok(match row.try_get::<Option<Vec<u8>>, _>("payload")? {
            Some(bytes) => Self::Encoded(bytes),
            None => Self::Json(row.try_get("data")?),
        })
    }
}

impl Stored {
    /// The payload, or the part of it `T` names.
    pub async fn read<T: serde::de::DeserializeOwned>(self, pool: &DbPool) -> AppResult<T> {
        match self {
            Self::Json(json) => serde_json::from_value(json),
            Self::Encoded(bytes) => serde_json::from_slice(&decode(pool, &bytes).await?),
        }
        .map_err(|e| corrupt(&format!("is not the JSON expected: {e}")))
    }
}

/// The JSON a training reads at most. Training peaks at about twice this
/// plus zstd's own ~12 MB of tables.
const MAX_SAMPLE_BYTES: usize = 16 * 1024 * 1024;

/// Trains every column's dictionary that is due, off the write path: its
/// first once the newest rows hold `Column::rows` in the current format, a
/// new one once the rows in that format have grown tenfold since the last.
/// Rows written before compression (JSON in `data`) are never learnt from:
/// on an upgraded database they are most of the table, in another shape. A
/// training that fails is logged and tried again next time; writes go on
/// with what they have.
pub async fn train_due(pool: &DbPool) -> AppResult<Vec<(Column, u32)>> {
    let mut trained = Vec::new();
    for column in Column::ALL {
        let rows = estimated_rows(pool, column).await?;
        let (first, last): (Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT MIN(row_count), MAX(row_count) FROM payload_dictionaries WHERE kind = $1",
        )
        .bind(column.table())
        .fetch_one(pool)
        .await?;
        let wait = column.rows().0;
        let due = match (first, last) {
            // The table's rows in the current format, estimated: those
            // written since the first dictionary plus the `wait` it was
            // trained at. Counting them would read the table. Old rows are
            // only ever deleted, which can delay a retraining, never hasten
            // one. On a database that never held old rows and trained at
            // `wait` rows this is the table's estimate itself.
            (Some(first), Some(last)) => {
                let current = |rows: i64| rows.saturating_sub(first).saturating_add(wait);
                current(rows) >= current(last).saturating_mul(10)
            }
            _ => rows >= wait,
        };
        if !due {
            continue;
        }
        match train(pool, column, rows).await {
            // Fewer than `wait` of the newest rows are in the current format.
            Ok(None) => {}
            Ok(Some(id)) => {
                lock(&FAILING).remove(&column);
                trained.push((column, id));
            }
            // Warned once when it starts failing, not every run while it does.
            Err(e) if lock(&FAILING).insert(column) => {
                log::warn!("Payload dictionary for {} not trained: {e}", column.table())
            }
            Err(e) => log::debug!("Payload dictionary for {} not trained: {e}", column.table()),
        }
    }
    Ok(trained)
}

/// Columns whose last training failed. ponytail: one set for the process,
/// not per database; it only decides which failures are logged.
static FAILING: LazyLock<Mutex<HashSet<Column>>> = LazyLock::new(Default::default);

/// A table's rows, cheaply: SQLite's largest rowid (the rows ever written,
/// read off the end of the table's b-tree), Postgres's planner estimate
/// (kept by autovacuum's ANALYZE). Never a count, which reads the table.
async fn estimated_rows(pool: &DbPool, column: Column) -> AppResult<i64> {
    #[cfg(feature = "postgres")]
    let rows = sqlx::query_scalar(
        "SELECT CAST(GREATEST(reltuples, 0) AS BIGINT) FROM pg_class WHERE oid = to_regclass($1)",
    )
    .bind(column.table())
    .fetch_one(pool)
    .await?;
    #[cfg(not(feature = "postgres"))]
    let rows = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COALESCE(MAX(rowid), 0) FROM {}",
        column.table()
    )))
    .fetch_one(pool)
    .await?;
    Ok(rows)
}

async fn train(pool: &DbPool, column: Column, rows: i64) -> AppResult<Option<u32>> {
    let Some((samples, sizes)) = samples(pool, column).await? else {
        return Ok(None);
    };
    let size = column.dictionary_size();
    let dictionary =
        tokio::task::spawn_blocking(move || zstd::dict::from_continuous(&samples, &sizes, size))
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
            .map_err(|e| AppError::Internal(format!("training failed: {e}")))?;
    let id = zstd::zstd_safe::get_dict_id_from_dict(&dictionary)
        .ok_or_else(|| AppError::Internal("trained a dictionary without an id".into()))?
        .get();
    // Committed, or already stored and now measured from these rows, so the
    // next training waits for ten times them. The id hashes the dictionary's
    // content but not its entropy tables (`ZDICT_finalizeDictionary`), so a
    // stored one can differ from these bytes: every write that names the id
    // compresses with the stored bytes, the ones its readers load.
    let stored: Vec<u8> = sqlx::query_scalar(
        "INSERT INTO payload_dictionaries (id, kind, dictionary, row_count, created_at) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (id) DO UPDATE SET row_count = excluded.row_count \
         RETURNING dictionary",
    )
    .bind(i64::from(id))
    .bind(column.table())
    .bind(&dictionary)
    .bind(rows)
    .bind(chrono::Utc::now())
    .fetch_one(pool)
    .await?;
    let prepared = Arc::new(EncoderDictionary::copy(&stored, LEVEL));
    encoders_of(&mut lock(&ENCODERS), pool).insert(column, Some((id, prepared)));
    Ok(Some(id))
}

/// The JSON the encoder saw for the newest rows in the current format, end
/// to end in one buffer (`from_continuous` trains on it without a copy),
/// with each one's size; `None` while fewer than `Column::rows` of them.
/// At most `Column::rows` rows and `MAX_SAMPLE_BYTES`, read one at a time.
///
/// A bounded part of the table is read, however many old rows it holds:
/// SQLite's newest rows by rowid, which every new row extends. Postgres puts
/// a new row wherever its free space map has room, and autovacuum maps the
/// leftover space of every old page, so new rows land all over the heap:
/// there it is `MAX_SAMPLE_BYTES` of pages picked at random. ponytail: a
/// table of mostly old rows waits until a sample finds `Column::rows` new
/// ones (for spans, about a fifth of the table); an index on ingest order
/// would find them sooner but cost every write.
async fn samples(pool: &DbPool, column: Column) -> AppResult<Option<(Vec<u8>, Vec<usize>)>> {
    use futures_util::TryStreamExt;

    let limit = column.rows().1;
    // The rows stream on one connection; a decode must not wait for another.
    decoders(pool, column).await?;
    #[cfg(feature = "postgres")]
    let sql = format!(
        "SELECT payload FROM {table} TABLESAMPLE SYSTEM \
           ((SELECT LEAST(100, 100.0 * $2 / GREATEST(pg_relation_size('{table}'), 1)))) \
         WHERE payload IS NOT NULL ORDER BY ctid DESC LIMIT $1",
        table = column.table()
    );
    #[cfg(not(feature = "postgres"))]
    let sql = format!(
        "SELECT payload FROM (SELECT payload FROM {} ORDER BY rowid DESC LIMIT $1) \
         WHERE payload IS NOT NULL",
        column.table()
    );
    let query = sqlx::query_scalar::<_, Vec<u8>>(sqlx::AssertSqlSafe(sql)).bind(limit);
    #[cfg(feature = "postgres")]
    let query = query.bind(MAX_SAMPLE_BYTES as i64);
    let mut rows = query.fetch(pool);
    let (mut samples, mut sizes, mut read) = (Vec::new(), Vec::new(), 0);
    while let Some(stored) = rows.try_next().await? {
        read += 1;
        let json = decode(pool, &stored).await?;
        if samples.len() + json.len() <= MAX_SAMPLE_BYTES {
            samples.extend_from_slice(&json);
            sizes.push(json.len());
        }
    }
    Ok((read >= column.rows().0).then_some((samples, sizes)))
}
