use serde::{Deserialize, Serialize};

/// Request body for a cleanup (preview or execute): remove data older than
/// `older_than_days`, optionally scoped to a single project (omit for all).
///
/// `older_than_days` must be at least 1 — the service rejects smaller values,
/// since a window of 0 (or negative) would move the cutoff to now/the future and
/// purge the entire dataset.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CleanupRequest {
    #[cfg_attr(feature = "openapi", schema(minimum = 1))]
    #[schemars(description = "Delete data older than this many days")]
    pub older_than_days: i64,
    #[serde(default)]
    #[schemars(description = "Scope to one project (omit for all projects)")]
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
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CleanupCounts {
    pub events: i64,
    pub transactions: i64,
    pub spans: i64,
    pub logs: i64,
    pub issues_removed: i64,
    /// Unused source maps deleted; only a retention cleanup removes them.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(required))]
    pub source_maps: i64,
}

impl std::ops::AddAssign for CleanupCounts {
    fn add_assign(&mut self, other: Self) {
        self.events += other.events;
        self.transactions += other.transactions;
        self.spans += other.spans;
        self.logs += other.logs;
        self.issues_removed += other.issues_removed;
        self.source_maps += other.source_maps;
    }
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

/// What the job is doing: deleting old data, or compacting the SQLite file.
#[derive(Debug, Default, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum CleanupKind {
    #[default]
    Cleanup,
    Compaction,
}

/// The cleanup job as the API reports it. Kept in memory: a restart forgets
/// it, and a run cut short by one is simply run again, since every batch it
/// committed is already gone.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CleanupStatus {
    pub state: CleanupState,
    pub kind: CleanupKind,
    /// Rows removed so far while `running`; the total once it has finished.
    pub removed: CleanupCounts,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Why a `failed` run stopped. Batches committed before it stay deleted.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub error: Option<String>,
}

/// Whether the SQLite file can give the space a cleanup frees back to the
/// disk, and what compacting it once would take.
///
/// A database created before Rustrak set `auto_vacuum = INCREMENTAL` keeps
/// SQLite's default, `NONE`: its freed pages are reused but the file never
/// shrinks. One `VACUUM` rewrites it in incremental mode. Always
/// `needed: false` on Postgres, whose autovacuum gives the space back.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Compaction {
    pub needed: bool,
    /// The database file's size.
    pub file_bytes: i64,
    /// What the data in it takes, the part a compaction keeps.
    pub live_bytes: i64,
    /// Free pages inside the file, the part a compaction gives back.
    pub reclaimable_bytes: i64,
    /// Free disk a compaction needs: twice the live data, one copy in
    /// SQLite's temporary file and one in the write-ahead log.
    pub required_disk_bytes: i64,
    /// Free disk next to the database, `null` when it cannot be read.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub free_disk_bytes: Option<i64>,
}

impl Compaction {
    /// Whether the disk can take the compaction. An unreadable free space
    /// lets it run: SQLite fails it cleanly if the disk fills.
    pub fn has_room(&self) -> bool {
        self.free_disk_bytes
            .is_none_or(|free| free >= self.required_disk_bytes)
    }
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
    /// `events_bytes + transactions_bytes + logs_bytes`.
    pub estimated_bytes: i64,
    /// Events with their attachments.
    pub events_bytes: i64,
    /// Transactions with their spans.
    pub transactions_bytes: i64,
    pub logs_bytes: i64,
}

/// Upper bounds, in days, of the age buckets: under 7 days, 7–14, 14–30,
/// 30–90, 90–180, 180–365 and over a year. They are the retention presets, so
/// what a preset deletes is whole buckets.
pub const AGE_BUCKET_DAYS: [i64; 6] = [7, 14, 30, 90, 180, 365];

/// How long ago a kind of data arrived: rows per age bucket, and the oldest.
#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KindAge {
    /// One count per bucket, `AGE_BUCKET_DAYS.len() + 1` of them.
    pub counts: Vec<i64>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub oldest: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectAge {
    pub project_id: i32,
    pub events: KindAge,
    /// Transactions; their spans are as old.
    pub transactions: KindAge,
    pub logs: KindAge,
}

