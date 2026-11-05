//! `openapi.json` against the server that publishes it.
//!
//! `@rustrak/client` is generated from the spec and validates every response
//! with it, so a field the spec calls required that the server omits, or a
//! format it promises and does not keep, is a broken client in production.
//! This calls every `GET` in the spec against seeded data and checks that the
//! status is one the spec documents and that the body matches its schema,
//! with formats (`uuid`, `date-time`) asserted the way the client's zod
//! schemas assert them.
//!
//! It walks the spec rather than a list, so a new `GET` is covered the day it
//! is added. Every `GET` must answer 200 against the seed, so the schema
//! checked is its success body; the few that cannot are in `NOT_SEEDED`.
#![cfg(feature = "openapi")]

use crate::common::TestDb;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::cookie::Key;
use actix_web::{test, web, App};
use chrono::Utc;
use rustrak::config::{Config, DashboardConfig, DatabaseConfig, RateLimitConfig};
use rustrak::models::{CreateAuthToken, CreateProject, CreateRelease, CreateUserRequest, UserRole};
use rustrak::routes;
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{
    AuthTokenService, CleanupJob, EventService, IssueService, ProjectService, ReleaseService,
    UsersService,
};
use rustrak::telemetry::{
    ConfigFacts, Context, Counters, Report, Reporter, Sink, SinkError, TelemetryStatus,
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use utoipa::OpenApi;
use uuid::Uuid;

struct Nowhere;

#[async_trait::async_trait]
impl Sink for Nowhere {
    fn name(&self) -> &'static str {
        "nowhere"
    }
    async fn send(&self, _: &Report) -> Result<(), SinkError> {
        Ok(())
    }
}

const EMAIL: &str = "contract@example.test";
const PASSWORD: &str = "contract-test-password";

/// Paths whose `GET` is a browser redirect, not a JSON API.
const REDIRECTS: &[&str] = &[
    "/api/github/callback",
    "/api/github/installed",
    "/auth/sso/callback",
    "/{org_slug}/{project_slug}/issues/{issue_id}",
];

/// `GET`s that cannot answer 200 against the seed: a row it has no way to
/// create, or a service the test does not configure. Every other `GET` must
/// succeed, so its success schema is the one checked.
const NOT_SEEDED: &[&str] = &[
    // No GitHub App configured.
    "github.repositories",
    // A token only an emailed invitation or an SSO login hands out.
    "auth.getInvitation",
    "auth.ssoLink",
    // Bytes, not JSON, and the seed has no feedback screenshot.
    "events.downloadAttachment",
];

struct Seed {
    project_id: i32,
    project_slug: String,
    token_id: i32,
    issue_id: Uuid,
    issue_numeric_id: i64,
    event_row_id: Uuid,
    sdk_event_id: Uuid,
    transaction_id: Uuid,
    span_row_id: Uuid,
    log_id: Uuid,
    rule_id: i32,
    integration_id: i32,
}

