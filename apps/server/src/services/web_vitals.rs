//! Web Vitals: the performance score of a page load or an interaction.
//!
//! A port of Relay's `normalize_performance_score`
//! (relay-event-normalization/src/event.rs, b95381eca) with the profiles
//! Sentry sends it (`src/sentry/relay/config/__init__.py`, 7543d2ed).

use crate::db::dialect::{bucket, bucket_time, since_utc};
use crate::db::{Db, DbPool};
use crate::error::AppResult;
use crate::models::web_vital::{
    PercentileValue, RatingCounts, VitalStats, WebVitalSample, WebVitalTarget,
    WebVitalsBreakdownRow, WebVitalsPage, WebVitalsPoint, WebVitalsSummary,
};
use crate::protocol::clamp;
use crate::services::scope::bucket_hours;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::QueryBuilder;
use std::collections::BTreeMap;
use std::f64::consts::SQRT_2;
use uuid::Uuid;

/// The vitals a performance score is made of.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Vital {
    Fcp,
    Lcp,
    Cls,
    Ttfb,
    Inp,
}

impl Vital {
    /// The three Core Web Vitals first.
    pub const ALL: [Vital; 5] = [Vital::Lcp, Vital::Inp, Vital::Cls, Vital::Fcp, Vital::Ttfb];

    /// Its measurement's name, which is also how it is stored.
    pub fn name(self) -> &'static str {
        match self {
            Vital::Fcp => "fcp",
            Vital::Lcp => "lcp",
            Vital::Cls => "cls",
            Vital::Ttfb => "ttfb",
            Vital::Inp => "inp",
        }
    }

    pub fn parse(name: &str) -> Option<Vital> {
        Vital::ALL.into_iter().find(|vital| vital.name() == name)
    }

    /// Its weight in a performance score made of every vital
    /// (`WEB_VITALS_PERFORMANCE_SCORE_WEIGHTS`, src/sentry/search/events/constants.py).
    pub fn weight(self) -> f64 {
        match self {
            Vital::Lcp | Vital::Inp => 0.30,
            Vital::Fcp | Vital::Cls => 0.15,
            Vital::Ttfb => 0.10,
        }
    }

    /// How a value of it reads, by web.dev's published thresholds (good up
    /// to the first, poor past the second), the ones the dashboard colours
    /// a value by: `good`, `meh` or `poor`.
    pub fn rating(self, value: f64) -> &'static str {
        let (good, poor) = self.thresholds();
        if value <= good {
            "good"
        } else if value <= poor {
            "meh"
        } else {
            "poor"
        }
    }

    /// The good and poor lines of [`Vital::rating`]. The dashboard keeps the
    /// same in `shared/lib/web-vitals.ts`.
    fn thresholds(self) -> (f64, f64) {
        match self {
            Vital::Lcp => (2500.0, 4000.0),
            Vital::Fcp => (1800.0, 3000.0),
            Vital::Cls => (0.1, 0.25),
            Vital::Ttfb => (800.0, 1800.0),
            Vital::Inp => (200.0, 500.0),
        }
    }

    /// The vitals in a `measurements` object (`{"lcp": {"value": 2100}}`).
    pub fn from_measurements(measurements: &Value) -> Vec<(Vital, f64)> {
        Vital::ALL
            .into_iter()
            .filter_map(|vital| {
                let value = measurements.get(vital.name())?.get("value")?.as_f64()?;
                Some((vital, value))
            })
            .collect()
    }

    /// The vitals in a v2 span's attributes (`browser.web_vital.lcp.value`).
    pub fn from_attributes(attributes: &Value) -> Vec<(Vital, f64)> {
        Vital::ALL
            .into_iter()
            .filter_map(|vital| {
                let key = format!("browser.web_vital.{}.value", vital.name());
                Some((vital, attributes[key].as_f64()?))
            })
            .collect()
    }
}

/// `s` as a column of `max_chars` takes it: cut to fit, which Postgres
/// would otherwise refuse, and without U+0000, which Postgres text cannot
/// hold. Either would fail the whole batch of spans the vital came with.
fn text(s: &str, max_chars: usize) -> String {
    clamp(&s.replace('\0', ""), max_chars)
}

