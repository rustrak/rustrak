use rustrak_plugin::prelude::*;
use serde::Deserialize;

use crate::models::hit::{self, Hit};
use crate::Perm;

/// An app sends this one with its project's DSN key.
#[post("/hit", public)]
fn record(ctx: &Ctx, body: Body) -> Result<NoContent> {
    hit::record(ctx, &body.text())?;
    Ok(NoContent)
}

#[get("/count", requires = Perm::Stats.read())]
fn count(ctx: &Ctx) -> Result<String> {
    Ok(hit::count(ctx)?.to_string())
}

#[derive(Deserialize)]
struct Page {
    limit: Option<u32>,
}

#[get("/hits", requires = Perm::Hits.read())]
fn list(ctx: &Ctx, page: Query<Page>) -> Result<Json<Vec<Hit>>> {
    Ok(Json(hit::latest(ctx, page.0.limit.unwrap_or(20).min(100))?))
}

#[derive(Deserialize)]
struct NewHit {
    path: String,
}

/// A hit added by hand, from the dashboard.
#[post("/hits")]
fn add(ctx: &Ctx, body: Json<NewHit>) -> Result<Created<Hit>> {
    let path = body.0.path.trim();
    if path.is_empty() {
        return Err(Error::BadRequest("a path is required".into()));
    }
    Ok(Created(hit::record(ctx, path)?))
}

#[delete("/hits", requires = Perm::Hits.delete())]
fn clear(ctx: &Ctx) -> Result<NoContent> {
    hit::clear(ctx)?;
    Ok(NoContent)
}
