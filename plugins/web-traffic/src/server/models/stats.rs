//! The traffic report's figures, lists and series, all read through one
//! `Report`.

use rustrak_plugin::ToValue;
use rustrak_plugin::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::report::{Report, Sql};

/// The most a list returns; a request may ask for fewer.
pub const MAX_ROWS: u32 = 500;
/// An event's latest occurrences shown.
const RECENT: u32 = 50;
/// What "online now" means, in seconds.
const LIVE_WINDOW: i64 = 300;
/// The events a chart of all of them draws.
const CHARTED_EVENTS: u32 = 5;
/// A pause this long between two pages starts a new visit, as the visitor
/// page splits a day (`VISIT_GAP` in `src/ui/model.ts`).
const VISIT_GAP: i64 = 30 * 60;

fn query<T: for<'de> Deserialize<'de>>(ctx: &Ctx, sql: &Sql) -> Result<Vec<T>> {
    Ok(ctx.db().query(&sql.text, &sql.params())?)
}

fn one<T: for<'de> Deserialize<'de> + Default>(ctx: &Ctx, sql: &Sql) -> Result<T> {
    Ok(query(ctx, sql)?.into_iter().next().unwrap_or_default())
}

#[derive(Serialize, Deserialize, Default)]
pub struct Totals {
    pub pageviews: i64,
    pub visitors: i64,
    /// Visitors who saw one page and left.
    pub bounces: i64,
    pub visits: i64,
    /// Seconds from each visit's first page to its last, summed.
    pub duration: i64,
}

/// `(SELECT visitor, views, visits, length ...)`: each visitor's day in the
/// report, its time away left out of `length`.
fn days(report: &Report) -> Sql {
    let mut sql = Sql::new(
        "(SELECT visitor, count(*) AS views, sum(gap IS NULL OR gap > ?) AS visits, \
         sum(CASE WHEN gap <= ? THEN gap ELSE 0 END) AS length \
         FROM (SELECT visitor, created_at - lag(created_at) \
         OVER (PARTITION BY visitor ORDER BY created_at, id) AS gap ",
        [VISIT_GAP.to_value(), VISIT_GAP.to_value()],
    );
    sql.append(report.pageviews())
        .push(") GROUP BY visitor)", []);
    sql
}

fn totals(ctx: &Ctx, report: &Report) -> Result<Totals> {
    let mut sql = Sql::new(
        "SELECT coalesce(sum(views), 0) AS pageviews, count(*) AS visitors, \
         coalesce(sum(visits), 0) AS visits, coalesce(sum(views = 1), 0) AS bounces, \
         coalesce(sum(length), 0) AS duration FROM ",
        [],
    );
    sql.append(days(report));
    one(ctx, &sql)
}

#[derive(Serialize, Deserialize)]
pub struct Bucket {
    /// Its start, unix seconds.
    pub t: i64,
    pub pageviews: i64,
    pub visitors: i64,
}

#[derive(Serialize)]
pub struct Overview {
    pub current: Totals,
    /// The window just before, the same length: what the figures compare to.
    pub previous: Totals,
    pub series: Vec<Bucket>,
}

/// Every bucket start of the report's window.
fn buckets(report: &Report) -> impl Iterator<Item = i64> {
    (report.from..report.to).step_by(report.bucket as usize)
}

pub fn overview(ctx: &Ctx, report: &Report) -> Result<Overview> {
    let mut sql = Sql::new(
        "SELECT (created_at / ?) * ? AS t, count(*) AS pageviews, \
         count(DISTINCT visitor) AS visitors ",
        [report.bucket.to_value(), report.bucket.to_value()],
    );
    sql.append(report.pageviews())
        .push(" GROUP BY 1 ORDER BY 1", []);
    // Every bucket, the empty ones too: a chart with gaps reads as a dip.
    let mut found = query::<Bucket>(ctx, &sql)?.into_iter().peekable();
    let series = buckets(report)
        .map(|t| {
            found.next_if(|b| b.t == t).unwrap_or(Bucket {
                t,
                pageviews: 0,
                visitors: 0,
            })
        })
        .collect();
    Ok(Overview {
        current: totals(ctx, report)?,
        previous: totals(ctx, &report.previous())?,
        series,
    })
}

