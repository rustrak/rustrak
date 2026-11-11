use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Chunk {
    pub checksum: String,
    pub size: i32,
    pub data: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AssemblyJob {
    pub id: i64,
    pub bundle_checksum: String,
    pub project_id: i32,
    pub state: String,
    pub detail: Option<String>,
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

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AssembleResponse {
    pub state: String,
    #[serde(rename = "missingChunks")]
    pub missing_chunks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SourceMapFile {
    pub debug_id: String,
    pub file_type: String,
    pub size: i32,
    pub times_used: i32,
    pub date_uploaded: String,
}