/// What each vital of a page load or interaction is about, as the browser
/// SDK attributes it.
#[derive(Default)]
pub(crate) struct Targets {
    pub lcp: Option<String>,
    pub lcp_resource: Option<String>,
    pub cls: Option<String>,
    pub inp: Option<String>,
}

impl Targets {
    /// The attributes `@sentry/browser` sets on the span it measured them
    /// on: `lcp.element`, `lcp.url` and `cls.source.1`, under `prefix`
    /// (`browser.web_vital.` when it streams spans). An interaction is
    /// named after its element, so its span's name is its target.
    pub fn read(attributes: &Value, prefix: &str, interaction: Option<&str>) -> Self {
        let text = |key: &str| {
            attributes[format!("{prefix}{key}")]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        Targets {
            lcp: text("lcp.element"),
            lcp_resource: text("lcp.url"),
            cls: text("cls.source.1"),
            inp: interaction.map(str::to_string),
        }
    }

    /// What a vital is about, and the resource it loaded.
    fn of(&self, vital: Vital) -> (Option<&str>, Option<&str>) {
        match vital {
            Vital::Lcp => (self.lcp.as_deref(), self.lcp_resource.as_deref()),
            Vital::Cls => (self.cls.as_deref(), None),
            Vital::Inp => (self.inp.as_deref(), None),
            Vital::Fcp | Vital::Ttfb => (None, None),
        }
    }
}

/// The vitals one page load or one interaction reported, and where.
pub(crate) struct PageVitals {
    pub page: String,
    pub browser: Option<String>,
    pub environment: Option<String>,
    pub release: Option<String>,
    pub trace_id: Option<String>,
    /// The span they came on: delivered again, it adds nothing.
    pub span_id: String,
    /// The page load they belong to: its span. `None` for an interaction.
    pub pageload_span_id: Option<String>,
    pub transaction_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub vitals: Vec<(Vital, f64)>,
    pub targets: Targets,
    /// The page's URL.
    pub url: Option<String>,
    /// The network's effective type.
    pub connection: Option<String>,
}

impl PageVitals {
    /// Stores each vital the browser's profile scores; any other is left
    /// out, as Sentry leaves an unscored vital out of its pages.
    pub(crate) async fn insert(
        &self,
        conn: &mut <crate::db::Db as sqlx::Database>::Connection,
        project_id: i32,
    ) -> AppResult<()> {
        for (vital, ratio) in score(&self.vitals, self.browser.as_deref()) {
            let value = self
                .vitals
                .iter()
                .find(|(v, _)| *v == vital)
                .map(|(_, value)| *value);
            let (target, resource) = self.targets.of(vital);
            sqlx::query(
                "INSERT INTO web_vitals ( \
                     project_id, page, vital, value, score, browser, environment, release, \
                     trace_id, span_id, transaction_id, timestamp, \
                     target, resource, url, device, connection, pageload_span_id \
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                           $13, $14, $15, $16, $17, $18) \
                 ON CONFLICT (project_id, span_id, vital) DO NOTHING",
            )
            .bind(project_id)
            .bind(text(&self.page, 200))
            .bind(vital.name())
            .bind(value)
            .bind(ratio)
            .bind(self.browser.as_deref().map(|b| text(b, 64)))
            .bind(self.environment.as_deref().map(|e| text(e, 64)))
            .bind(self.release.as_deref().map(|r| text(r, 200)))
            .bind(self.trace_id.as_deref().map(|t| text(t, 64)))
            .bind(text(&self.span_id, 32))
            .bind(self.transaction_id)
            .bind(self.timestamp)
            .bind(target.map(|t| text(t, 1024)))
            .bind(resource.map(|r| text(r, 1024)))
            .bind(self.url.as_deref().map(|u| text(u, 1024)))
            .bind(device(self.browser.as_deref()))
            .bind(self.connection.as_deref().map(|c| text(c, 16)))
            .bind(self.pageload_span_id.as_deref().map(|p| text(p, 32)))
            .execute(&mut *conn)
            .await?;
        }
        Ok(())
    }
}

/// Whether a span of this op is one Sentry's Web Vitals reads vitals from:
/// a page load, a standalone LCP or CLS, or an interaction's INP
/// (`DEFAULT_QUERY_FILTER`, static/app/views/insights/browser/webVitals/settings.ts).
pub fn reports_vitals(op: &str) -> bool {
    matches!(
        op,
        "pageload"
            | "ui.webvital.lcp"
            | "ui.webvital.cls"
            | "ui.interaction.click"
            | "ui.interaction.hover"
            | "ui.interaction.drag"
            | "ui.interaction.press"
    )
}

/// The browser a span was sent from: the `browser.name` the SDK set, or
/// else the one its `user_agent.original` names.
pub fn span_browser(attributes: &Value) -> Option<String> {
    if let Some(name) = attributes["browser.name"].as_str() {
        return Some(name.to_string());
    }
    browser_name(attributes["user_agent.original"].as_str()?).map(str::to_string)
}

/// The browser an event was sent from: the one it names in
/// `contexts.browser`, or else the one its `User-Agent` header names, which
/// is where Relay finds it for a browser SDK's events.
pub fn event_browser(event: &Value) -> Option<String> {
    if let Some(name) = event
        .pointer("/contexts/browser/name")
        .and_then(Value::as_str)
    {
        return Some(name.to_string());
    }
    fn user_agent<'a>(name: &str, value: &'a Value) -> Option<&'a str> {
        name.eq_ignore_ascii_case("user-agent")
            .then(|| value.as_str())
            .flatten()
    }
    // Headers are an object or a list of pairs.
    let user_agent = match event.pointer("/request/headers")? {
        Value::Object(headers) => headers.iter().find_map(|(k, v)| user_agent(k, v)),
        Value::Array(pairs) => pairs
            .iter()
            .find_map(|pair| user_agent(pair.get(0)?.as_str()?, pair.get(1)?)),
        _ => None,
    }?;
    browser_name(user_agent).map(str::to_string)
}