/// What a top list ranks.
#[derive(Clone, Copy)]
pub enum Ranked {
    Column(&'static str),
    /// The first page of each visitor's day.
    Entry,
    /// The last one.
    Exit,
}

#[derive(Serialize, Deserialize)]
pub struct Row {
    pub value: String,
    pub visitors: i64,
    pub count: i64,
    /// Of `visitors`, those who saw one page all day.
    #[serde(default)]
    pub bounces: i64,
    /// Their visits.
    #[serde(default)]
    pub visits: i64,
    /// Their visits' length in seconds, summed.
    #[serde(default)]
    pub duration: i64,
}

/// `AND instr(lower(<column>), lower(?)) > 0`, when there is a search.
fn containing(sql: &mut Sql, column: &str, search: Option<&str>) {
    if let Some(search) = search {
        sql.push(
            &format!(" AND instr(lower({column}), lower(?)) > 0"),
            [search.to_value()],
        );
    }
}

/// The most common values, each with how its visitors' days went.
pub fn top(
    ctx: &Ctx,
    report: &Report,
    ranked: Ranked,
    search: Option<&str>,
    limit: u32,
) -> Result<Vec<Row>> {
    let mut values = match ranked {
        Ranked::Column(column) => {
            let mut sql = Sql::new(&format!("SELECT {column} AS value, visitor "), []);
            sql.append(report.pageviews())
                .push(&format!(" AND {column} IS NOT NULL"), []);
            sql
        }
        Ranked::Entry | Ranked::Exit => {
            let order = if matches!(ranked, Ranked::Entry) {
                "ASC"
            } else {
                "DESC"
            };
            let mut sql = Sql::new(
                &format!(
                    "SELECT value, visitor FROM (SELECT path AS value, visitor, row_number() \
                     OVER (PARTITION BY visitor ORDER BY created_at {order}, id {order}) AS nth "
                ),
                [],
            );
            sql.append(report.pageviews()).push(") WHERE nth = 1", []);
            sql
        }
    };
    containing(&mut values, "value", search);
    let mut sql = Sql::new(
        "SELECT value, count(*) AS visitors, sum(n) AS count, \
         sum(day.views = 1) AS bounces, sum(day.visits) AS visits, \
         sum(day.length) AS duration \
         FROM (SELECT value, visitor, count(*) AS n FROM (",
        [],
    );
    sql.append(values)
        .push(") GROUP BY value, visitor) JOIN ", [])
        .append(days(report))
        .push(
            " day USING (visitor) GROUP BY value \
             ORDER BY visitors DESC, count DESC, value LIMIT ?",
            [limit.to_value()],
        );
    query(ctx, &sql)
}

pub fn top_events(
    ctx: &Ctx,
    report: &Report,
    search: Option<&str>,
    limit: u32,
) -> Result<Vec<Row>> {
    let mut sql = Sql::new(
        "SELECT name AS value, count(DISTINCT visitor) AS visitors, count(*) AS count \
         FROM events WHERE ",
        [],
    );
    sql.append(report.events());
    containing(&mut sql, "name", search);
    sql.push(
        " GROUP BY name ORDER BY count DESC, name LIMIT ?",
        [limit.to_value()],
    );
    query(ctx, &sql)
}

/// `FROM events WHERE <report> AND name = ?`.
fn one_event(report: &Report, name: &str) -> Sql {
    let mut sql = Sql::new("FROM events WHERE ", []);
    sql.append(report.events())
        .push(" AND events.name = ?", [name.to_value()]);
    sql
}

#[derive(Serialize, Deserialize, Default)]
pub struct EventTotals {
    pub count: i64,
    pub visitors: i64,
    /// Every visitor of the window, by page or by event: what a conversion
    /// is out of.
    pub everyone: i64,
}

pub fn event_totals(ctx: &Ctx, report: &Report, name: &str) -> Result<EventTotals> {
    let mut sql = Sql::new(
        "SELECT count(*) AS count, count(DISTINCT visitor) AS visitors, \
         (SELECT count(*) FROM (SELECT visitor ",
        [],
    );
    sql.append(report.pageviews())
        .push(" UNION SELECT visitor FROM events WHERE ", [])
        .append(report.events())
        .push(")) AS everyone ", [])
        .append(one_event(report, name));
    one(ctx, &sql)
}

#[derive(Serialize, Deserialize)]
pub struct Property {
    pub key: String,
    pub value: String,
    pub count: i64,
}

/// What one event carried: each property's values and how often. A JSON
/// `null` reads as `null`.
pub fn event_properties(ctx: &Ctx, report: &Report, name: &str) -> Result<Vec<Property>> {
    let mut sql = Sql::new(
        "SELECT p.key AS key, coalesce(CAST(p.value AS TEXT), 'null') AS value, count(*) AS count \
         FROM events, json_each(events.data) AS p WHERE ",
        [],
    );
    sql.append(report.events())
        .push(" AND events.name = ?", [name.to_value()])
        .push(
            " GROUP BY 1, 2 ORDER BY 1, count DESC, 2 LIMIT ?",
            [MAX_ROWS.to_value()],
        );
    query(ctx, &sql)
}

/// The pages an event happened on.
pub fn event_pages(ctx: &Ctx, report: &Report, name: &str) -> Result<Vec<Row>> {
    let mut sql = Sql::new(
        "SELECT path AS value, count(DISTINCT visitor) AS visitors, count(*) AS count ",
        [],
    );
    sql.append(one_event(report, name))
        .push(" GROUP BY path ORDER BY count DESC, path LIMIT 10", []);
    query(ctx, &sql)
}

#[derive(Serialize, Deserialize)]
pub struct Occurrence {
    pub at: i64,
    pub path: String,
    pub visitor: String,
    pub data: Option<String>,
}

/// The last times an event happened, newest first.
pub fn occurrences(ctx: &Ctx, report: &Report, name: &str) -> Result<Vec<Occurrence>> {
    let mut sql = Sql::new("SELECT created_at AS at, path, visitor, data ", []);
    sql.append(one_event(report, name)).push(
        " ORDER BY created_at DESC, id DESC LIMIT ?",
        [RECENT.to_value()],
    );
    query(ctx, &sql)
}

#[derive(Deserialize)]
struct EventBucket {
    t: i64,
    name: String,
    count: i64,
}

#[derive(Serialize)]
pub struct EventSeries {
    pub names: Vec<String>,
    /// One per bucket of the window, the empty ones too: `{ t, "0": n, ... }`,
    /// each count keyed by its name's place in `names`, so an event named `t`
    /// cannot overwrite the time.
    pub rows: Vec<Map<String, Value>>,
}

/// Events over the window, by name: the most common few, or `name`.
pub fn event_series(ctx: &Ctx, report: &Report, name: Option<&str>) -> Result<EventSeries> {
    let names: Vec<String> = match name {
        Some(name) => vec![name.to_string()],
        None => top_events(ctx, report, None, CHARTED_EVENTS)?
            .into_iter()
            .map(|r| r.value)
            .collect(),
    };
    let mut sql = Sql::new(
        "SELECT (events.created_at / ?) * ? AS t, events.name AS name, count(*) AS count \
         FROM events WHERE ",
        [report.bucket.to_value(), report.bucket.to_value()],
    );
    // Only the names charted: SQLite takes `IN ()` as matching nothing.
    let marks = vec!["?"; names.len()].join(", ");
    sql.append(report.events()).push(
        &format!(" AND events.name IN ({marks}) GROUP BY 1, 2"),
        names.iter().map(|n| n.to_value()),
    );
    let found = query::<EventBucket>(ctx, &sql)?;
    let rows = buckets(report)
        .map(|t| {
            let mut row = Map::from_iter([("t".to_string(), Value::from(t))]);
            for (i, name) in names.iter().enumerate() {
                let count = found
                    .iter()
                    .find(|b| b.t == t && &b.name == name)
                    .map_or(0, |b| b.count);
                row.insert(i.to_string(), count.into());
            }
            row
        })
        .collect();
    Ok(EventSeries { names, rows })
}

#[derive(Serialize, Deserialize, Default)]
pub struct Count {
    pub pageviews: i64,
    pub visitors: i64,
}

/// One page's traffic over a window: what an error there weighs.
pub fn page(ctx: &Ctx, report: &Report, path: &str) -> Result<Count> {
    let mut sql = Sql::new(
        "SELECT count(*) AS pageviews, count(DISTINCT visitor) AS visitors ",
        [],
    );
    sql.append(report.pageviews())
        .push(" AND path = ?", [path.to_value()]);
    one(ctx, &sql)
}

#[derive(Serialize, Deserialize)]
pub struct Visitor {
    pub id: String,
    pub first: i64,
    pub last: i64,
    pub pageviews: i64,
    pub events: i64,
    pub entry: String,
    pub referrer: Option<String>,
    pub country: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub device: Option<String>,
}

/// The window's visitors, most recent first: who they are, as far as a
/// report may know, and what they did.
pub fn visitors(ctx: &Ctx, report: &Report, limit: u32) -> Result<Vec<Visitor>> {
    let mut sql = Sql::new(
        "SELECT v.visitor AS id, v.first, v.last, v.pageviews, \
         (SELECT count(*) FROM events e WHERE e.project_id = ? AND e.visitor = v.visitor \
            AND e.created_at >= ? AND e.created_at < ?) AS events, \
         f.path AS entry, f.referrer, f.country, f.browser, f.os, f.device \
         FROM (SELECT visitor, min(created_at) AS first, max(created_at) AS last, \
            count(*) AS pageviews, min(id) AS first_id ",
        [
            report.project().to_value(),
            report.from.to_value(),
            report.to.to_value(),
        ],
    );
    sql.append(report.pageviews()).push(
        " GROUP BY visitor) v JOIN pageviews f ON f.id = v.first_id \
         ORDER BY v.last DESC, v.visitor LIMIT ?",
        [limit.to_value()],
    );
    query(ctx, &sql)
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    Pageview,
    Event,
}

#[derive(Serialize, Deserialize)]
pub struct Step {
    pub kind: StepKind,
    pub at: i64,
    pub path: String,
    /// A page's title, or an event's name.
    pub label: Option<String>,
    /// An event's data, as JSON.
    pub data: Option<String>,
}

/// One visitor's day: every page and event, in order.
pub fn journey(ctx: &Ctx, project: u64, visitor: &str) -> Result<Vec<Step>> {
    Ok(ctx.db().query(
        "SELECT kind, at, path, label, data FROM ( \
           SELECT 'pageview' AS kind, created_at AS at, path, title AS label, NULL AS data, id \
           FROM pageviews WHERE project_id = ?1 AND visitor = ?2 \
           UNION ALL SELECT 'event', created_at, path, name, data, id \
           FROM events WHERE project_id = ?1 AND visitor = ?2) \
         ORDER BY at, kind DESC, id LIMIT ?3",
        params![project, visitor, MAX_ROWS],
    )?)
}

#[derive(Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub first: i64,
    pub last: i64,
    pub pageviews: i64,
    pub events: i64,
    /// Distinct pages read.
    pub pages: i64,
    pub entry: String,
    pub exit: String,
    pub referrer: Option<String>,
    pub source: Option<String>,
    pub campaign: Option<String>,
    pub country: Option<String>,
    pub language: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub device: Option<String>,
}

