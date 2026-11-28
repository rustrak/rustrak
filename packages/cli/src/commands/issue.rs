//! Issue triage and the events and activity behind each issue.

use std::io::{IsTerminal, Write};

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::api::{paths, ActivityEntry, CursorPage, Event, EventDetail, Issue, Page};
use crate::cli::{FilterArg, IssueCommand, IssueListArgs, PriorityArg, SortArg};
use crate::commands::Context;
use crate::error::{Error, Result};
use crate::output::{one_line, relative, truncate, when};

pub async fn run(command: IssueCommand, ctx: &Context) -> Result<()> {
    ctx.require_auth()?;
    let project = ctx.project_id(None).await?;
    match command {
        IssueCommand::List(args) => list(args, ctx, project).await,
        IssueCommand::Get(args) => {
            let uuid = resolve_issue(ctx, project, &args.issue).await?;
            let (raw, issue): (_, Issue) = get(ctx, &paths::issue(project, &uuid), &[]).await?;
            if ctx.output.is_json() {
                return ctx.output.json(&raw);
            }
            ctx.output.line(format!(
                "Issue:      {}\nUUID:       {}\nTitle:      {}\nCulprit:    {}\nStatus:     {}\nPriority:   {}\nLevel:      {}\nPlatform:   {}\nEvents:     {}\nUsers:      {}\nFirst seen: {}\nLast seen:  {}\nRelease:    {}",
                issue.short_id, issue.id, issue.title, issue.culprit, issue.status,
                issue.priority.as_deref().unwrap_or("-"), issue.level.as_deref().unwrap_or("-"),
                issue.platform.as_deref().unwrap_or("-"), issue.event_count,
                issue.user_count.map(|n| n.to_string()).unwrap_or_else(|| "-".into()),
                when(&issue.first_seen), when(&issue.last_seen), issue.release.as_deref().unwrap_or("-")
            ));
            Ok(())
        }
        IssueCommand::Resolve(args) => {
            update(ctx, project, &args.issues, "status", "resolved").await
        }
        IssueCommand::Unresolve(args) => {
            update(ctx, project, &args.issues, "status", "unresolved").await
        }
        IssueCommand::Ignore(args) => update(ctx, project, &args.issues, "status", "ignored").await,
        IssueCommand::Priority(args) => {
            let priority = match args.priority {
                PriorityArg::High => "high",
                PriorityArg::Medium => "medium",
                PriorityArg::Low => "low",
            };
            update(ctx, project, &args.issues, "priority", priority).await
        }
        IssueCommand::Events(args) => {
            validate_limit(args.limit)?;
            let uuid = resolve_issue(ctx, project, &args.issue).await?;
            let mut query = vec![
                ("per_page", args.limit.to_string()),
                ("order", order(args.asc).into()),
            ];
            optional(&mut query, "cursor", args.cursor);
            optional(&mut query, "environment", args.environment);
            let (raw, page): (_, CursorPage<Event>) =
                get(ctx, &paths::issue_events(project, &uuid), &query).await?;
            if ctx.output.is_json() {
                return ctx.output.json(&raw);
            }
            let mut table =
                ctx.output
                    .table(&["EVENT ID", "LEVEL", "TIMESTAMP", "PLATFORM", "TITLE"]);
            for event in &page.items {
                table.add_row([
                    event_reference(event).to_string(),
                    event.level.clone(),
                    when(&event.timestamp),
                    event.platform.clone(),
                    truncate(&one_line(&event.title), 72),
                ]);
            }
            println!("{table}");
            if page.has_more {
                if let Some(cursor) = page.next_cursor {
                    ctx.output
                        .line(format!("Next cursor: {cursor} (pass --cursor {cursor})"));
                }
            }
            Ok(())
        }
        IssueCommand::Event(args) => {
            let uuid = resolve_issue(ctx, project, &args.issue).await?;
            let event_id = if args.event == "latest" {
                let page: CursorPage<Event> = ctx
                    .client
                    .get(
                        &paths::issue_events(project, &uuid),
                        &[("order", "desc".into()), ("per_page", "1".into())],
                    )
                    .await?;
                page.items
                    .first()
                    .map(|e| event_reference(e).to_string())
                    .ok_or_else(|| {
                        Error::Invalid(format!("issue {:?} has no events", args.issue))
                    })?
            } else {
                args.event
            };
            let path = format!(
                "/api/projects/{project}/events/sentry/{}",
                segment(&event_id)
            );
            let (raw, event): (_, EventDetail) = get(ctx, &path, &[]).await?;
            if ctx.output.is_json() {
                return ctx.output.json(&raw);
            }
            ctx.output.line(format!(
                "Event:       {}\nTitle:       {}\nLevel:       {}\nTimestamp:   {}\nPlatform:    {}\nEnvironment: {}\nRelease:     {}\nSDK:         {} {}\nIngested:    {}\nData:\n{}",
                event.event_id, event.title, event.level, when(&event.timestamp), event.platform,
                event.environment, event.release, event.sdk_name, event.sdk_version,
                when(&event.ingested_at), serde_json::to_string_pretty(&event.data)
                    .map_err(|e| Error::Invalid(format!("could not encode event data: {e}")))?
            ));
            Ok(())
        }
        IssueCommand::Activity(args) => {
            let uuid = resolve_issue(ctx, project, &args.issue).await?;
            let (raw, activity): (_, Vec<ActivityEntry>) =
                get(ctx, &paths::issue_activity(project, &uuid), &[]).await?;
            if ctx.output.is_json() {
                return ctx.output.json(&raw);
            }
            let mut table = ctx.output.table(&["WHEN", "TYPE", "USER", "DATA"]);
            for entry in activity {
                table.add_row([
                    when(&entry.created_at),
                    entry.r#type,
                    entry
                        .user_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".into()),
                    truncate(&one_line(&entry.data), 100),
                ]);
            }
            println!("{table}");
            Ok(())
        }
        IssueCommand::Comment(args) => {
            if args.text.trim().is_empty() {
                return Err(Error::Invalid("comment text cannot be empty".into()));
            }
            let uuid = resolve_issue(ctx, project, &args.issue).await?;
            let response: Value = ctx
                .client
                .post(
                    &paths::issue_comments(project, &uuid),
                    &json!({"text": args.text}),
                )
                .await?;
            if ctx.output.is_json() {
                return ctx.output.json(&response);
            }
            ctx.output.line(format!("Comment added to {}.", args.issue));
            Ok(())
        }
        IssueCommand::Seen(args) => {
            let uuid = resolve_issue(ctx, project, &args.issue).await?;
            ctx.client
                .post_empty(&paths::issue_seen(project, &uuid), &json!({}))
                .await?;
            if ctx.output.is_json() {
                return ctx.output.json(&json!({"issue": uuid, "has_seen": true}));
            }
            ctx.output.line(format!("Marked {} as seen.", args.issue));
            Ok(())
        }
        IssueCommand::Tags(args) => {
            let uuid = resolve_issue(ctx, project, &args.issue).await?;
            let issue: Value = ctx.client.get(&paths::issue(project, &uuid), &[]).await?;
            match issue.get("tags").filter(|tags| !tags.is_null()) {
                Some(tags) => ctx.output.json(tags),
                None => {
                    if ctx.output.is_json() {
                        ctx.output.json(&json!({"tags": null, "message": "Issue tags are available in the dashboard."}))
                    } else {
                        ctx.output.line("Issue tags are available in the dashboard; the issue API returned no tags.");
                        Ok(())
                    }
                }
            }
        }
        IssueCommand::Delete(args) => {
            let ids = resolve_issues(ctx, project, &args.issues).await?;
            if !args.yes && !confirm(&format!("Delete {} issue(s) permanently?", ids.len()))? {
                return Err(Error::Invalid("deletion cancelled".into()));
            }
            let response: Value = ctx
                .client
                .delete_with(&paths::issues(project), &json!({"ids": ids}))
                .await?;
            ctx.output.json(&response)
        }
        IssueCommand::Stats(args) => {
            let uuid = resolve_issue(ctx, project, &args.issue).await?;
            let path = format!("/api/projects/{project}/issues/{uuid}/stats");
            let (raw, stats): (_, Stats) = get(ctx, &path, &[("window", args.window)]).await?;
            let total: i128 = stats.data.iter().map(|point| i128::from(point[1])).sum();
            if ctx.output.is_json() {
                let mut response = raw;
                response["total"] = json!(total);
                return ctx.output.json(&response);
            }
            if args.spark {
                ctx.output.line(sparkline(&stats.data));
            } else {
                let mut table = ctx.output.table(&["TIMESTAMP", "COUNT"]);
                for [timestamp, count] in &stats.data {
                    let timestamp = chrono::DateTime::from_timestamp(*timestamp, 0)
                        .map(|dt| when(&dt.to_rfc3339()))
                        .unwrap_or_else(|| timestamp.to_string());
                    table.add_row([timestamp, count.to_string()]);
                }
                println!("{table}");
            }
            ctx.output.line(format!("Total: {total}"));
            Ok(())
        }
    }
}

