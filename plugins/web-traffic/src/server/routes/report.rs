//! The traffic report, for people who hold `web-traffic:traffic:read`;
//! visitors one by one also need `web-traffic:visitors:read`. Every route
//! reads the same narrowing: `?period=` and any dimension
//! (`?country=ES&path=/pricing`).

use std::collections::HashMap;

use rustrak_plugin::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Perm;
use crate::models::report::{self, Report};
use crate::models::stats::{
    self, Count, EventSeries, EventTotals, MAX_ROWS, Occurrence, Overview, Profile, Property,
    Ranked, Row, Step, Visitor,
};

type Params = Query<HashMap<String, String>>;

fn param<'a>(q: &'a Params, key: &str) -> Option<&'a str> {
    q.0.get(key).map(|v| v.trim()).filter(|v| !v.is_empty())
}

fn required<'a>(q: &'a Params, key: &str) -> Result<&'a str> {
    param(q, key).ok_or_else(|| Error::BadRequest(format!("{key} is required")))
}

/// `?limit=`, within what a list may return.
fn limit(q: &Params, default: u32) -> u32 {
    param(q, "limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(default)
        .clamp(1, MAX_ROWS)
}

/// Totals over the window, the window before, and a series to chart.
#[get("/overview", requires = Perm::Traffic.read())]
fn overview(ctx: &Ctx, q: Params) -> Result<Json<Overview>> {
    Ok(Json(stats::overview(ctx, &Report::new(ctx, &q.0)?)?))
}

/// The most common pages, entry and exit pages, referrers, campaigns,
/// browsers... or events; `?q=` keeps those containing it.
#[get("/top", requires = Perm::Traffic.read())]
fn top(ctx: &Ctx, q: Params) -> Result<Json<Vec<Row>>> {
    let report = Report::new(ctx, &q.0)?;
    let (search, limit) = (param(&q, "q"), limit(&q, 10));
    let ranked = match required(&q, "by")? {
        "event" => return Ok(Json(stats::top_events(ctx, &report, search, limit)?)),
        "entry" => Ranked::Entry,
        "exit" => Ranked::Exit,
        other => Ranked::Column(
            report::column(other)
                .ok_or_else(|| Error::BadRequest(format!("{other} is not something to rank by")))?,
        ),
    };
    Ok(Json(stats::top(ctx, &report, ranked, search, limit)?))
}

/// One page's traffic: what an error on it weighs.
#[get("/page", requires = Perm::Traffic.read())]
fn page(ctx: &Ctx, q: Params) -> Result<Json<Count>> {
    let report = Report::new(ctx, &q.0)?;
    Ok(Json(stats::page(ctx, &report, required(&q, "path")?)?))
}

/// How often an event happened, and to how many.
#[get("/event", requires = Perm::Traffic.read())]
fn event(ctx: &Ctx, q: Params) -> Result<Json<EventTotals>> {
    let report = Report::new(ctx, &q.0)?;
    Ok(Json(stats::event_totals(
        ctx,
        &report,
        required(&q, "name")?,
    )?))
}

/// What an event carried: each property and its values.
#[get("/event-properties", requires = Perm::Traffic.read())]
fn event_properties(ctx: &Ctx, q: Params) -> Result<Json<Vec<Property>>> {
    let report = Report::new(ctx, &q.0)?;
    Ok(Json(stats::event_properties(
        ctx,
        &report,
        required(&q, "name")?,
    )?))
}

/// The pages an event happened on.
#[get("/event-pages", requires = Perm::Traffic.read())]
fn event_pages(ctx: &Ctx, q: Params) -> Result<Json<Vec<Row>>> {
    let report = Report::new(ctx, &q.0)?;
    Ok(Json(stats::event_pages(
        ctx,
        &report,
        required(&q, "name")?,
    )?))
}

/// Events over time, by name: the most common few, or `?name=`.
#[get("/event-series", requires = Perm::Traffic.read())]
fn event_series(ctx: &Ctx, q: Params) -> Result<Json<EventSeries>> {
    let report = Report::new(ctx, &q.0)?;
    Ok(Json(stats::event_series(ctx, &report, param(&q, "name"))?))
}

/// The last times an event happened, each with who did it.
#[get("/event-recent", requires = Perm::Visitors.read())]
fn event_recent(ctx: &Ctx, q: Params) -> Result<Json<Vec<Occurrence>>> {
    let report = Report::new(ctx, &q.0)?;
    Ok(Json(stats::occurrences(
        ctx,
        &report,
        required(&q, "name")?,
    )?))
}

/// The window's visitors, most recent first.
#[get("/visitors", requires = Perm::Visitors.read())]
fn visitors(ctx: &Ctx, q: Params) -> Result<Json<Vec<Visitor>>> {
    let report = Report::new(ctx, &q.0)?;
    Ok(Json(stats::visitors(ctx, &report, limit(&q, 100))?))
}

#[derive(Deserialize)]
struct OneVisitor {
    id: String,
}

/// One visitor's day as a profile: where they came from, on what, and how
/// it went.
#[get("/visitor", requires = Perm::Visitors.read())]
fn visitor(ctx: &Ctx, q: Query<OneVisitor>) -> Result<Json<Profile>> {
    let project = ctx.project()?;
    let found = stats::profile(ctx, project, &q.0.id)?;
    Ok(Json(found.ok_or(Error::NotFound)?))
}

/// One visitor's pages and events, in order.
#[get("/journey", requires = Perm::Visitors.read())]
fn journey(ctx: &Ctx, q: Query<OneVisitor>) -> Result<Json<Vec<Step>>> {
    let project = ctx.project()?;
    let steps = stats::journey(ctx, project, &q.0.id)?;
    if steps.is_empty() {
        return Err(Error::NotFound);
    }
    Ok(Json(steps))
}

#[derive(Serialize)]
struct Live {
    visitors: i64,
}

/// Visitors on the site in the last few minutes.
#[get("/live", requires = Perm::Traffic.read())]
fn live(ctx: &Ctx) -> Result<Json<Live>> {
    let project = ctx.project()?;
    Ok(Json(Live {
        visitors: stats::live(ctx, project)?,
    }))
}