/// The age of everything the instance holds, by project and kind, keyed on
/// ingest time like retention.
#[derive(Debug, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StorageAge {
    /// The buckets' upper bounds in days; the last bucket has none.
    pub bucket_days: Vec<i64>,
    pub projects: Vec<ProjectAge>,
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

/// How long each kind of data is kept, in days. `None` keeps it forever.
#[derive(
    Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema,
)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct RetentionWindows {
    /// Error events, and the issues they leave empty.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub events_days: Option<i64>,
    /// Transactions with their spans.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub transactions_days: Option<i64>,
    #[cfg_attr(feature = "openapi", schema(required))]
    #[schemars(description = "Logs")]
    pub logs_days: Option<i64>,
}

impl RetentionWindows {
    /// Whether anything is ever deleted.
    pub fn deletes_anything(&self) -> bool {
        self.events_days.is_some() || self.transactions_days.is_some() || self.logs_days.is_some()
    }

    /// One `(days, filter)` per kind with a window: each kind has its own
    /// cutoff, so each is its own purge.
    pub fn steps(&self) -> impl Iterator<Item = (i64, CleanupFilter)> {
        let only = |events, transactions, logs| CleanupFilter {
            include_events: events,
            include_transactions: transactions,
            include_logs: logs,
        };
        [
            self.events_days.map(|d| (d, only(true, false, false))),
            self.transactions_days
                .map(|d| (d, only(false, true, false))),
            self.logs_days.map(|d| (d, only(false, false, true))),
        ]
        .into_iter()
        .flatten()
    }

    fn validate(&self) -> crate::error::AppResult<()> {
        use crate::error::FieldErrorCode;
        for (field, days) in [
            ("events_days", self.events_days),
            ("transactions_days", self.transactions_days),
            ("logs_days", self.logs_days),
        ] {
            if let Some(days) = days {
                validate_retention(days)
                    .map_err(|e| e.with_field(field, FieldErrorCode::Invalid))?;
            }
        }
        Ok(())
    }
}

/// A project that keeps its data for other windows than the defaults. Every
/// kind is set: an override replaces the defaults whole.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectRetention {
    pub project_id: i32,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub events_days: Option<i64>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub transactions_days: Option<i64>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub logs_days: Option<i64>,
}

impl ProjectRetention {
    pub fn windows(&self) -> RetentionWindows {
        RetentionWindows {
            events_days: self.events_days,
            transactions_days: self.transactions_days,
            logs_days: self.logs_days,
        }
    }
}

/// How often the cleanup that enforces retention runs.
#[derive(
    Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum CleanupFrequency {
    #[default]
    Daily,
    Weekly,
}

/// Retention: how long each kind of data is kept, by default and per
/// project, and when the cleanup that enforces it runs. Stored as the
/// settings of the `storage_cleanup` scheduled job; a field left out takes
/// its default. Keeping everything forever runs nothing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
#[schemars(extend("required" = ["defaults", "overrides", "frequency", "hour", "weekday"]))]
pub struct RetentionSettings {
    #[cfg_attr(feature = "openapi", schema(required))]
    #[schemars(description = "How long every project keeps each kind; null is forever")]
    pub defaults: RetentionWindows,
    #[cfg_attr(feature = "openapi", schema(required))]
    #[schemars(description = "Projects kept for other windows; each replaces the defaults whole")]
    pub overrides: Vec<ProjectRetention>,
    #[cfg_attr(feature = "openapi", schema(required))]
    #[schemars(description = "How often it runs: daily or weekly")]
    pub frequency: CleanupFrequency,
    /// Hour of the day it runs, UTC.
    #[cfg_attr(feature = "openapi", schema(minimum = 0, maximum = 23, required))]
    pub hour: u8,
    /// Day a weekly cleanup runs, 0 = Sunday.
    #[cfg_attr(feature = "openapi", schema(minimum = 0, maximum = 6, required))]
    pub weekday: u8,
}

