//! Integration tests for the MCP endpoint at `/mcp` and the switch that turns
//! it on.
//!
//! Requests are shaped the way a `2026-07-28` client sends them: stateless,
//! with no `initialize` handshake, the protocol version carried in `_meta`.

use crate::common::TestDb;
use actix_web::{test, web, App};
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig, SecurityConfig};
use rustrak::db::DbPool;
use rustrak::models::{CreateAuthToken, CreateProject, CreateUserRequest, ProjectRole, UserRole};
use rustrak::routes;
use rustrak::services::{AuthTokenService, ProjectMemberService, ProjectService, UsersService};
use serde_json::{json, Value};
use std::time::Duration;

pub(crate) fn create_test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "postgres://test:test@localhost/test".to_string(),
            max_connections: 5,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(300),
        },
        rate_limit: RateLimitConfig {
            max_events_per_minute: 1000,
            max_events_per_hour: 10000,
            max_events_per_project_per_minute: 500,
            max_events_per_project_per_hour: 5000,
        },
        security: SecurityConfig {
            ssl_proxy: false,
            session_secret_key: None,
        },
        ingest_dir: None,
        public_url: None,
        sourcemap_storage_path: "/tmp/test_sourcemaps".to_string(),
        sourcemap_cache_bytes: 64 * 1024 * 1024,
        max_chunk_size_bytes: 10 * 1024 * 1024,
        session_flush_interval_secs: 30,
        session_cardinality_cap: 10_000,
        dashboard: DashboardConfig {
            dir: "./static".to_string(),
            enabled: true,
            url: None,
        },
        telemetry: rustrak::config::TelemetryConfig {
            enabled: false,
            do_not_track: false,
        },
    }
}

macro_rules! build_app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool))
                .app_data(web::Data::new(create_test_config()))
                .configure(routes::mcp::configure),
        )
        .await
    };
}

async fn seed_legacy_token(pool: &DbPool) -> String {
    AuthTokenService::create(pool, CreateAuthToken { description: None })
        .await
        .expect("seed legacy token")
        .token
}

async fn seed_member_token(pool: &DbPool, email: &str) -> (i32, String) {
    let user = UsersService::create_user(
        pool,
        &CreateUserRequest {
            email: email.to_string(),
            password: "password123".to_string(),
        },
        UserRole::Member,
    )
    .await
    .expect("seed user");
    let token = AuthTokenService::create_for_user(
        pool,
        CreateAuthToken { description: None },
        Some(user.id),
    )
    .await
    .expect("seed token")
    .token;
    (user.id, token)
}

async fn seed_project(pool: &DbPool, name: &str) -> i32 {
    ProjectService::create(
        pool,
        CreateProject {
            name: name.to_string(),
            slug: None,
            platform: None,
        },
    )
    .await
    .expect("seed project")
    .id
}

pub(crate) fn set_mcp(token: &str, enabled: bool) -> test::TestRequest {
    test::TestRequest::put()
        .uri("/api/settings/mcp")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({ "enabled": enabled }))
}

pub(crate) fn call_tool(token: Option<&str>, name: &str, arguments: Value) -> test::TestRequest {
    let mut req = test::TestRequest::post()
        .uri("/mcp")
        .insert_header(("Content-Type", "application/json"))
        .insert_header(("Accept", "application/json, text/event-stream"))
        .insert_header(("MCP-Protocol-Version", "2026-07-28"))
        .insert_header(("Mcp-Method", "tools/call"))
        .insert_header(("Mcp-Name", name))
        .set_payload(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments,
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "1.0" },
                        "io.modelcontextprotocol/clientCapabilities": {}
                    }
                }
            })
            .to_string(),
        );
    if let Some(token) = token {
        req = req.insert_header(("Authorization", format!("Bearer {token}")));
    }
    req
}

/// The JSON a tool printed, out of the JSON-RPC envelope around it.
fn tool_output(body: &Value) -> Value {
    let text = body["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text content in {body}"));
    serde_json::from_str(text).expect("tool output is JSON")
}