fn test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        database: DatabaseConfig {
            url: "sqlite::memory:".to_string(),
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
        security: rustrak::config::SecurityConfig {
            ssl_proxy: false,
            session_secret_key: None,
        },
        ingest_dir: None,
        public_url: Some("https://rustrak.example.com".to_string()),
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

async fn seed(db: &TestDb) -> (Seed, String) {
    let admin = UsersService::create_user(
        &db.pool,
        &CreateUserRequest {
            email: EMAIL.into(),
            password: PASSWORD.into(),
        },
        UserRole::Admin,
    )
    .await
    .unwrap();
    let token =
        AuthTokenService::create_for_user(&db.pool, CreateAuthToken::default(), Some(admin.id))
            .await
            .unwrap();
    let project = ProjectService::create(
        &db.pool,
        CreateProject {
            name: "Contract".into(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap();
    let fields = DenormalizedFields {
        calculated_type: "TypeError".into(),
        calculated_value: "contract".into(),
        transaction: String::new(),
        last_frame_filename: String::new(),
        last_frame_module: String::new(),
        last_frame_function: String::new(),
        culprit: String::new(),
        logger: String::new(),
        release: String::new(),
    };
    let issue = IssueService::create(
        &db.pool,
        project.id,
        Utc::now(),
        &fields,
        Some("error"),
        Some("javascript"),
    )
    .await
    .unwrap();
    let grouping_id: i32 = sqlx::query_scalar(
        "INSERT INTO groupings (project_id, issue_id, grouping_key, grouping_key_hash) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(project.id)
    .bind(issue.id)
    .bind("contract")
    .bind("0".repeat(64))
    .fetch_one(&db.pool)
    .await
    .unwrap();
    // Version and variant bits a strict `z.uuid()` rejects, as real SDK ids do.
    let sdk_event_id = Uuid::parse_str("0123456789abcdef0123456789abcdef").unwrap();
    let event_row_id = EventService::create(
        &db.pool,
        sdk_event_id,
        project.id,
        issue.id,
        grouping_id,
        &json!({"message": "contract", "level": "error"}),
        Utc::now(),
        &fields,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    ReleaseService::create(
        &db.pool,
        project.id,
        CreateRelease {
            version: CONTRACT_RELEASE.into(),
            reference: None,
            url: None,
        },
    )
    .await
    .unwrap();

    let now = Utc::now();
    let transaction_id = Uuid::new_v4();
    // No `op`: the stat-group route, called without one, matches a null op.
    sqlx::query(
        "INSERT INTO transactions (id, event_id, project_id, trace_id, transaction_name, \
         start_timestamp, timestamp, duration_ms, data) \
         VALUES ($1, $2, $3, $4, 'GET /contract', $5, $5, 12.5, $6)",
    )
    .bind(transaction_id)
    .bind(Uuid::new_v4())
    .bind(project.id)
    .bind("0".repeat(32))
    .bind(now)
    .bind(json!({"transaction": "GET /contract"}))
    .execute(&db.pool)
    .await
    .unwrap();
    let span_row_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO spans (id, transaction_id, project_id, span_id, trace_id, op, \
         start_timestamp, timestamp, duration_ms, tags, data) \
         VALUES ($1, $2, $3, $4, $5, 'db.query', $6, $6, 2.5, $7, $8)",
    )
    .bind(span_row_id)
    .bind(transaction_id)
    .bind(project.id)
    .bind("0".repeat(16))
    .bind("0".repeat(32))
    .bind(now)
    .bind(json!({"k": "v"}))
    .bind(json!({}))
    .execute(&db.pool)
    .await
    .unwrap();
    // An agent run in a conversation, so the agents endpoints answer with rows.
    sqlx::query(
        "INSERT INTO spans (id, project_id, span_id, trace_id, op, start_timestamp, timestamp, \
         duration_ms, data, gen_ai_operation_type, gen_ai_agent_name, gen_ai_conversation_id) \
         VALUES ($1, $2, $3, $4, 'gen_ai.invoke_agent', $5, $5, 3.0, $6, 'agent', 'contract', 'contract')",
    )
    .bind(Uuid::new_v4())
    .bind(project.id)
    .bind("1".repeat(16))
    .bind("1".repeat(32))
    .bind(now)
    .bind(json!({"gen_ai.response.text": "done"}))
    .execute(&db.pool)
    .await
    .unwrap();
    let log_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO logs (id, project_id, level, body, attributes, timestamp) \
         VALUES ($1, $2, 'info', 'contract', $3, $4)",
    )
    .bind(log_id)
    .bind(project.id)
    .bind(json!({}))
    .bind(now)
    .execute(&db.pool)
    .await
    .unwrap();

    let seed = Seed {
        project_id: project.id,
        project_slug: project.slug,
        token_id: token.id,
        issue_id: issue.id,
        issue_numeric_id: issue.numeric_id,
        event_row_id,
        sdk_event_id,
        transaction_id,
        span_row_id,
        log_id,
        rule_id: 0,
        integration_id: 0,
    };
    (seed, token.token)
}

/// A value for a path or query parameter: the seeded row where there is one,
/// otherwise something of the right type that matches nothing.
fn param_value(name: &str, path: &str, schema: &Value, seed: &Seed) -> String {
    match name {
        "project_id" => seed.project_id.to_string(),
        "id" if path.starts_with("/api/projects/") => seed.project_id.to_string(),
        "id" if path.starts_with("/api/tokens/") => seed.token_id.to_string(),
        "id" if path.starts_with("/api/integrations/") => seed.integration_id.to_string(),
        "name" if path.ends_with("/transactions/stats/group") => "GET%20%2Fcontract".into(),
        "issue_id" if path.starts_with("/api/0/") => seed.issue_numeric_id.to_string(),
        "issue_id" => seed.issue_id.to_string(),
        "event_id" if path.contains("/events/sentry/") => seed.sdk_event_id.simple().to_string(),
        "event_id" => seed.event_row_id.to_string(),
        "transaction_id" => seed.transaction_id.to_string(),
        "span_id" => seed.span_row_id.to_string(),
        "log_id" => seed.log_id.to_string(),
        "rule_id" => seed.rule_id.to_string(),
        "slug" | "conversation_id" => "contract".into(),
        "release" => CONTRACT_RELEASE.into(),
        "project_slug" => seed.project_slug.clone(),
        "org_slug" => "rustrak".into(),
        "key" => "level".into(),
        "response_type" => "code".into(),
        "client_id" => CONTRACT_CLIENT.into(),
        "redirect_uri" => CONTRACT_REDIRECT.into(),
        "code_challenge" => "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".into(),
        "code_challenge_method" => "S256".into(),
        _ => match (schema["type"].as_str(), schema["format"].as_str()) {
            (_, Some("uuid")) => Uuid::nil().to_string(),
            (Some("integer"), _) => "1".into(),
            _ => "missing".into(),
        },
    }
}

const CONTRACT_RELEASE: &str = "1.0.0";
const CONTRACT_CLIENT: &str = "https://client.example/metadata.json";
const CONTRACT_REDIRECT: &str = "https://client.example/callback";

/// The one OAuth client the consent endpoints know, without the network.
fn contract_client() -> std::sync::Arc<dyn rustrak::services::oauth::ClientMetadataSource> {
    struct Client;
    #[async_trait::async_trait]
    impl rustrak::services::oauth::ClientMetadataSource for Client {
        async fn fetch(
            &self,
            _: &str,
        ) -> rustrak::error::AppResult<rustrak::services::oauth::ClientMetadata> {
            Ok(serde_json::from_value(json!({
                "client_id": CONTRACT_CLIENT,
                "client_name": "Contract",
                "redirect_uris": [CONTRACT_REDIRECT],
            }))
            .unwrap())
        }
    }
    std::sync::Arc::new(Client)
}

/// Validates `body` against `schema`, resolving `$ref`s inside the spec.
fn schema_errors(spec: &Value, schema: &Value, body: &Value) -> Vec<String> {
    let mut root = schema.clone();
    root["components"] = spec["components"].clone();
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&root)
        .expect("spec schema compiles");
    validator
        .iter_errors(body)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

/// Properties that arrived as `null` but are not `required`.
///
/// utoipa leaves every `Option` field out of `required`, because only serde
/// knows whether `skip_serializing_if` omits it. A field that came back as
/// `null` was serialized, so it is never omitted: it needs
/// `#[schema(required)]`, or the client's types say it may be missing when
/// it never is.
fn null_but_optional(spec: &Value, schema: &Value, value: &Value, at: &str, out: &mut Vec<String>) {
    if let Some(name) = schema["$ref"].as_str() {
        let name = name.rsplit('/').next().unwrap();
        return null_but_optional(spec, &spec["components"]["schemas"][name], value, at, out);
    }
    for key in ["allOf", "oneOf", "anyOf"] {
        for sub in schema[key].as_array().into_iter().flatten() {
            null_but_optional(spec, sub, value, at, out);
        }
    }
    match value {
        Value::Object(fields) => {
            let required = schema["required"].as_array();
            for (name, field) in fields {
                let Some(property) = schema["properties"].get(name) else {
                    continue;
                };
                let listed = required.is_some_and(|r| r.iter().any(|n| n == name));
                if field.is_null() && !listed {
                    out.push(format!("{at}/{name} is null but not required"));
                }
                null_but_optional(spec, property, field, &format!("{at}/{name}"), out);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                null_but_optional(spec, &schema["items"], item, &format!("{at}/{i}"), out);
            }
        }
        _ => {}
    }
}

#[actix_web::test]
async fn every_get_matches_the_published_spec() {
    let db = TestDb::new().await;
    let (mut seed, token) = seed(&db).await;
    let spec = serde_json::to_value(rustrak::openapi::ApiDoc::openapi()).unwrap();
    let reporter = Arc::new(Reporter::new(
        db.pool.clone(),
        Arc::new(Nowhere),
        Box::leak(Box::new(Counters::new())),
        Context {
            dashboard_served: false,
            config: ConfigFacts::default(),
            sqlite_path: None,
            ingest_dir: std::env::temp_dir(),
        },
    ));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db.pool.clone()))
            .app_data(web::Data::new(test_config()))
            .app_data(web::Data::new(CleanupJob::default()))
            .app_data(web::Data::new(reporter))
            .app_data(web::Data::new(TelemetryStatus::Disabled(
                rustrak::telemetry::Disabled::SwitchedOff,
            )))
            .app_data(web::Data::new(None::<rustrak::auth::OidcService>))
            .app_data(web::Data::new(contract_client()))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), Key::generate())
                    .cookie_secure(false)
                    .build(),
            )
            .service(
                web::scope("/health")
                    .route("", web::get().to(routes::health::liveness))
                    .route("/version", web::get().to(routes::health::version))
                    .route("/ready", web::get().to(routes::health::readiness)),
            )
            .configure(routes::auth::configure)
            .configure(routes::events::configure)
            .configure(routes::github::configure)
            .configure(routes::issues::configure)
            .configure(routes::alerts::configure_rules)
            .configure(routes::alerts::configure_history)
            .configure(routes::members::configure)
            .configure(routes::roles::configure)
            .configure(routes::sessions::configure)
            .configure(routes::stats::configure)
            .configure(routes::releases::configure)
            .configure(routes::transactions::configure)
            .configure(routes::logs::configure)
            .configure(routes::monitors::configure)
            .configure(routes::schedules::configure)
            .configure(routes::spans::configure)
            .configure(routes::agents::configure)
            .configure(routes::projects::configure)
            .configure(routes::tokens::configure)
            .configure(routes::team::configure)
            .configure(routes::invitations::configure)
            .configure(routes::alerts::configure_channels)
            .configure(routes::sourcemaps::configure)
            .configure(routes::sentry_api::configure)
            .configure(routes::storage::configure)
            .configure(routes::telemetry::configure)
            .configure(routes::mcp::configure)
            .configure(routes::oauth::configure),
    )
    .await;

    // A session as well as the token, for the routes that only take a cookie.
    let login = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({"email": EMAIL, "password": PASSWORD}))
            .to_request(),
    )
    .await;
    assert_eq!(login.status(), 200, "login");
    let cookie = login
        .response()
        .cookies()
        .next()
        .expect("login sets a session cookie")
        .into_owned();

    // Through the handlers, so these rows are what a real client creates.
    let project = seed.project_id;
    let monitor = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/projects/{project}/monitors"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"name": "Contract", "slug": "contract",
                "config": {"schedule": {"type": "crontab", "value": "0 * * * *"}}}))
            .to_request(),
    )
    .await;
    assert!(
        monitor.status().is_success(),
        "seed monitor: {}",
        monitor.status()
    );
    let rule: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/projects/{project}/alert-rules"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"name": "Contract", "alert_type": "new_issue"}))
            .to_request(),
    )
    .await;
    seed.rule_id = rule["id"].as_i64().expect("seed alert rule") as i32;
    let integration: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/integrations")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .set_json(json!({"name": "Contract", "provider_type": "webhook",
                "credentials": {"url": "https://example.com/hook"}}))
            .to_request(),
    )
    .await;
    seed.integration_id = integration["id"].as_i64().expect("seed integration") as i32;

    let mut failures = Vec::new();
    let mut checked = 0;
    for (path, item) in spec["paths"].as_object().unwrap() {
        let Some(op) = item.get("get") else { continue };
        if REDIRECTS.contains(&path.as_str()) {
            continue;
        }
        let id = op["operationId"].as_str().unwrap();
        let params = op["parameters"].as_array().cloned().unwrap_or_default();

        let mut uri = path.clone();
        let mut query = Vec::new();
        for p in &params {
            let name = p["name"].as_str().unwrap();
            let value = param_value(name, path, &p["schema"], &seed);
            match p["in"].as_str() {
                Some("path") => uri = uri.replace(&format!("{{{name}}}"), &value),
                Some("query") if p["required"] == true => query.push(format!("{name}={value}")),
                _ => {}
            }
        }
        if !query.is_empty() {
            uri = format!("{uri}?{}", query.join("&"));
        }

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&uri)
                .insert_header(("Authorization", format!("Bearer {token}")))
                .cookie(cookie.clone())
                .to_request(),
        )
        .await;
        let status = response.status().as_u16().to_string();
        let body = test::read_body(response).await;
        checked += 1;
        if status != "200" && !NOT_SEEDED.contains(&id) {
            failures.push(format!(
                "{id} GET {uri}: {status}, expected 200 against the seed"
            ));
        }

        let Some(documented) = op["responses"].get(&status) else {
            failures.push(format!("{id} GET {uri}: undocumented status {status}"));
            continue;
        };
        let Some(schema) = documented.pointer("/content/application~1json/schema") else {
            continue;
        };
        let json: Value = match serde_json::from_slice(&body) {
            Ok(json) => json,
            Err(_) => {
                failures.push(format!("{id} GET {uri} {status}: body is not JSON"));
                continue;
            }
        };
        for error in schema_errors(&spec, schema, &json) {
            failures.push(format!("{id} GET {uri} {status}: {error}"));
        }
        let mut loose = Vec::new();
        null_but_optional(&spec, schema, &json, "", &mut loose);
        for error in loose {
            failures.push(format!("{id} GET {uri} {status}: {error}"));
        }
    }

    assert!(checked > 60, "walked only {checked} operations");
    assert!(
        failures.is_empty(),
        "{} response(s) disagree with openapi.json:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Every operation names its `@rustrak/client` method as `resource.method`
/// (`operation_id = "issues.list"`), and no two share one. The client is
/// generated straight from these ids, so a missing or repeated one would
/// produce a flat or colliding method instead of failing loudly.
#[actix_web::test]
async fn every_operation_names_its_client_method() {
    let spec = serde_json::to_value(rustrak::openapi::ApiDoc::openapi()).unwrap();
    let mut seen = std::collections::HashMap::new();
    let mut failures = Vec::new();
    for (path, item) in spec["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            let id = op["operationId"].as_str().unwrap_or_default();
            let valid = id.split_once('.').is_some_and(|(resource, name)| {
                [resource, name].iter().all(|part| {
                    part.starts_with(|c: char| c.is_ascii_lowercase())
                        && part.chars().all(|c| c.is_ascii_alphanumeric())
                })
            });
            if !valid {
                failures.push(format!(
                    "{method} {path}: operation_id \"{id}\" is not resource.method"
                ));
            }
            if let Some(other) = seen.insert(id.to_string(), format!("{method} {path}")) {
                failures.push(format!(
                    "{method} {path}: operation_id \"{id}\" also used by {other}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A project's key is whatever 16 bytes its DSN carries, and a key carried
/// over from Sentry is random hex, not an RFC 4122 UUID. The client turns
/// `format: uuid` into a check of the version and variant digits, which would
/// fail the whole project list for such a project; the formats this file
/// asserts above are too lenient to notice.
#[actix_web::test]
async fn the_spec_does_not_promise_an_rfc_4122_uuid_for_a_project_key() {
    let spec = serde_json::to_value(rustrak::openapi::ApiDoc::openapi()).unwrap();
    let key = &spec["components"]["schemas"]["ProjectResponse"]["properties"]["sentry_key"];

    assert_eq!(key["type"], serde_json::json!(["string", "null"]), "{key}");
    assert!(key.get("format").is_none(), "{key}");
}
