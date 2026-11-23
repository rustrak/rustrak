//! What the Web Vitals endpoints answer: a project's or a page's vitals,
//! scored as Sentry scores them.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::services::web_vitals::Vital;

/// How many measurements web.dev rates good, needing improvement and poor.
#[derive(Debug, Default, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RatingCounts {
    pub good: i64,
    pub meh: i64,
    pub poor: i64,
}

/// A vital's value at one percentile.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PercentileValue {
    pub percentile: u8,
    pub value: f64,
}

/// One vital over a window: its 75th percentile, the spread around it, its
/// average score and how its measurements rate.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VitalStats {
    pub vital: Vital,
    /// The 75th percentile, which Core Web Vitals are judged by:
    /// milliseconds, or unitless for CLS.
    pub value: f64,
    /// The 50th, 75th, 90th, 95th and 99th percentiles.
    pub percentiles: Vec<PercentileValue>,
    /// Average score ratio, from 0 to 1.
    pub score: f64,
    /// How many measurements.
    pub count: i64,
    pub distribution: RatingCounts,
}

/// One element a vital was about (an LCP's image, an INP's button), and how
/// it did.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebVitalTarget {
    /// The element, as a CSS-like path (`body > img.hero`).
    pub target: String,
    /// The resource it loaded, for an LCP (an image's URL).
    #[cfg_attr(feature = "openapi", schema(required))]
    pub resource: Option<String>,
    pub count: i64,
    /// The 75th percentile.
    pub value: f64,
    /// Average score ratio, from 0 to 1.
    pub score: f64,
    pub distribution: RatingCounts,
}

/// One vital on one device or one browser.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebVitalsBreakdownRow {
    /// The device (`mobile`, `desktop`) or browser; `null` when the SDK
    /// sent nothing to tell it by.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub key: Option<String>,
    pub stats: VitalStats,
}

/// One measurement of a vital, and where it happened.
#[derive(Debug, Serialize, sqlx::FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebVitalSample {
    pub value: f64,
    pub score: f64,
    pub page: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub target: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub resource: Option<String>,
    /// The page's URL.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub url: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub browser: Option<String>,
    /// `mobile` or `desktop`.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub device: Option<String>,
    /// The network's effective type (`4g`, `3g`...).
    #[cfg_attr(feature = "openapi", schema(required))]
    pub connection: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub release: Option<String>,
    /// The page load's transaction, when it is known.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub transaction_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
}

/// A project's, or one page's, web vitals over a window.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebVitalsSummary {
    /// The performance score, from 0 to 1: each vital's average score
    /// weighted as Sentry weighs them, over the vitals with data. `null`
    /// without any.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub score: Option<f64>,
    /// The vitals with data, in score weight order.
    pub vitals: Vec<VitalStats>,
    /// The browsers the window's vitals came from, for the browser filter.
    pub browsers: Vec<String>,
}

/// One page's web vitals, and how much of the project's score it costs.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebVitalsPage {
    /// The route or URL the page loaded as (its transaction's name).
    pub page: String,
    /// How many page loads reported a vital.
    pub pageloads: i64,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub score: Option<f64>,
    /// The share of the project's score this page loses, from 0 to 1: the
    /// pages' add up to one minus the project's score.
    pub opportunity: f64,
    pub vitals: Vec<VitalStats>,
}

/// One bucket of the trend: how its measurements rate.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebVitalsPoint {
    pub bucket: DateTime<Utc>,
    pub distribution: RatingCounts,
}