#[actix_web::test]
async fn mcp_is_off_until_an_admin_turns_it_on() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let (_, member) = seed_member_token(&db.pool, "member@example.com").await;
    let app = build_app!(db.pool.clone());

    let resp = test::call_service(
        &app,
        call_tool(Some(&admin), "list_projects", json!({})).to_request(),
    )
    .await;
    assert_eq!(
        resp.status(),
        404,
        "a fresh install does not answer on /mcp"
    );

    let resp = test::call_service(&app, set_mcp(&member, true).to_request()).await;
    assert_eq!(resp.status(), 403, "only an instance admin may turn it on");

    let resp = test::call_service(&app, set_mcp(&admin, true).to_request()).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/settings/mcp")
        .insert_header(("Authorization", format!("Bearer {admin}")))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    // No PUBLIC_URL, so no URL to vouch for: the dashboard falls back to its
    // own origin rather than advertise the bind address.
    assert_eq!(body, json!({ "enabled": true, "url": null }));

    let resp = test::call_service(
        &app,
        call_tool(Some(&admin), "list_projects", json!({})).to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);

    test::call_service(&app, set_mcp(&admin, false).to_request()).await;
    let resp = test::call_service(
        &app,
        call_tool(Some(&admin), "list_projects", json!({})).to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404, "turning it off closes it again");
}

#[actix_web::test]
async fn mcp_requires_a_bearer_token() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let resp = test::call_service(
        &app,
        call_tool(None, "list_projects", json!({})).to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn list_projects_only_shows_the_callers_projects() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let (user_id, member) = seed_member_token(&db.pool, "member@example.com").await;
    let mine = seed_project(&db.pool, "Mine").await;
    seed_project(&db.pool, "Not mine").await;
    ProjectMemberService::upsert(&db.pool, mine, user_id, ProjectRole::Viewer)
        .await
        .unwrap();
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        call_tool(Some(&member), "list_projects", json!({})).to_request(),
    )
    .await;
    let output = tool_output(&body);
    let names: Vec<&str> = output["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Mine"]);
}

