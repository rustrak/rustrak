//! A page view counted by the plugin.

use rustrak_plugin::prelude::*;
use serde::Serialize;

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub id: i64,
    pub path: String,
    pub created_at: i64,
}

/// Records a hit in the request's project and returns it, its path cut to
/// the project's `maxPathLength`.
pub fn record(ctx: &Ctx, path: &str) -> Result<Hit> {
    let longest = crate::Settings::get(ctx)?.max_path_length.max(0) as usize;
    let path: String = path.chars().take(longest).collect();
    ctx.db().execute(
        "INSERT INTO hits (project_id, path, created_at) VALUES (?1, ?2, unixepoch())",
        params![ctx.project_id(), path],
    )?;
    let mut hits = ctx.db().query(
        "SELECT id, path, created_at AS \"createdAt\" FROM hits WHERE id = last_insert_rowid()",
        params![],
    )?;
    hits.pop()
        .ok_or_else(|| Error::Internal("the hit was not stored".into()))
}

pub fn count(ctx: &Ctx) -> Result<i64> {
    Ok(ctx.db().scalar(
        "SELECT count(*) FROM hits WHERE project_id = ?1",
        params![ctx.project_id()],
    )?)
}

/// The project's latest hits, newest first.
pub fn latest(ctx: &Ctx, limit: u32) -> Result<Vec<Hit>> {
    Ok(ctx.db().query(
        "SELECT id, path, created_at AS \"createdAt\" FROM hits \
         WHERE project_id = ?1 ORDER BY id DESC LIMIT ?2",
        params![ctx.project_id(), limit],
    )?)
}

pub fn clear(ctx: &Ctx) -> Result<()> {
    ctx.db().execute(
        "DELETE FROM hits WHERE project_id = ?1",
        params![ctx.project_id()],
    )?;
    Ok(())
}