async fn list(args: IssueListArgs, ctx: &Context, project: i32) -> Result<()> {
    validate_limit(args.limit)?;
    if args.page < 1 {
        return Err(Error::Invalid("--page must be at least 1".into()));
    }
    let sort = match args.sort {
        SortArg::LastSeen => "last_seen",
        SortArg::EventCount => "event_count",
        SortArg::FirstSeen => "first_seen",
        SortArg::Priority => "priority",
        SortArg::DigestOrder => "digest_order",
    };
    let filter = match args.filter {
        FilterArg::Open => "open",
        FilterArg::Resolved => "resolved",
        FilterArg::Muted => "muted",
        FilterArg::All => "all",
    };
    let mut query = vec![
        ("page", args.page.to_string()),
        ("per_page", args.limit.to_string()),
        ("sort", sort.into()),
        ("order", order(args.asc).into()),
        ("filter", filter.into()),
    ];
    for (name, value) in [
        ("q", args.query),
        ("level", args.level),
        ("priority", args.priority),
        ("environment", args.environment),
        ("release", args.release),
        ("first_seen", args.first_seen),
        ("last_seen", args.last_seen),
    ] {
        optional(&mut query, name, value);
    }
    let (raw, page): (_, Page<Issue>) = get(ctx, &paths::issues(project), &query).await?;
    if ctx.output.is_json() {
        return ctx.output.json(&raw);
    }
    let mut table =
        ctx.output
            .table(&["SHORT ID", "LEVEL", "EVENTS", "USERS", "LAST SEEN", "TITLE"]);
    for issue in &page.items {
        table.add_row([
            issue.short_id.clone(),
            issue.level.clone().unwrap_or_else(|| "-".into()),
            issue.event_count.to_string(),
            issue
                .user_count
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into()),
            relative(&issue.last_seen),
            truncate(&one_line(&issue.title), 72),
        ]);
    }
    println!("{table}");
    ctx.output.line(format!(
        "{} of {} issues (page {} of {}).",
        page.items.len(),
        page.total_count,
        page.page,
        page.total_pages
    ));
    Ok(())
}