#[actix_web::test]
async fn a_project_the_caller_cannot_see_is_a_tool_error_not_a_leak() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let (_, member) = seed_member_token(&db.pool, "member@example.com").await;
    let hidden = seed_project(&db.pool, "Hidden").await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        call_tool(
            Some(&member),
            "list_issues",
            json!({ "project_id": hidden }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(body["result"]["isError"], json!(true), "{body}");
}

#[actix_web::test]
async fn list_issues_and_get_issue_answer_for_a_visible_project() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let project = seed_project(&db.pool, "Shop").await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        call_tool(
            Some(&admin),
            "list_issues",
            json!({ "project_id": project }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(tool_output(&body)["total_count"], json!(0), "{body}");

    let body: Value = test::call_and_read_body_json(
        &app,
        call_tool(
            Some(&admin),
            "get_issue",
            json!({ "project_id": project, "issue_id": uuid::Uuid::new_v4() }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(body["result"]["isError"], json!(true), "{body}");
}

fn list_tools(token: &str) -> test::TestRequest {
    test::TestRequest::post()
        .uri("/mcp")
        .insert_header(("Content-Type", "application/json"))
        .insert_header(("Accept", "application/json, text/event-stream"))
        .insert_header(("MCP-Protocol-Version", "2026-07-28"))
        .insert_header(("Mcp-Method", "tools/list"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_payload(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list",
                "params": {
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "1.0" },
                        "io.modelcontextprotocol/clientCapabilities": {}
                    }
                }
            })
            .to_string(),
        )
}

async fn seed_issue(pool: &DbPool, project_id: i32) -> uuid::Uuid {
    let denormalized = rustrak::services::grouping::DenormalizedFields {
        calculated_type: "TypeError".to_string(),
        calculated_value: "boom".to_string(),
        transaction: "/api/test".to_string(),
        last_frame_filename: "test.rs".to_string(),
        last_frame_module: "test_module".to_string(),
        last_frame_function: "test_function".to_string(),
        culprit: "test_function".to_string(),
        logger: String::new(),
        release: String::new(),
    };
    rustrak::services::IssueService::create(
        pool,
        project_id,
        chrono::Utc::now(),
        &denormalized,
        Some("error"),
        Some("rust"),
    )
    .await
    .expect("seed issue")
    .id
}

/// The whole surface the npm package served, so dropping a tool is a test
/// failure rather than a silent regression for every connected assistant.
const TOOLS: [&str; 82] = [
    "assign_issue",
    "bookmark_issue",
    "bulk_delete_issues",
    "bulk_update_issues",
    "comment_on_issue",
    "create_alert_channel",
    "create_invitation",
    "create_monitor",
    "create_project",
    "create_token",
    "delete_issue",
    "delete_monitor",
    "execute_storage_cleanup",
    "gc_storage_source_maps",
    "get_agent_duration",
    "get_agent_models_by_calls",
    "get_agent_models_by_tokens",
    "get_agent_runs",
    "get_agent_tools",
    "get_attachment",
    "get_error_volume",
    "get_event",
    "get_issue",
    "get_issue_activity",
    "get_issue_aggregates",
    "get_issue_hashes",
    "get_issue_stats",
    "get_issue_tag_values",
    "get_monitor",
    "get_project",
    "get_project_stats",
    "get_release_health",
    "get_retention",
    "get_server_version",
    "get_storage_by_project",
    "get_storage_cleanup_status",
    "get_storage_summary",
    "get_token",
    "get_transaction",
    "get_transaction_spans",
    "get_transaction_stats",
    "list_agent_traces",
    "list_alert_channels",
    "list_alert_rules",
    "list_events",
    "list_feedback",
    "list_invitations",
    "list_issues",
    "list_logs",
    "list_monitor_checkins",
    "list_monitors",
    "list_project_members",
    "list_projects",
    "list_spans",
    "list_team_members",
    "list_tokens",
    "list_transactions",
    "list_user_reports",
    "mark_issue_seen",
    "mute_issue",
    "preview_alert_template",
    "preview_retention",
    "preview_schedule",
    "preview_storage_cleanup",
    "preview_storage_source_maps_gc",
    "remove_project_member",
    "remove_team_member",
    "resend_invitation",
    "resolve_issue",
    "revoke_invitation",
    "revoke_token",
    "run_retention",
    "set_project_member",
    "submit_user_report",
    "subscribe_issue",
    "test_alert_channel",
    "unresolve_issue",
    "update_alert_channel",
    "update_issue_status",
    "update_member_role",
    "update_monitor",
    "update_retention",
];

#[actix_web::test]
async fn tools_list_serves_every_tool_with_its_hints() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let body: Value = test::call_and_read_body_json(&app, list_tools(&admin).to_request()).await;
    let tools = body["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"));
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(names, TOOLS);

    let tool = |name: &str| tools.iter().find(|t| t["name"] == name).unwrap();
    assert_eq!(
        tool("list_issues")["annotations"]["readOnlyHint"],
        json!(true)
    );
    assert_eq!(
        tool("delete_issue")["annotations"]["destructiveHint"],
        json!(true)
    );
    assert_eq!(
        tool("resolve_issue")["annotations"]["destructiveHint"],
        json!(false)
    );
    let required = &tool("get_event")["inputSchema"]["required"];
    for field in ["project_id", "issue_id", "event_id"] {
        assert!(
            required.as_array().unwrap().contains(&json!(field)),
            "{required}"
        );
    }
}

#[actix_web::test]
async fn a_write_tool_goes_through_the_rest_handler() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let project = seed_project(&db.pool, "Shop").await;
    let issue = seed_issue(&db.pool, project).await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        call_tool(
            Some(&admin),
            "resolve_issue",
            json!({ "project_id": project, "issue_id": issue }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(tool_output(&body)["status"], json!("resolved"), "{body}");
}

#[actix_web::test]
async fn a_member_cannot_resolve_an_issue_in_a_project_they_only_view() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let (user_id, member) = seed_member_token(&db.pool, "viewer@example.com").await;
    let project = seed_project(&db.pool, "Shop").await;
    ProjectMemberService::upsert(&db.pool, project, user_id, ProjectRole::Viewer)
        .await
        .unwrap();
    let issue = seed_issue(&db.pool, project).await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        call_tool(
            Some(&member),
            "resolve_issue",
            json!({ "project_id": project, "issue_id": issue }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(body["result"]["isError"], json!(true), "{body}");
}

#[actix_web::test]
async fn an_unconfirmed_destructive_tool_is_refused() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        call_tool(
            Some(&admin),
            "execute_storage_cleanup",
            json!({ "older_than_days": 1 }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(body["result"]["isError"], json!(true), "{body}");
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("confirm=true"), "{text}");
}

#[actix_web::test]
async fn a_tool_whose_handler_answers_no_content_says_what_it_did() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let doomed = AuthTokenService::create(&db.pool, CreateAuthToken { description: None })
        .await
        .unwrap();
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        call_tool(
            Some(&admin),
            "revoke_token",
            json!({ "token_id": doomed.id }),
        )
        .to_request(),
    )
    .await;
    assert_eq!(
        body["result"]["content"][0]["text"],
        json!(format!("Token {} revoked successfully.", doomed.id)),
        "{body}"
    );
}

#[actix_web::test]
async fn a_tool_without_arguments_answers() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        call_tool(Some(&admin), "get_server_version", json!({})).to_request(),
    )
    .await;
    assert_eq!(
        tool_output(&body)["version"],
        json!(env!("CARGO_PKG_VERSION")),
        "{body}"
    );
}

