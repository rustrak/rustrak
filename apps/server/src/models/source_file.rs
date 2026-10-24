use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Chunk {
    pub checksum: String,
    pub size: i32,
    pub data: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct SourceFile {
    pub id: Uuid,
    pub checksum: String,
    pub size: i32,
    pub storage_path: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct SourceFileMetadata {
    pub id: Uuid,
    pub project_id: i32,
    pub debug_id: Uuid,
    pub file_type: String,
    pub file_id: Uuid,
    pub times_used: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AssemblyJob {
    pub id: i64,
    pub bundle_checksum: String,
    pub project_id: i32,
    // TEXT[] in PG (Vec<String> via FromRow); TEXT in SQLite (JSON-encoded, needs Json wrapper).
    // sqlx does NOT auto-decode JSON from TEXT in SQLite — plain Vec<String> will panic at runtime.
    #[cfg(feature = "postgres")]
    pub chunks: Vec<String>,
    #[cfg(not(feature = "postgres"))]
    pub chunks: sqlx::types::Json<Vec<String>>,
    pub state: String,
    pub detail: Option<String>,
    pub locked_until: Option<DateTime<Utc>>,
    pub worker_id: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// `artifact_bundle` or `dif` ([`DIF_JOB_KIND`]).
    pub kind: String,
    /// A DIF's file name from the assemble request.
    pub name: Option<String>,
    /// A DIF's debug ID from the assemble request.
    pub debug_id: Option<String>,
}

/// `assembly_jobs.kind` of a debug information file.
pub const DIF_JOB_KIND: &str = "dif";

impl AssemblyJob {
    /// Backend-agnostic accessor — use this everywhere instead of `.chunks` directly.
    pub fn chunk_list(&self) -> &[String] {
        #[cfg(feature = "postgres")]
        return &self.chunks;
        #[cfg(not(feature = "postgres"))]
        return &self.chunks.0;
    }
}