impl Default for RetentionSettings {
    /// Everything kept forever; once a window is set, daily at 03:00 UTC.
    fn default() -> Self {
        Self {
            defaults: RetentionWindows::default(),
            overrides: Vec::new(),
            frequency: CleanupFrequency::Daily,
            hour: 3,
            weekday: 0,
        }
    }
}

impl RetentionSettings {
    /// Whether these settings can run, naming the field that can't.
    pub fn validate(&self) -> crate::error::AppResult<()> {
        use crate::error::{AppError, FieldErrorCode};
        let invalid = |message: &str, field: &str| {
            Err(AppError::Validation(message.to_string())
                .with_field(field, FieldErrorCode::Invalid))
        };
        self.defaults.validate()?;
        let mut seen = std::collections::HashSet::new();
        for project in &self.overrides {
            if !seen.insert(project.project_id) {
                return invalid("A project can have only one override", "overrides");
            }
            project
                .windows()
                .validate()
                .map_err(|e| e.with_field("overrides", FieldErrorCode::Invalid))?;
        }
        if self.hour > 23 {
            return invalid("hour must be between 0 and 23", "hour");
        }
        if self.weekday > 6 {
            return invalid("weekday must be between 0 (Sunday) and 6", "weekday");
        }
        Ok(())
    }

    /// Whether any project ever loses data, so the cleanup needs to run.
    pub fn deletes_anything(&self) -> bool {
        self.defaults.deletes_anything()
            || self
                .overrides
                .iter()
                .any(|o| o.windows().deletes_anything())
    }

    /// The windows a project keeps its data for.
    pub fn windows_for(&self, project_id: i32) -> RetentionWindows {
        self.overrides
            .iter()
            .find(|o| o.project_id == project_id)
            .map(ProjectRetention::windows)
            .unwrap_or(self.defaults)
    }

    /// When the cleanup runs, in UTC.
    pub fn schedule(&self) -> crate::models::schedule::Schedule {
        let value = match self.frequency {
            CleanupFrequency::Daily => format!("0 {} * * *", self.hour),
            CleanupFrequency::Weekly => format!("0 {} * * {}", self.hour, self.weekday),
        };
        crate::models::schedule::Schedule::Crontab { value }
    }
}

/// How the last retention cleanup went.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RetentionRun {
    pub started_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    /// `running`, `ok` or `error`.
    pub status: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub removed: Option<CleanupCounts>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RetentionResponse {
    pub settings: RetentionSettings,
    /// `null` while everything is kept forever, and for up to a minute after
    /// the settings change.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub last_run: Option<RetentionRun>,
}

/// Smallest accepted retention window. A value below this would push the cutoff
/// to "now" or into the future, turning the cleanup into a full data wipe — so
/// it's rejected before any cutoff is computed. Mirrors the `min(1)` the MCP and
/// UI already enforce on the client side.
const MIN_RETENTION_DAYS: i64 = 1;
/// Largest accepted window, a century: far past any real retention, and far
/// below where `Duration::days` panics.
const MAX_RETENTION_DAYS: i64 = 36_500;

/// Rejects retention windows that would delete current/future data, or that
/// no cutoff can be computed for. Shared by the preview, execute and
/// automatic paths so all fail fast on the same contract.
pub(crate) fn validate_retention(older_than_days: i64) -> crate::error::AppResult<()> {
    if older_than_days < MIN_RETENTION_DAYS {
        return Err(crate::error::AppError::Validation(format!(
            "older_than_days must be at least {MIN_RETENTION_DAYS}"
        )));
    }
    if older_than_days > MAX_RETENTION_DAYS {
        return Err(crate::error::AppError::Validation(format!(
            "older_than_days must be at most {MAX_RETENTION_DAYS}"
        )));
    }
    Ok(())
}