/// One vital's place in a profile: its weight in the total, and the two
/// points of the log-normal curve its value is scored on.
struct Component {
    vital: Vital,
    weight: f64,
    p10: f64,
    p50: f64,
    optional: bool,
}

/// The components a browser is scored with. No browsers matches any.
struct Profile {
    browsers: &'static [&'static str],
    components: &'static [Component],
}

const fn part(vital: Vital, weight: f64, p10: f64, p50: f64, optional: bool) -> Component {
    Component {
        vital,
        weight,
        p10,
        p50,
        optional,
    }
}

use Vital::*;

const DESKTOP: &[Component] = &[
    part(Fcp, 0.15, 900.0, 1600.0, true),
    part(Lcp, 0.30, 1200.0, 2400.0, true),
    part(Cls, 0.15, 0.1, 0.25, true),
    part(Ttfb, 0.10, 200.0, 400.0, true),
];
/// Firefox reports no CLS.
const DESKTOP_FIREFOX: &[Component] = &[
    part(Fcp, 0.15, 900.0, 1600.0, true),
    part(Lcp, 0.30, 1200.0, 2400.0, true),
    part(Cls, 0.0, 0.1, 0.25, false),
    part(Ttfb, 0.10, 200.0, 400.0, true),
];
/// Safari reports neither LCP nor CLS.
const DESKTOP_SAFARI: &[Component] = &[
    part(Fcp, 0.15, 900.0, 1600.0, true),
    part(Lcp, 0.0, 1200.0, 2400.0, false),
    part(Cls, 0.0, 0.1, 0.25, false),
    part(Ttfb, 0.10, 200.0, 400.0, true),
];
const MOBILE: &[Component] = &[
    part(Fcp, 0.15, 1800.0, 3000.0, true),
    part(Lcp, 0.30, 2500.0, 4000.0, true),
    part(Cls, 0.15, 0.1, 0.25, true),
    part(Ttfb, 0.10, 800.0, 1800.0, true),
];
const MOBILE_FIREFOX: &[Component] = &[
    part(Fcp, 0.15, 1800.0, 3000.0, true),
    part(Lcp, 0.30, 2500.0, 4000.0, true),
    part(Cls, 0.0, 0.1, 0.25, false),
    part(Ttfb, 0.10, 800.0, 1800.0, true),
];
const MOBILE_SAFARI: &[Component] = &[
    part(Fcp, 0.15, 1800.0, 3000.0, true),
    part(Lcp, 0.0, 2500.0, 4000.0, false),
    part(Cls, 0.0, 0.1, 0.25, false),
    part(Ttfb, 0.10, 800.0, 1800.0, true),
];
/// Every browser's INP profile is this one.
const INP: &[Component] = &[part(Inp, 1.0, 200.0, 500.0, false)];

