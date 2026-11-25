//! Writing what a website reports: page views and its own events.

use rustrak_plugin::prelude::*;

use super::page::Page;
use super::user_agent::Agent;

/// Who saw it, and with what.
pub struct Visitor<'a> {
    pub id: &'a str,
    pub agent: &'a Agent,
    pub country: Option<String>,
    pub language: Option<String>,
}

pub fn record_pageview(
    ctx: &Ctx,
    project: u64,
    page: &Page,
    title: Option<&str>,
    who: &Visitor,
) -> Result<()> {
    ctx.db().execute(
        "INSERT INTO pageviews (project_id, visitor, path, query, title, referrer, \
         utm_source, utm_medium, utm_campaign, browser, os, device, country, language, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, unixepoch())",
        params![
            project,
            who.id,
            page.path.as_str(),
            page.query.clone(),
            title,
            page.referrer.clone(),
            page.utm_source.clone(),
            page.utm_medium.clone(),
            page.utm_campaign.clone(),
            who.agent.browser,
            who.agent.os,
            who.agent.device,
            who.country.clone(),
            who.language.clone(),
        ],
    )?;
    Ok(())
}

pub fn record_event(
    ctx: &Ctx,
    project: u64,
    name: &str,
    page: &Page,
    data: Option<String>,
    visitor: &str,
) -> Result<()> {
    ctx.db().execute(
        "INSERT INTO events (project_id, visitor, name, path, data, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, unixepoch())",
        params![project, visitor, name, page.path.as_str(), data],
    )?;
    Ok(())
}