async fn resolve_issue(ctx: &Context, project: i32, reference: &str) -> Result<String> {
    // The management routes accept UUIDs, while the dashboard displays short ids.
    if is_uuid(reference) {
        return Ok(reference.to_string());
    }
    for number in 1..=20 {
        let page: Page<Issue> = ctx
            .client
            .get(
                &paths::issues(project),
                &[
                    ("per_page", "100".into()),
                    ("page", number.to_string()),
                    ("filter", "all".into()),
                    ("sort", "digest_order".into()),
                    ("order", "desc".into()),
                ],
            )
            .await?;
        if let Some(issue) = page
            .items
            .iter()
            .find(|issue| issue.short_id.eq_ignore_ascii_case(reference) || issue.id == reference)
        {
            return Ok(issue.id.clone());
        }
        if page.items.is_empty() || (page.total_pages > 0 && number >= page.total_pages) {
            break;
        }
    }
    Err(Error::Invalid(format!("no issue matches {reference:?} in project {project} within 20 pages; use the issue UUID for older issues")))
}

async fn resolve_issues(ctx: &Context, project: i32, references: &[String]) -> Result<Vec<String>> {
    if references.is_empty() {
        return Err(Error::Invalid("at least one issue is required".into()));
    }
    let mut ids = Vec::new();
    for reference in references {
        let id = resolve_issue(ctx, project, reference).await?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

async fn update(
    ctx: &Context,
    project: i32,
    references: &[String],
    field: &str,
    value: &str,
) -> Result<()> {
    let ids = resolve_issues(ctx, project, references).await?;
    let mut body = json!({"ids": ids});
    body[field] = json!(value);
    let response: Value = ctx.client.put(&paths::issues(project), &body).await?;
    if ctx.output.is_json() {
        return ctx.output.json(&response);
    }
    let updated = response
        .get("updated")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::Invalid("bulk update response has no updated count".into()))?;
    ctx.output
        .line(format!("Updated {updated} issue(s): {field} = {value}."));
    Ok(())
}

// Keep the original response for JSON output; API structs deliberately omit fields.
async fn get<T: DeserializeOwned>(
    ctx: &Context,
    path: &str,
    query: &[(&str, String)],
) -> Result<(Value, T)> {
    let raw: Value = ctx.client.get(path, query).await?;
    let parsed = serde_json::from_value(raw.clone()).map_err(|source| Error::Decode {
        path: path.to_string(),
        source,
    })?;
    Ok((raw, parsed))
}

fn optional<'a>(query: &mut Vec<(&'a str, String)>, name: &'a str, value: Option<String>) {
    if let Some(value) = value {
        query.push((name, value));
    }
}

fn order(asc: bool) -> &'static str {
    if asc {
        "asc"
    } else {
        "desc"
    }
}

fn validate_limit(limit: i64) -> Result<()> {
    if !(1..=100).contains(&limit) {
        return Err(Error::Invalid("--limit must be between 1 and 100".into()));
    }
    Ok(())
}