const fn profile(browsers: &'static [&'static str], components: &'static [Component]) -> Profile {
    Profile {
        browsers,
        components,
    }
}

/// In Sentry's order: the first that matches and can score is the one used.
const PROFILES: &[Profile] = &[
    profile(&["Chrome"], DESKTOP),
    profile(&["Firefox"], DESKTOP_FIREFOX),
    profile(&["Safari"], DESKTOP_SAFARI),
    profile(&["Edge"], DESKTOP),
    profile(&["Opera"], DESKTOP),
    profile(&["Chrome Mobile"], MOBILE),
    profile(&["Firefox Mobile"], MOBILE_FIREFOX),
    profile(&["Mobile Safari"], MOBILE_SAFARI),
    profile(&["Edge Mobile"], MOBILE),
    profile(&["Opera Mobile"], MOBILE),
    profile(&[], DESKTOP),
    // Sentry has one INP profile per browser, all alike and all after the
    // pageload profiles, which can never score INP: this one gives the same.
    profile(&[], INP),
];

/// Each vital's score ratio, from 0 to 1, as Relay writes it to
/// `score.ratio.<vital>`. A vital the chosen profile has no curve for is
/// left out, and so is everything when no profile can score these values.
pub fn score(measurements: &[(Vital, f64)], browser: Option<&str>) -> Vec<(Vital, f64)> {
    let value = |vital| {
        measurements
            .iter()
            .find(|(v, _)| *v == vital)
            .map(|(_, value)| *value)
    };
    for profile in PROFILES {
        let matches = profile.browsers.is_empty()
            || browser.is_some_and(|browser| profile.browsers.contains(&browser));
        if !matches {
            continue;
        }
        // A required component with weight must be there.
        if profile
            .components
            .iter()
            .any(|c| value(c.vital).is_none() && c.weight != 0.0 && !c.optional)
        {
            continue;
        }
        // Nothing to weigh: Relay tries the next profile.
        let weight_total: f64 = profile
            .components
            .iter()
            .filter(|c| !c.optional || value(c.vital).is_some())
            .map(|c| c.weight)
            .sum();
        if weight_total.abs() < f64::EPSILON {
            continue;
        }
        return profile
            .components
            .iter()
            .filter_map(|c| {
                let ratio = cdf(value(c.vital)?.max(0.0), c.p10, c.p50);
                ratio.is_finite().then_some((c.vital, ratio))
            })
            .collect();
    }
    Vec::new()
}

/// The complement of the log-normal CDF through `p10` (0.9) and `p50` (0.5).
fn cdf(value: f64, p10: f64, p50: f64) -> f64 {
    let sigma = (p50.ln() - p10.ln()).abs() / (SQRT_2 * 0.9061938024368232);
    0.5 * (1.0 - erf((value.ln() - p50.ln()) / (SQRT_2 * sigma)))
}

/// Relay's error function: Abramowitz and Stegun 7.1.26. Not the exact one,
/// so scores agree with Sentry's to the last digit.
fn erf(x: f64) -> f64 {
    let (a1, a2, a3, a4, a5) = (
        0.254829592,
        -0.284496736,
        1.421413741,
        -1.453152027,
        1.061405429,
    );
    let p = 0.3275911;
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - ((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t * (-x * x).exp();
    sign * y
}

/// The browser a User-Agent names, as ua-parser (which Relay uses) names
/// it: what [`score`] compares a profile's browsers with.
///
/// ponytail: tokens for the browsers Sentry has a profile for, and Chrome
/// on iOS; any other is `None` and scored by the default profile, as Relay
/// scores a browser it has no profile for. A full ua-parser if the browser
/// filter must name more of them.
pub fn browser_name(user_agent: &str) -> Option<&'static str> {
    let has = |token| user_agent.contains(token);
    let mobile = has("Mobile");
    // Most to least specific: every Chromium UA also says Chrome and Safari.
    let (desktop, phone) = if has("Edg/") || has("EdgA/") || has("EdgiOS/") {
        ("Edge", "Edge Mobile")
    } else if has("OPR/") || has("Opera") {
        ("Opera", "Opera Mobile")
    } else if has("CriOS/") {
        return Some("Chrome Mobile iOS");
    } else if has("Firefox/") {
        ("Firefox", "Firefox Mobile")
    } else if has("Chrome/") {
        ("Chrome", "Chrome Mobile")
    } else if has("Safari/") && has("Version/") {
        ("Safari", "Mobile Safari")
    } else {
        return None;
    };
    Some(if mobile { phone } else { desktop })
}

/// The performance score of vitals' average scores, as Sentry totals it:
/// weighted, over the vitals that have one. `None` without any.
pub fn total_score(scores: impl IntoIterator<Item = (Vital, f64)>) -> Option<f64> {
    let (sum, weights) = scores
        .into_iter()
        .fold((0.0, 0.0), |(sum, weights), (vital, score)| {
            (sum + vital.weight() * score, weights + vital.weight())
        });
    (weights > 0.0).then(|| sum / weights)
}

/// The device a browser runs on, `mobile` or `desktop`, by its name:
/// ua-parser names every phone browser after it (`Chrome Mobile`, `Mobile
/// Safari`, `Chrome Mobile iOS`).
pub fn device(browser: Option<&str>) -> Option<&'static str> {
    Some(match browser?.contains("Mobile") {
        true => "mobile",
        false => "desktop",
    })
}

