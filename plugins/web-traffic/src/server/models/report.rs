//! What a report covers: one project, a window, and any of the dimensions a
//! visitor is counted by (`?country=ES&browser=Firefox`). Every figure,
//! list and chart reads through the same `Report`, so they always agree.

use std::collections::HashMap;

use rustrak_plugin::prelude::*;
use rustrak_plugin::{ToValue, Value};

/// The dimensions a report is narrowed and ranked by: the query key and its
/// column. The UI's list (`src/ui/model.ts`) uses the same keys.
pub const DIMENSIONS: &[(&str, &str)] = &[
    ("path", "path"),
    ("title", "title"),
    ("referrer", "referrer"),
    ("source", "utm_source"),
    ("medium", "utm_medium"),
    ("campaign", "utm_campaign"),
    ("browser", "browser"),
    ("os", "os"),
    ("device", "device"),
    ("country", "country"),
    ("language", "language"),
];

pub fn column(dimension: &str) -> Option<&'static str> {
    DIMENSIONS
        .iter()
        .find(|(key, _)| *key == dimension)
        .map(|(_, column)| *column)
}

/// The windows on offer and the bucket each is charted in; anything else is
/// two weeks, as the dashboard's pages open. The UI offers the same ones.
fn window_of(period: Option<&str>) -> (i64, i64) {
    const DAY: i64 = 86_400;
    match period {
        Some("24h") => (DAY, 3_600),
        Some("7d") => (7 * DAY, DAY),
        Some("30d") => (30 * DAY, DAY),
        Some("90d") => (90 * DAY, DAY),
        _ => (14 * DAY, DAY),
    }
}

pub struct Report {
    project: u64,
    /// `[from, to)` in unix seconds, whole buckets: the last one is now's.
    pub from: i64,
    pub to: i64,
    pub bucket: i64,
    filters: Vec<(&'static str, String)>,
}

impl Report {
    /// The report a request asks for: its project, `?period=` and filters.
    pub fn new(ctx: &Ctx, query: &HashMap<String, String>) -> Result<Report> {
        let project = ctx.project()?;
        let now: i64 = ctx.db().scalar("SELECT unixepoch()", params![])?;
        let (length, bucket) = window_of(query.get("period").map(String::as_str));
        let to = (now / bucket + 1) * bucket;
        Ok(Report {
            project,
            from: to - length,
            to,
            bucket,
            filters: DIMENSIONS
                .iter()
                .filter_map(|(key, column)| {
                    let value = query.get(*key)?.trim();
                    (!value.is_empty()).then(|| (*column, value.to_string()))
                })
                .collect(),
        })
    }

    /// The same report over the window just before: what figures compare to.
    pub fn previous(&self) -> Report {
        let length = self.to - self.from;
        Report {
            project: self.project,
            from: self.from - length,
            to: self.from,
            bucket: self.bucket,
            filters: self.filters.clone(),
        }
    }

    /// `pageviews` of the project in the window, narrowed: the `FROM … WHERE`
    /// of a query, with its parameters in order.
    pub fn pageviews(&self) -> Sql {
        let mut sql = Sql::new(
            "FROM pageviews WHERE project_id = ? AND created_at >= ? AND created_at < ?",
            [
                self.project.to_value(),
                self.from.to_value(),
                self.to.to_value(),
            ],
        );
        for (column, value) in &self.filters {
            sql.push(&format!(" AND {column} = ?"), [value.to_value()]);
        }
        sql
    }

    /// `events` in the window, of the visitors the filters keep: a `WHERE`
    /// condition on `events`.
    pub fn events(&self) -> Sql {
        let mut sql = Sql::new(
            "events.project_id = ? AND events.created_at >= ? AND events.created_at < ?",
            [
                self.project.to_value(),
                self.from.to_value(),
                self.to.to_value(),
            ],
        );
        if !self.filters.is_empty() {
            sql.push(" AND events.visitor IN (SELECT visitor ", []);
            sql.append(self.pageviews());
            sql.push(")", []);
        }
        sql
    }

    pub fn project(&self) -> u64 {
        self.project
    }
}

/// A piece of SQL and its parameters, in the order its `?` appear.
#[derive(Clone)]
pub struct Sql {
    pub text: String,
    params: Vec<Value>,
}

impl Sql {
    pub fn new(text: &str, params: impl IntoIterator<Item = Value>) -> Sql {
        Sql {
            text: text.to_string(),
            params: params.into_iter().collect(),
        }
    }

    pub fn push(&mut self, text: &str, params: impl IntoIterator<Item = Value>) -> &mut Sql {
        self.text.push_str(text);
        self.params.extend(params);
        self
    }

    pub fn append(&mut self, other: Sql) -> &mut Sql {
        self.text.push_str(&other.text);
        self.params.extend(other.params);
        self
    }

    pub fn params(&self) -> Vec<&dyn ToValue> {
        self.params.iter().map(|v| v as &dyn ToValue).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dimension_has_its_own_column_and_unknown_ones_none() {
        assert_eq!(column("path"), Some("path"));
        assert_eq!(column("source"), Some("utm_source"));
        assert_eq!(column("visitor"), None);
        let keys: std::collections::HashSet<_> = DIMENSIONS.iter().map(|(k, _)| k).collect();
        assert_eq!(keys.len(), DIMENSIONS.len());
    }

    #[test]
    fn a_window_is_one_the_dashboard_offers_else_two_weeks() {
        assert_eq!(window_of(Some("24h")), (86_400, 3_600));
        assert_eq!(window_of(Some("30d")).0, 30 * 86_400);
        assert_eq!(window_of(Some("all")).0, 14 * 86_400);
        assert_eq!(window_of(None).0, 14 * 86_400);
    }
}