/// Everything known of one visitor's day, from their first page.
pub fn profile(ctx: &Ctx, project: u64, visitor: &str) -> Result<Option<Profile>> {
    let rows: Vec<Profile> = ctx.db().query(
        "SELECT f.visitor AS id, a.first, a.last, a.pageviews, a.pages, \
         (SELECT count(*) FROM events WHERE project_id = ?1 AND visitor = ?2) AS events, \
         f.path AS entry, \
         (SELECT path FROM pageviews WHERE project_id = ?1 AND visitor = ?2 \
            ORDER BY created_at DESC, id DESC LIMIT 1) AS exit, \
         f.referrer, f.utm_source AS source, f.utm_campaign AS campaign, f.country, \
         f.language, f.browser, f.os, f.device \
         FROM (SELECT min(id) AS first_id, min(created_at) AS first, max(created_at) AS last, \
            count(*) AS pageviews, count(DISTINCT path) AS pages \
            FROM pageviews WHERE project_id = ?1 AND visitor = ?2) a \
         JOIN pageviews f ON f.id = a.first_id",
        params![project, visitor],
    )?;
    if rows.is_empty() {
        // Someone who only sent events (the tracker's auto-track off): what
        // their events say, and nothing of a first page.
        let rows: Vec<Profile> = ctx.db().query(
            "SELECT visitor AS id, min(created_at) AS first, max(created_at) AS last, \
                 0 AS pageviews, count(*) AS events, 0 AS pages, \
                 (SELECT path FROM events WHERE project_id = ?1 AND visitor = ?2 \
                    ORDER BY created_at, id LIMIT 1) AS entry, \
                 (SELECT path FROM events WHERE project_id = ?1 AND visitor = ?2 \
                    ORDER BY created_at DESC, id DESC LIMIT 1) AS exit \
                 FROM events WHERE project_id = ?1 AND visitor = ?2 HAVING count(*) > 0",
            params![project, visitor],
        )?;
        return Ok(rows.into_iter().next());
    }
    Ok(rows.into_iter().next())
}

/// Visitors in the last few minutes.
pub fn live(ctx: &Ctx, project: u64) -> Result<i64> {
    Ok(ctx.db().scalar(
        "SELECT count(DISTINCT visitor) FROM pageviews \
         WHERE project_id = ?1 AND created_at >= unixepoch() - ?2",
        params![project, LIVE_WINDOW],
    )?)
}