/// Which of a project's vitals a read is over. `None` keeps every one.
#[derive(Debug, Clone)]
pub struct WebVitalsFilter {
    pub environment: Option<String>,
    pub browser: Option<String>,
    /// `mobile` or `desktop`.
    pub device: Option<String>,
    pub page: Option<String>,
    pub vital: Option<Vital>,
    /// The last this many hours.
    pub period_hours: i64,
}

impl Default for WebVitalsFilter {
    fn default() -> Self {
        Self {
            environment: None,
            browser: None,
            device: None,
            page: None,
            vital: None,
            period_hours: crate::routes::period::MAX_WINDOW_HOURS,
        }
    }
}

impl WebVitalsFilter {
    fn push_where(&self, qb: &mut QueryBuilder<Db>, project_id: i32) {
        qb.push(" WHERE project_id = ").push_bind(project_id);
        for (column, value) in [
            ("environment", &self.environment),
            ("browser", &self.browser),
            ("device", &self.device),
            ("page", &self.page),
        ] {
            if let Some(value) = value {
                qb.push(format!(" AND {column} = "))
                    .push_bind(value.clone());
            }
        }
        if let Some(vital) = self.vital {
            qb.push(" AND vital = ").push_bind(vital.name());
        }
        qb.push(" AND ")
            .push(since_utc("timestamp", Some(self.period_hours)));
    }
}

/// What measurements are grouped by, besides their vital.
#[derive(Clone, Copy)]
enum By {
    /// Every page together.
    Project,
    Page,
    /// The element they were about, and the resource it loaded.
    Target,
    Browser,
    /// `mobile` or `desktop`.
    Device,
}

/// The dimensions a vital breaks down by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Breakdown {
    Device,
    Browser,
}

/// The percentiles every vital is read at: the 75th is the one Core Web
/// Vitals are judged by, the others show the spread around it.
pub const PERCENTILES: [u8; 5] = [50, 75, 90, 95, 99];

/// The rank just below a percentile in a group of `count`, 1-based, in
/// integer arithmetic both backends floor the same.
fn rank_below(count: i64, percentile: u8) -> i64 {
    (count - 1) * i64::from(percentile) * 10 / 1000 + 1
}

/// One vital's measurements in one group.
struct Group {
    count: i64,
    score_sum: f64,
    /// The values at the ranks the percentiles need, by rank.
    ranked: BTreeMap<i64, f64>,
    distribution: RatingCounts,
}

impl Group {
    /// The value at a percentile, interpolated as `percentile_cont` is,
    /// from the two values around it.
    fn at(&self, percentile: u8) -> f64 {
        let below = rank_below(self.count, percentile);
        let lower = self.ranked.get(&below).copied().unwrap_or_default();
        let upper = self.ranked.get(&(below + 1)).copied().unwrap_or(lower);
        let fraction = (self.count - 1) as f64 * f64::from(percentile) / 100.0 - (below - 1) as f64;
        lower + fraction * (upper - lower)
    }