#[actix_web::test]
async fn a_legacy_get_stream_is_405_not_404() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let req = test::TestRequest::get()
        .uri("/mcp")
        .insert_header(("Accept", "text/event-stream"))
        .insert_header(("Authorization", format!("Bearer {admin}")))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 405);
}

/// A `2025-06-18` client, which most assistants still are: it shakes hands
/// with `initialize`, then calls in separate requests. The server stays
/// stateless for it too, so no session id comes back and none is needed.
#[actix_web::test]
async fn a_legacy_client_initializes_then_lists_and_calls_tools() {
    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    seed_project(&db.pool, "Shop").await;
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let legacy = |body: Value| {
        test::TestRequest::post()
            .uri("/mcp")
            .insert_header(("Content-Type", "application/json"))
            .insert_header(("Accept", "application/json, text/event-stream"))
            .insert_header(("MCP-Protocol-Version", "2025-06-18"))
            .insert_header(("Authorization", format!("Bearer {admin}")))
            .set_payload(body.to_string())
            .to_request()
    };

    let resp = test::call_service(
        &app,
        legacy(json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "legacy", "version": "1.0" }
            }
        })),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers().get("mcp-session-id").is_none(),
        "stateless: no session to carry"
    );
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["result"]["protocolVersion"], "2025-06-18", "{body}");
    assert_eq!(body["result"]["serverInfo"]["name"], "rustrak", "{body}");

    let body: Value = test::call_and_read_body_json(
        &app,
        legacy(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} })),
    )
    .await;
    assert_eq!(
        body["result"]["tools"].as_array().map(Vec::len),
        Some(TOOLS.len()),
        "{body}"
    );

    let body: Value = test::call_and_read_body_json(
        &app,
        legacy(json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "list_projects", "arguments": {} }
        })),
    )
    .await;
    assert_eq!(tool_output(&body)["items"][0]["name"], "Shop", "{body}");
}

/// `list_agent_traces` takes the traces list's filters and rankings: the
/// tool calls the handler, so they mean what they mean on the dashboard.
#[actix_web::test]
async fn list_agent_traces_narrows_and_ranks() {
    use rustrak::digest::processors::{Processor, ProcessorCtx, SpanProcessor};

    let db = TestDb::new().await;
    let admin = seed_legacy_token(&db.pool).await;
    let project = seed_project(&db.pool, "Agents").await;
    let ctx = ProcessorCtx {
        pool: db.pool.clone(),
        project_id: project,
        event_id: uuid::Uuid::nil(),
        ingested_at: chrono::Utc::now(),
        remote_addr: None,
        keep_ai_spans: true,
    };
    for (span, trace, tool, tokens) in [
        ("a1", "small", "search", 10),
        ("b1", "large", "refund", 900),
    ] {
        for body in [
            json!({"span_id": format!("{span}00000000000000"), "trace_id": trace, "start_timestamp": 1.0, "timestamp": 2.0,
                   "data": {"gen_ai.operation.type": "tool", "gen_ai.tool.name": tool}}),
            json!({"span_id": format!("{span}11111111111111"), "trace_id": trace, "start_timestamp": 1.0, "timestamp": 2.0,
                   "data": {"gen_ai.operation.type": "ai_client", "gen_ai.request.model": "gpt-4o", "gen_ai.usage.total_tokens": tokens}}),
        ] {
            SpanProcessor
                .process(bytes::Bytes::from(serde_json::to_vec(&body).unwrap()), &ctx)
                .await
                .unwrap();
        }
    }
    let app = build_app!(db.pool.clone());
    test::call_service(&app, set_mcp(&admin, true).to_request()).await;

    let ids = |body: &Value| -> Vec<String> {
        tool_output(body)["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["trace_id"].as_str().unwrap().to_string())
            .collect()
    };
    let call =
        |arguments: Value| call_tool(Some(&admin), "list_agent_traces", arguments).to_request();

    let body: Value =
        test::call_and_read_body_json(&app, call(json!({"project_id": project, "tool": "refund"})))
            .await;
    assert_eq!(ids(&body), ["large"], "{body}");
    let body: Value =
        test::call_and_read_body_json(&app, call(json!({"project_id": project, "sort": "tokens"})))
            .await;
    assert_eq!(ids(&body), ["large", "small"], "{body}");
}
