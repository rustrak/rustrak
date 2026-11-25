//! What the tracker script (`src/public/script.js`) sends: one page view or
//! event per request, as a beacon (text/plain JSON, so no CORS preflight),
//! with the project's DSN key.

use rustrak_plugin::prelude::*;
use serde::Deserialize;

use crate::Settings;
use crate::models::{page, user_agent, visit, visitor};

const MAX_TITLE: usize = 200;
const MAX_EVENT_NAME: usize = 100;
/// An event's data, as JSON; bigger data is dropped, the event kept.
const MAX_EVENT_DATA: usize = 4_096;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Beacon {
    Pageview {
        url: String,
        /// `document.referrer`, on a visit's first page only.
        #[serde(default)]
        referrer: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        language: Option<String>,
    },
    Event {
        url: String,
        name: String,
        #[serde(default)]
        data: Option<serde_json::Map<String, serde_json::Value>>,
    },
}

/// A page view or an event. Bots, visitors asking not to be tracked, and
/// pages on another site are dropped without saying so: a beacon reads no
/// answer, and a scraper learns nothing from one.
#[post("/collect", public)]
fn collect(ctx: &Ctx, beacon: Json<Beacon>) -> Result<NoContent> {
    let settings = Settings::get(ctx)?;
    let project = ctx.project()?;
    if settings.respect_do_not_track && ctx.header("dnt") == Some("1") {
        return Ok(NoContent);
    }
    let ua = ctx.header("user-agent").unwrap_or("");
    let Some(agent) = user_agent::parse(ua) else {
        return Ok(NoContent);
    };
    let (url, referrer) = match &beacon.0 {
        Beacon::Pageview { url, referrer, .. } => (url, referrer.as_deref()),
        Beacon::Event { url, .. } => (url, None),
    };
    let page = page::read(url, referrer, settings.keep_query)
        .ok_or_else(|| Error::BadRequest("the page is not a web URL".into()))?;
    if !page::allowed(&page.host, settings.allowed_domains.as_deref()) {
        return Ok(NoContent);
    }
    let id = visitor::id(ctx, project, ua)?;
    match beacon.0 {
        Beacon::Pageview {
            title, language, ..
        } => {
            let language = language.as_deref().or(ctx.header("accept-language"));
            let who = visit::Visitor {
                id: &id,
                agent: &agent,
                country: page::country(|h| ctx.header(h).map(str::to_string), language),
                language: page::language(language),
            };
            let title = title.map(|t| page::shorten(&t, MAX_TITLE));
            visit::record_pageview(ctx, project, &page, title.as_deref(), &who)?;
        }
        Beacon::Event { name, data, .. } => {
            let name = page::shorten(&name, MAX_EVENT_NAME);
            if name.is_empty() {
                return Err(Error::BadRequest("an event needs a name".into()));
            }
            let data = data
                .map(|d| serde_json::Value::Object(d).to_string())
                .filter(|d| d.len() <= MAX_EVENT_DATA);
            visit::record_event(ctx, project, &name, &page, data, &id)?;
        }
    }
    Ok(NoContent)
}