    fn stats(&self, vital: Vital) -> VitalStats {
        VitalStats {
            vital,
            value: self.at(75),
            percentiles: PERCENTILES
                .into_iter()
                .map(|percentile| PercentileValue {
                    percentile,
                    value: self.at(percentile),
                })
                .collect(),
            score: self.score_sum / self.count as f64,
            count: self.count,
            distribution: RatingCounts {
                ..self.distribution
            },
        }
    }

    /// How much score its measurements miss of a perfect one.
    fn missed(&self) -> f64 {
        self.count as f64 - self.score_sum
    }
}

/// A group's key: `(page, "")` by page, `(target, resource)` by target,
/// `("", "")` for the project.
type Groups = BTreeMap<(String, String, Vital), Group>;

pub struct WebVitalsService;

impl WebVitalsService {
    /// The project's vitals, or one page's when the filter names it.
    pub async fn summary(
        pool: &DbPool,
        project_id: i32,
        filter: &WebVitalsFilter,
    ) -> AppResult<WebVitalsSummary> {
        let groups = Self::groups(pool, project_id, filter, By::Project).await?;
        let vitals = stats(&groups, "");
        let mut qb = QueryBuilder::new("SELECT DISTINCT browser FROM web_vitals");
        // Every browser the other filters keep, so another can be picked.
        WebVitalsFilter {
            browser: None,
            ..filter.clone()
        }
        .push_where(&mut qb, project_id);
        qb.push(" AND browser IS NOT NULL ORDER BY browser");
        let browsers = qb.build_query_scalar().fetch_all(pool).await?;
        Ok(WebVitalsSummary {
            score: total_score(vitals.iter().map(|v| (v.vital, v.score))),
            vitals,
            browsers,
        })
    }

    /// Every page with a vital, the one that costs the project the most
    /// score first, and how many there are. With `filter.vital`, the score
    /// of that vital alone.
    pub async fn pages(
        pool: &DbPool,
        project_id: i32,
        filter: &WebVitalsFilter,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<WebVitalsPage>, i64)> {
        // Every vital is read, so a page's row stays whole; only the
        // ranking is the one vital's.
        let all = WebVitalsFilter {
            vital: None,
            ..filter.clone()
        };
        let groups = Self::groups(pool, project_id, &all, By::Page).await?;

        // Sentry's opportunity: what a page's measurements miss of a perfect
        // score, over all of the vital's measurements, weighted over the
        // vitals the project has (`opportunity_score`,
        // src/sentry/search/eap/spans/formulas.py).
        let mut totals: BTreeMap<Vital, i64> = BTreeMap::new();
        for ((_, _, vital), group) in &groups {
            *totals.entry(*vital).or_default() += group.count;
        }
        let ranked = |vital: &Vital| filter.vital.is_none_or(|only| only == *vital);
        let weights: f64 = totals
            .keys()
            .filter(|v| ranked(v))
            .map(|v| v.weight())
            .sum();

        let mut qb = QueryBuilder::new(
            "SELECT page, COUNT(DISTINCT COALESCE(pageload_span_id, span_id)) \
             FROM web_vitals",
        );
        all.push_where(&mut qb, project_id);
        // An interaction is not a page load.
        qb.push(" AND vital <> 'inp' GROUP BY page");
        let pageloads: BTreeMap<String, i64> = qb
            .build_query_as::<(String, i64)>()
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();

        let mut pages: Vec<WebVitalsPage> = Vec::new();
        for (page, _, _) in groups.keys() {
            if pages.last().is_some_and(|last| &last.page == page) {
                continue;
            }
            let opportunity = Vital::ALL
                .into_iter()
                .filter(ranked)
                .filter_map(|vital| {
                    let group = groups.get(&(page.clone(), String::new(), vital))?;
                    Some(vital.weight() / weights * group.missed() / totals[&vital] as f64)
                })
                .sum();
            let vitals = stats(&groups, page);
            pages.push(WebVitalsPage {
                page: page.clone(),
                pageloads: pageloads.get(page).copied().unwrap_or(0),
                score: total_score(vitals.iter().map(|v| (v.vital, v.score))),
                opportunity,
                vitals,
            });
        }
        pages.sort_by(|a, b| {
            b.opportunity
                .total_cmp(&a.opportunity)
                .then_with(|| a.page.cmp(&b.page))
        });
        let total = pages.len() as i64;
        let pages = pages
            .into_iter()
            .skip(offset.max(0) as usize)
            .take(limit.max(0) as usize)
            .collect();
        Ok((pages, total))
    }