fn is_uuid(reference: &str) -> bool {
    reference.len() == 36
        && reference.bytes().enumerate().all(|(i, byte)| {
            if [8, 13, 18, 23].contains(&i) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn event_reference(event: &Event) -> &str {
    if event.event_id.is_empty() {
        &event.id
    } else {
        &event.event_id
    }
}

fn segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn confirm(prompt: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Err(Error::Invalid(
            "deletion needs confirmation; pass --yes in scripts".into(),
        ));
    }
    eprint!("{prompt} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

#[derive(serde::Deserialize)]
struct Stats {
    data: Vec<[i64; 2]>,
}

fn sparkline(data: &[[i64; 2]]) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = data.iter().map(|point| point[1]).max().unwrap_or(0).max(0);
    data.iter()
        .map(|point| {
            let index = if max == 0 {
                0
            } else {
                (i128::from(point[1].max(0)) * 7 / i128::from(max)) as usize
            };
            BLOCKS[index]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Client;
    use crate::output::{Format, Output};
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const UUID: &str = "12345678-1234-1234-1234-123456789abc";

    fn context(server: &MockServer) -> Context {
        Context {
            client: Client::new(server.uri(), Some("test-token".into())).unwrap(),
            output: Output {
                format: Format::Json,
                colour: false,
            },
            url: server.uri(),
            profile: None,
            project_hint: Some("7".into()),
            json: true,
        }
    }

    #[tokio::test]
    async fn short_ids_search_all_statuses_and_later_pages() {
        let server = MockServer::start().await;
        for (number, items) in [
            (
                1,
                json!([{"id": "aaaaaaaa-1234-1234-1234-123456789abc", "short_id": "APP-13", "title": "other"}]),
            ),
            (
                2,
                json!([{"id": UUID, "short_id": "APP-12", "title": "target"}]),
            ),
        ] {
            Mock::given(method("GET"))
                .and(path("/api/projects/7/issues"))
                .and(query_param("filter", "all"))
                .and(query_param("per_page", "100"))
                .and(query_param("page", number.to_string()))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "items": items, "total_count": 101, "total_pages": 2
                })))
                .expect(1)
                .mount(&server)
                .await;
        }
        assert_eq!(
            resolve_issue(&context(&server), 7, "app-12").await.unwrap(),
            UUID
        );
    }

    #[tokio::test]
    async fn unresolved_short_id_is_invalid() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [], "total_count": 0, "total_pages": 0
            })))
            .expect(1)
            .mount(&server)
            .await;
        assert!(matches!(
            resolve_issue(&context(&server), 7, "APP-999").await,
            Err(Error::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn bulk_update_resolves_before_writing_and_deduplicates() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/projects/7/issues"))
            .and(body_json(json!({"ids": [UUID], "status": "resolved"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"updated": 1})))
            .expect(1)
            .mount(&server)
            .await;
        update(
            &context(&server),
            7,
            &[UUID.into(), UUID.into()],
            "status",
            "resolved",
        )
        .await
        .unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn comments_send_text_and_seen_accepts_empty_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/api/projects/7/issues/{UUID}/comments")))
            .and(body_json(json!({"text": "investigating"})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "note"})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("/api/projects/7/issues/{UUID}/seen")))
            .and(body_json(json!({})))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let ctx = context(&server);
        run(
            IssueCommand::Comment(crate::cli::IssueCommentArgs {
                issue: UUID.into(),
                text: "investigating".into(),
            }),
            &ctx,
        )
        .await
        .unwrap();
        run(
            IssueCommand::Seen(crate::cli::IssueGetArgs { issue: UUID.into() }),
            &ctx,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn latest_event_uses_cursor_list_then_sentry_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/api/projects/7/issues/{UUID}/events")))
            .and(query_param("order", "desc"))
            .and(query_param("per_page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{"id": "internal-id", "event_id": UUID}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/api/projects/7/events/sentry/{UUID}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"event_id": UUID})))
            .expect(1)
            .mount(&server)
            .await;
        run(
            IssueCommand::Event(crate::cli::IssueEventArgs {
                issue: UUID.into(),
                event: "latest".into(),
            }),
            &context(&server),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn delete_with_yes_sends_only_explicit_ids() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/projects/7/issues"))
            .and(body_json(json!({"ids": [UUID]})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"deleted": 1})))
            .expect(1)
            .mount(&server)
            .await;
        run(
            IssueCommand::Delete(crate::cli::IssueDeleteArgs {
                issues: vec![UUID.into()],
                yes: true,
            }),
            &context(&server),
        )
        .await
        .unwrap();
    }

    #[test]
    fn sparkline_handles_zero_and_large_counts() {
        assert_eq!(sparkline(&[]), "");
        assert_eq!(sparkline(&[[0, 0], [1, 0]]), "▁▁");
        assert_eq!(sparkline(&[[0, 0], [1, i64::MAX]]), "▁█");
    }
}
