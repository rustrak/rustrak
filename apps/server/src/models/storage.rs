use serde::{Deserialize, Serialize};

/// Request body for a cleanup (preview or execute): remove data older than
/// `older_than_days`, optionally scoped to a single project (omit for all).
///
/// `older_than_days` must be at least 1 — the service rejects smaller values,
/// since a window of 0 (or negative) would move the cutoff to now/the future and
/// purge the entire dataset.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CleanupRequest {
    #[cfg_attr(feature = "openapi", schema(minimum = 1))]
    pub older_than_days: i64,
    #[serde(default)]
    pub project_id: Option<i32>,
    /// Whether to purge error events (and the issues they empty). Defaults to
    /// `true` so an older client that omits it keeps the "delete everything"
    /// behaviour.
    #[serde(default = "default_true")]
    pub include_events: bool,
    /// Whether to purge transactions (and their cascaded spans). Defaults to `true`.
    #[serde(default = "default_true")]
    pub include_transactions: bool,
    /// Whether to purge logs. Defaults to `true`.
    #[serde(default = "default_true")]
    pub include_logs: bool,
}

/// serde default for the `include_*` flags: an omitted flag means "yes, include
/// this category", preserving the pre-filter contract for older callers.
fn default_true() -> bool {
    true
}

impl CleanupRequest {
    /// The data-category selection this request describes.
    pub fn filter(&self) -> CleanupFilter {
        CleanupFilter {
            include_events: self.include_events,
            include_transactions: self.include_transactions,
            include_logs: self.include_logs,
        }
    }
}

/// Which data categories a cleanup acts on. Lets an admin reclaim one kind of
/// data (e.g. logs) without touching the others. `spans` are governed by
/// `include_transactions` (they cascade from their parent transaction) and
/// `issues_removed` only happens when `include_events` is set — a transaction- or
/// log-only purge never empties an issue.
#[derive(Debug, Clone, Copy)]
pub struct CleanupFilter {
    pub include_events: bool,
    pub include_transactions: bool,
    pub include_logs: bool,
}

impl CleanupFilter {
    /// Every category — the historical "delete everything older than the cutoff".
    pub fn all() -> Self {
        Self {
            include_events: true,
            include_transactions: true,
            include_logs: true,
        }
    }
}

/// Instance-wide storage summary: what Rustrak is holding right now.
///
/// Row counts are exact (indexed `COUNT(*)`). `total_db_size_bytes` is a
/// best-effort figure from the backend (`pg_database_size` / SQLite page count),
/// while `source_maps` is exact (summed `size` columns).
#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StorageSummary {
    /// Whole-database size in bytes (best-effort, backend-reported).
    pub total_db_size_bytes: i64,
    pub events_count: i64,
    pub transactions_count: i64,
    pub spans_count: i64,
    pub logs_count: i64,
    /// Exact source-map weight + file count.
    pub source_maps: SourceMapStorage,
}

/// Rows affected by a cleanup, used symmetrically for the dry-run preview ("what
/// would be removed") and the executed result ("what was removed"). `spans` are
/// counted directly even though they cascade from `transactions`, so the UI can
/// show the true row impact. `issues_removed` is the number of issues that would
/// be left with zero events and therefore deleted.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CleanupCounts {
    pub events: i64,
    pub transactions: i64,
    pub spans: i64,
    pub logs: i64,
    pub issues_removed: i64,
}

/// Where the instance's cleanup job is. There is at most one at a time.
#[derive(Debug, Default, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum CleanupState {
    /// No cleanup has run since the server started.
    #[default]
    Idle,
    Running,
    Completed,
    Failed,
}

/// The cleanup job as the API reports it. Kept in memory: a restart forgets
/// it, and a run cut short by one is simply run again, since every batch it
/// committed is already gone.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CleanupStatus {
    pub state: CleanupState,
    /// Rows removed so far while `running`; the total once it has finished.
    pub removed: CleanupCounts,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Why a `failed` run stopped. Batches committed before it stay deleted.
    pub error: Option<String>,
}

/// Outcome of a source-map garbage collection: orphaned `source_file` rows
/// (no `source_file_metadata` referencing them) removed from the DB and unlinked
/// from disk. `bytes_freed` is the exact summed `size` of the removed files.
#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceMapGcResult {
    pub files_removed: i64,
    pub bytes_freed: i64,
}

/// Per-project storage breakdown, one row per project (including empty ones).
///
/// Counts are exact. `estimated_bytes` estimates the JSON payload length this
/// project holds across events, transactions, spans and logs: a sample of its
/// newest rows per table (spans, which have no ingest time, are sampled in no
/// particular order), scaled by the row count. It is exact for a project with
/// no more rows than the sample.
#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectStorage {
    pub project_id: i32,
    pub project_name: String,
    pub events_count: i64,
    pub transactions_count: i64,
    pub spans_count: i64,
    pub logs_count: i64,
    pub source_maps_count: i64,
    pub estimated_bytes: i64,
}

/// Exact source-map storage weight, summed from the `size` columns.
///
/// Source maps live in two places: `chunk` holds the raw upload bytes as in-DB
/// BYTEA, `source_file` holds the assembled artifact on the filesystem CAS. Both
/// carry a `size` column, so the weight is an exact SUM — no filesystem walk.
#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceMapStorage {
    /// Bytes held in the in-DB `chunk` table (raw upload chunks).
    pub chunk_bytes: i64,
    /// Bytes held on disk via `source_file` (assembled artifacts).
    pub source_file_bytes: i64,
    /// `chunk_bytes + source_file_bytes`.
    pub total_bytes: i64,
    /// Number of `source_file` rows.
    pub file_count: i64,
}