    /// The elements one vital was about, the one that loses the most score
    /// first: what to fix to raise it.
    pub async fn targets(
        pool: &DbPool,
        project_id: i32,
        filter: &WebVitalsFilter,
        limit: usize,
    ) -> AppResult<Vec<WebVitalTarget>> {
        let groups = Self::groups(pool, project_id, filter, By::Target).await?;
        let mut targets: Vec<_> = groups.into_iter().collect();
        targets.sort_by(|(a_key, a), (b_key, b)| {
            b.missed()
                .total_cmp(&a.missed())
                .then_with(|| a_key.cmp(b_key))
        });
        Ok(targets
            .into_iter()
            .take(limit)
            .map(|((target, resource, _), group)| WebVitalTarget {
                target,
                resource: (!resource.is_empty()).then_some(resource),
                count: group.count,
                value: group.at(75),
                score: group.score_sum / group.count as f64,
                distribution: group.distribution,
            })
            .collect())
    }

    /// One vital per device or per browser, the most measured first.
    pub async fn breakdown(
        pool: &DbPool,
        project_id: i32,
        filter: &WebVitalsFilter,
        vital: Vital,
        by: Breakdown,
    ) -> AppResult<Vec<WebVitalsBreakdownRow>> {
        let one = WebVitalsFilter {
            vital: Some(vital),
            ..filter.clone()
        };
        let by = match by {
            Breakdown::Device => By::Device,
            Breakdown::Browser => By::Browser,
        };
        let groups = Self::groups(pool, project_id, &one, by).await?;
        let mut rows: Vec<_> = groups
            .into_iter()
            .map(|((key, _, vital), group)| WebVitalsBreakdownRow {
                key: (!key.is_empty()).then_some(key),
                stats: group.stats(vital),
            })
            .collect();
        rows.sort_by(|a, b| {
            b.stats
                .count
                .cmp(&a.stats.count)
                .then_with(|| a.key.cmp(&b.key))
        });
        Ok(rows)
    }

    /// The worst measurements, the highest value first: one vital's when
    /// the filter names it.
    pub async fn samples(
        pool: &DbPool,
        project_id: i32,
        filter: &WebVitalsFilter,
        limit: i64,
    ) -> AppResult<Vec<WebVitalSample>> {
        let mut qb = QueryBuilder::new(
            "SELECT value, score, page, target, resource, url, browser, device, connection, \
                    release, transaction_id, timestamp \
             FROM web_vitals",
        );
        filter.push_where(&mut qb, project_id);
        qb.push(" ORDER BY value DESC, timestamp DESC LIMIT ")
            .push_bind(limit);
        Ok(qb.build_query_as().fetch_all(pool).await?)
    }

    /// How the measurements rate bucket by bucket over the window, oldest
    /// first, every bucket present: one vital's when the filter names it.
    pub async fn timeseries(
        pool: &DbPool,
        project_id: i32,
        filter: &WebVitalsFilter,
    ) -> AppResult<Vec<WebVitalsPoint>> {
        let interval = bucket_hours(filter.period_hours) * 3600;
        let (good, poor) = rating_lines();
        let mut qb = QueryBuilder::new(format!(
            "SELECT {} AS at, \
                    SUM(CASE WHEN value <= {good} THEN 1 ELSE 0 END), \
                    SUM(CASE WHEN value > {good} AND value <= {poor} THEN 1 ELSE 0 END), \
                    COUNT(*) \
             FROM web_vitals",
            bucket("timestamp", interval)
        ));
        filter.push_where(&mut qb, project_id);
        qb.push(" GROUP BY at");
        let rows: Vec<(i64, i64, i64, i64)> = qb.build_query_as().fetch_all(pool).await?;

        let now = Utc::now().timestamp();
        let align = |at: i64| at.div_euclid(interval) * interval;
        let mut buckets: BTreeMap<i64, RatingCounts> = BTreeMap::new();
        let mut at = align(now - filter.period_hours * 3600);
        while at <= align(now) {
            buckets.insert(at, RatingCounts::default());
            at += interval;
        }
        for (at, good, meh, count) in rows {
            // A clock running ahead counts in the current bucket.
            if let Some(counts) = buckets.get_mut(&align(at.min(now))) {
                counts.good += good;
                counts.meh += meh;
                counts.poor += count - good - meh;
            }
        }
        buckets
            .into_iter()
            .map(|(at, distribution)| {
                Ok(WebVitalsPoint {
                    bucket: bucket_time(at)?,
                    distribution,
                })
            })
            .collect()
    }

    /// Each vital's count, score, rating counts and the values around each
    /// of [`PERCENTILES`], per group, which a window function finds on both
    /// backends.
    async fn groups(
        pool: &DbPool,
        project_id: i32,
        filter: &WebVitalsFilter,
        by: By,
    ) -> AppResult<Groups> {
        let (keys, partition, only) = match by {
            By::Project => ("'' AS k1, '' AS k2", "vital", ""),
            By::Page => ("page AS k1, '' AS k2", "page, vital", ""),
            By::Target => (
                "target AS k1, COALESCE(resource, '') AS k2",
                "target, COALESCE(resource, ''), vital",
                " AND target IS NOT NULL",
            ),
            By::Browser => (
                "COALESCE(browser, '') AS k1, '' AS k2",
                "COALESCE(browser, ''), vital",
                "",
            ),
            By::Device => (
                "COALESCE(device, '') AS k1, '' AS k2",
                "COALESCE(device, ''), vital",
                "",
            ),
        };
        let (good, poor) = rating_lines();
        let counted = |condition: String| {
            format!("SUM(CASE WHEN {condition} THEN 1 ELSE 0 END) OVER (PARTITION BY {partition})")
        };
        let mut qb = QueryBuilder::new(format!(
            "SELECT k1, k2, vital, n, total, good, meh, rn, value FROM ( \
                 SELECT {keys}, vital, value, \
                        ROW_NUMBER() OVER (PARTITION BY {partition} ORDER BY value) AS rn, \
                        COUNT(*) OVER (PARTITION BY {partition}) AS n, \
                        SUM(score) OVER (PARTITION BY {partition}) AS total, \
                        {} AS good, {} AS meh \
                 FROM web_vitals",
            counted(format!("value <= {good}")),
            counted(format!("value > {good} AND value <= {poor}")),
        ));
        filter.push_where(&mut qb, project_id);
        qb.push(only);
        // The two ranks around each percentile, as `rank_below` reads them.
        let ranks: Vec<String> = PERCENTILES
            .iter()
            .map(|p| {
                let per_mille = i64::from(*p) * 10;
                format!(
                    "rn = (n - 1) * {per_mille} / 1000 + 1 OR rn = (n - 1) * {per_mille} / 1000 + 2"
                )
            })
            .collect();
        qb.push(format!(
            ") ranked WHERE {} ORDER BY k1, k2, vital, rn",
            ranks.join(" OR ")
        ));
        type Row = (String, String, String, i64, f64, i64, i64, i64, f64);
        let rows: Vec<Row> = qb.build_query_as().fetch_all(pool).await?;

        let mut groups = Groups::new();
        for (k1, k2, vital, count, score_sum, good, meh, rank, value) in rows {
            let Some(vital) = Vital::parse(&vital) else {
                continue;
            };
            groups
                .entry((k1, k2, vital))
                .or_insert(Group {
                    count,
                    score_sum,
                    ranked: BTreeMap::new(),
                    distribution: RatingCounts {
                        good,
                        meh,
                        poor: count - good - meh,
                    },
                })
                .ranked
                .insert(rank, value);
        }
        Ok(groups)
    }
}

/// Each vital's web.dev good and poor lines as SQL over its `vital` column:
/// from code, never from the request.
fn rating_lines() -> (String, String) {
    let line = |pick: fn((f64, f64)) -> f64| {
        let arms: String = Vital::ALL
            .iter()
            .map(|v| format!(" WHEN '{}' THEN {}", v.name(), pick(v.thresholds())))
            .collect();
        format!("CASE vital{arms} END")
    };
    (line(|(good, _)| good), line(|(_, poor)| poor))
}

/// One page's vitals (`""` for every page's) in [`Vital::ALL`] order.
fn stats(groups: &Groups, page: &str) -> Vec<VitalStats> {
    Vital::ALL
        .into_iter()
        .filter_map(|vital| {
            Some(
                groups
                    .get(&(page.to_string(), String::new(), vital))?
                    .stats(vital),
            )
        })
        .collect()
}
