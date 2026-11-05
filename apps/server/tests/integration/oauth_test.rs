//! OAuth 2.1 for `/mcp`: Rustrak as its own authorization server, so a web
//! assistant can connect with the user's consent instead of a pasted token.
//!
//! The client's metadata document is served by a fake source, since the real
//! one fetches over HTTPS from the public internet.

use std::sync::Arc;

use actix_http::Request;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::cookie::{Cookie, Key};
use actix_web::dev::{Service, ServiceResponse};
use actix_web::{test, web, App};
use base64::Engine;
use rustrak::config::Config;
use rustrak::db::DbPool;
use rustrak::error::{AppError, AppResult};
use rustrak::models::{CreateAuthToken, CreateProject, CreateUserRequest, UserRole};
use rustrak::routes;
use rustrak::services::oauth::{ClientMetadata, ClientMetadataSource};
use rustrak::services::{AuthTokenService, ProjectService, UsersService};
use serde_json::{json, Value};
use sha2::Digest;

use crate::common::TestDb;

const CLIENT_ID: &str = "https://client.example/oauth/metadata.json";
const REDIRECT: &str = "https://client.example/callback";
const ISSUER: &str = "http://localhost:8080";
const RESOURCE: &str = "http://localhost:8080/mcp";
const VERIFIER: &str = "a-verifier-that-is-long-enough-to-be-valid-0123456789";

/// A client whose document registers a script as its redirect.
const SCRIPT_CLIENT: &str = "https://evil.example/oauth/metadata.json";
const SCRIPT_REDIRECT: &str = "javascript:alert(document.domain)//";

/// A client whose document names it with more than the 255 characters a
/// connection's name may hold.
const LONG_NAME_CLIENT: &str = "https://long.example/oauth/metadata.json";

/// Serves these clients' metadata, and refuses every other `client_id`.
struct FakeClients;

#[async_trait::async_trait]
impl ClientMetadataSource for FakeClients {
    async fn fetch(&self, client_id: &str) -> AppResult<ClientMetadata> {
        let (redirect, name) = match client_id {
            CLIENT_ID => (REDIRECT, "Example Assistant".to_string()),
            SCRIPT_CLIENT => (SCRIPT_REDIRECT, "Example Assistant".to_string()),
            LONG_NAME_CLIENT => (REDIRECT, "x".repeat(256)),
            _ => return Err(AppError::Validation("unknown client".into())),
        };
        Ok(serde_json::from_value(json!({
            "client_id": client_id,
            "client_name": name,
            "redirect_uris": [redirect],
            "token_endpoint_auth_method": "none"
        }))
        .unwrap())
    }
}

fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier))
}

async fn app(
    pool: &DbPool,
) -> impl Service<Request, Response = ServiceResponse, Error = actix_web::Error> {
    app_with(pool, Some(ISSUER)).await
}

async fn app_with(
    pool: &DbPool,
    public_url: Option<&str>,
) -> impl Service<Request, Response = ServiceResponse, Error = actix_web::Error> {
    app_configured(
        pool,
        Config {
            public_url: public_url.map(str::to_string),
            ..crate::integration::mcp_test::create_test_config()
        },
    )
    .await
}

async fn app_configured(
    pool: &DbPool,
    config: Config,
) -> impl Service<Request, Response = ServiceResponse, Error = actix_web::Error> {
    let clients: Arc<dyn ClientMetadataSource> = Arc::new(FakeClients);
    test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(clients))
            .app_data(web::Data::new(None::<rustrak::auth::OidcService>))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), Key::from(&[0u8; 64]))
                    .cookie_secure(false)
                    .build(),
            )
            .configure(routes::auth::configure)
            .configure(routes::projects::configure)
            .configure(routes::mcp::configure)
            .configure(routes::oauth::configure),
    )
    .await
}

/// A member who can see one project, signed in to the dashboard.
async fn signed_in_member<S>(app: &S, pool: &DbPool) -> Cookie<'static>
where
    S: Service<Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    let user = UsersService::create_user(
        pool,
        &CreateUserRequest {
            email: "member@example.com".into(),
            password: "password123".into(),
        },
        UserRole::Member,
    )
    .await
    .unwrap();
    for name in ["Mine", "Not mine"] {
        let project = ProjectService::create(
            pool,
            CreateProject {
                name: name.into(),
                slug: None,
                platform: None,
            },
        )
        .await
        .unwrap();
        if name == "Mine" {
            crate::common::grant_project_role(pool, project.id, user.id, "Viewer")
                .await
                .unwrap();
        }
    }
    let resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(json!({ "email": "member@example.com", "password": "password123" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    resp.response()
        .cookies()
        .find(|c| c.name() == "id")
        .unwrap()
        .into_owned()
}

async fn enable_mcp<S>(app: &S, pool: &DbPool)
where
    S: Service<Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    let admin = AuthTokenService::create(pool, CreateAuthToken::default())
        .await
        .unwrap()
        .token;
    let resp = test::call_service(
        app,
        test::TestRequest::put()
            .uri("/api/settings/mcp")
            .insert_header(("Authorization", format!("Bearer {admin}")))
            .set_json(json!({ "enabled": true }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
}

fn authorization() -> Value {
    json!({
        "response_type": "code",
        "client_id": CLIENT_ID,
        "redirect_uri": REDIRECT,
        "code_challenge": challenge(VERIFIER),
        "code_challenge_method": "S256",
        "state": "xyz",
        "resource": RESOURCE,
    })
}

fn query(params: &Value) -> String {
    params
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencode(v.as_str().unwrap())))
        .collect::<Vec<_>>()
        .join("&")
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Approves (or denies) an authorization request as the signed-in user, and
/// returns where the browser is sent.
async fn decide<S>(app: &S, cookie: &Cookie<'static>, params: Value, approve: bool) -> url::Url
where
    S: Service<Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    let mut body = params;
    body["approve"] = json!(approve);
    let resp: Value = test::call_and_read_body_json(
        app,
        test::TestRequest::post()
            .uri("/api/oauth/authorize")
            .cookie(cookie.clone())
            .set_json(body)
            .to_request(),
    )
    .await;
    url::Url::parse(
        resp["redirect_to"]
            .as_str()
            .unwrap_or_else(|| panic!("{resp}")),
    )
    .unwrap()
}

fn param(url: &url::Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}

async fn token<S>(app: &S, form: &[(&str, &str)]) -> (u16, Value)
where
    S: Service<Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    let resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/api/oauth/token")
            .set_form(form)
            .to_request(),
    )
    .await;
    let status = resp.status().as_u16();
    (status, test::read_body_json(resp).await)
}

async fn exchange<S>(app: &S, code: &str) -> (u16, Value)
where
    S: Service<Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    token(
        app,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT),
            ("client_id", CLIENT_ID),
            ("code_verifier", VERIFIER),
            ("resource", RESOURCE),
        ],
    )
    .await
}

/// Calls `list_projects` on `/mcp` with a bearer token and returns the status
/// and the project names it saw.
async fn list_projects<S>(app: &S, bearer: &str) -> (u16, Vec<String>)
where
    S: Service<Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    let resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/mcp")
            .insert_header(("Content-Type", "application/json"))
            .insert_header(("Accept", "application/json, text/event-stream"))
            .insert_header(("MCP-Protocol-Version", "2026-07-28"))
            .insert_header(("Mcp-Method", "tools/call"))
            .insert_header(("Mcp-Name", "list_projects"))
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .set_payload(
                json!({
                    "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": {
                        "name": "list_projects", "arguments": {},
                        "_meta": {
                            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                            "io.modelcontextprotocol/clientInfo": { "name": "t", "version": "1" },
                            "io.modelcontextprotocol/clientCapabilities": {}
                        }
                    }
                })
                .to_string(),
            )
            .to_request(),
    )
    .await;
    let status = resp.status().as_u16();
    if status != 200 {
        return (status, vec![]);
    }
    let body: Value = test::read_body_json(resp).await;
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let page: Value = serde_json::from_str(text).unwrap();
    let names = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    (status, names)
}

#[actix_web::test]
async fn discovery_documents_point_the_client_at_this_server() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;

    let resource: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/.well-known/oauth-protected-resource/mcp")
            .to_request(),
    )
    .await;
    assert_eq!(resource["resource"], RESOURCE);
    assert_eq!(resource["authorization_servers"], json!([ISSUER]));

    let server: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/.well-known/oauth-authorization-server")
            .to_request(),
    )
    .await;
    assert_eq!(server["issuer"], ISSUER);
    assert_eq!(
        server["authorization_endpoint"],
        format!("{ISSUER}/oauth/authorize")
    );
    assert_eq!(
        server["token_endpoint"],
        format!("{ISSUER}/api/oauth/token")
    );
    assert_eq!(server["code_challenge_methods_supported"], json!(["S256"]));
    assert_eq!(
        server["token_endpoint_auth_methods_supported"],
        json!(["none"])
    );
    assert_eq!(server["client_id_metadata_document_supported"], json!(true));
    assert_eq!(
        server["authorization_response_iss_parameter_supported"],
        json!(true)
    );
}

#[actix_web::test]
async fn an_unauthenticated_mcp_call_says_where_to_authorize() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    enable_mcp(&app, &db.pool).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/mcp")
            .set_payload("{}")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);
    let challenge = resp
        .headers()
        .get("WWW-Authenticate")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(
        challenge,
        format!("Bearer resource_metadata=\"{ISSUER}/.well-known/oauth-protected-resource/mcp\"")
    );
}

#[actix_web::test]
async fn the_full_flow_gives_the_assistant_the_users_own_access() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    enable_mcp(&app, &db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;

    // The consent page asks who is asking before showing anything.
    let info: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/oauth/authorize?{}", query(&authorization())))
            .cookie(cookie.clone())
            .to_request(),
    )
    .await;
    assert_eq!(info["client_name"], "Example Assistant");
    assert_eq!(info["redirect_host"], "client.example");

    let back = decide(&app, &cookie, authorization(), true).await;
    assert_eq!(back.host_str(), Some("client.example"));
    assert_eq!(param(&back, "state").as_deref(), Some("xyz"));
    assert_eq!(param(&back, "iss").as_deref(), Some(ISSUER));
    let code = param(&back, "code").unwrap();

    let (status, tokens) = exchange(&app, &code).await;
    assert_eq!(status, 200, "{tokens}");
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["expires_in"], 3600);
    let access = tokens["access_token"].as_str().unwrap().to_string();
    let refresh = tokens["refresh_token"].as_str().unwrap().to_string();

    assert_eq!(
        list_projects(&app, &access).await,
        (200, vec!["Mine".to_string()])
    );

    // A code works once.
    let (status, error) = exchange(&app, &code).await;
    assert_eq!(
        (status, error["error"].clone()),
        (400, json!("invalid_grant"))
    );

    // The token is for /mcp and nothing else.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/projects")
            .insert_header(("Authorization", format!("Bearer {access}")))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);

    // Refreshing rotates: the new pair works, the old refresh token does not.
    let (status, renewed) = token(
        &app,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", CLIENT_ID),
        ],
    )
    .await;
    assert_eq!(status, 200, "{renewed}");
    let new_access = renewed["access_token"].as_str().unwrap();
    assert_ne!(new_access, access);
    assert_eq!(list_projects(&app, new_access).await.0, 200);
    assert_eq!(
        list_projects(&app, &access).await.0,
        401,
        "the old access token is gone"
    );
    let (status, error) = token(
        &app,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", CLIENT_ID),
        ],
    )
    .await;
    assert_eq!(
        (status, error["error"].clone()),
        (400, json!("invalid_grant"))
    );
}

#[actix_web::test]
async fn a_wrong_code_verifier_is_refused() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;
    let code = param(&decide(&app, &cookie, authorization(), true).await, "code").unwrap();

    let (status, error) = token(
        &app,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT),
            ("client_id", CLIENT_ID),
            (
                "code_verifier",
                "another-verifier-that-is-also-long-enough-0123456789",
            ),
        ],
    )
    .await;
    assert_eq!(
        (status, error["error"].clone()),
        (400, json!("invalid_grant"))
    );
}

#[actix_web::test]
async fn denying_sends_the_user_back_with_access_denied() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;

    let back = decide(&app, &cookie, authorization(), false).await;
    assert_eq!(param(&back, "error").as_deref(), Some("access_denied"));
    assert_eq!(param(&back, "state").as_deref(), Some("xyz"));
    assert_eq!(param(&back, "iss").as_deref(), Some(ISSUER));
    assert!(param(&back, "code").is_none());
}

#[actix_web::test]
async fn a_request_that_cannot_be_trusted_is_never_redirected() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;

    let mut cases = vec![];
    let mut unregistered = authorization();
    unregistered["redirect_uri"] = json!("https://attacker.example/steal");
    cases.push(("an unregistered redirect_uri", unregistered));
    let mut script = authorization();
    script["client_id"] = json!(SCRIPT_CLIENT);
    script["redirect_uri"] = json!(SCRIPT_REDIRECT);
    cases.push(("a redirect that runs script", script));
    let mut long_name = authorization();
    long_name["client_id"] = json!(LONG_NAME_CLIENT);
    cases.push(("a name longer than a connection can hold", long_name));
    let mut long_redirect = authorization();
    long_redirect["redirect_uri"] = json!(format!("{REDIRECT}?{}", "x".repeat(2048)));
    cases.push((
        "a redirect longer than a connection can hold",
        long_redirect,
    ));
    let mut unknown = authorization();
    unknown["client_id"] = json!("https://unknown.example/metadata.json");
    cases.push(("an unknown client", unknown));
    let mut plain = authorization();
    plain["code_challenge_method"] = json!("plain");
    cases.push(("PKCE without S256", plain));
    let mut other = authorization();
    other["resource"] = json!("https://elsewhere.example/mcp");
    cases.push(("a token for another resource", other));

    for (what, params) in cases {
        let mut body = params;
        body["approve"] = json!(true);
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/oauth/authorize")
                .cookie(cookie.clone())
                .set_json(body)
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 400, "{what}");
    }
}

#[actix_web::test]
async fn consent_needs_a_signed_in_user() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    let mut body = authorization();
    body["approve"] = json!(true);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/oauth/authorize")
            .set_json(body)
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn a_revoked_connection_loses_access_at_once() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    enable_mcp(&app, &db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;
    let code = param(&decide(&app, &cookie, authorization(), true).await, "code").unwrap();
    let access = exchange(&app, &code).await.1["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    let grants: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/oauth/grants")
            .cookie(cookie.clone())
            .to_request(),
    )
    .await;
    assert_eq!(grants[0]["client_name"], "Example Assistant");
    let id = grants[0]["id"].as_i64().unwrap();

    let resp = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/api/oauth/grants/{id}"))
            .cookie(cookie.clone())
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 204);
    assert_eq!(list_projects(&app, &access).await.0, 401);
}

#[actix_web::test]
async fn a_disabled_account_loses_access_at_once() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    enable_mcp(&app, &db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;
    let code = param(&decide(&app, &cookie, authorization(), true).await, "code").unwrap();
    let access = exchange(&app, &code).await.1["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    sqlx::query("UPDATE users SET is_active = FALSE")
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(list_projects(&app, &access).await.0, 401);
}

#[actix_web::test]
async fn an_expired_code_is_refused_and_swept() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;
    let code = param(&decide(&app, &cookie, authorization(), true).await, "code").unwrap();

    sqlx::query("UPDATE oauth_codes SET expires_at = $1")
        .bind(chrono::Utc::now() - chrono::Duration::minutes(1))
        .execute(&db.pool)
        .await
        .unwrap();
    let (status, error) = exchange(&app, &code).await;
    assert_eq!(
        (status, error["error"].clone()),
        (400, json!("invalid_grant"))
    );

    // An expired code someone never redeemed goes with the next authorization.
    decide(&app, &cookie, authorization(), true).await;
    sqlx::query("UPDATE oauth_codes SET expires_at = $1")
        .bind(chrono::Utc::now() - chrono::Duration::minutes(1))
        .execute(&db.pool)
        .await
        .unwrap();
    decide(&app, &cookie, authorization(), true).await;
    let (left,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM oauth_codes")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(left, 1);
}

#[actix_web::test]
async fn a_signed_out_caller_is_told_to_sign_in_before_anything_else() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/oauth/authorize?response_type=code")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn without_a_public_url_there_is_no_oauth_to_discover() {
    let db = TestDb::new().await;
    let app = app_with(&db.pool, None).await;
    enable_mcp(&app, &db.pool).await;

    // The issuer has to be an address assistants can reach. The bind address
    // (`0.0.0.0` by default) is not one, and a request's forwarded headers
    // are the caller's to choose, so without PUBLIC_URL there is none.
    for path in [
        "/.well-known/oauth-authorization-server",
        "/.well-known/oauth-protected-resource/mcp",
    ] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(path)
                .insert_header(("X-Forwarded-Host", "evil.example"))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 404, "{path}");
    }

    // /mcp still asks for a token, and points nowhere to get one.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/mcp")
            .set_payload("{}")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 401);
    assert_eq!(resp.headers().get("WWW-Authenticate").unwrap(), "Bearer");
}

fn refresh_form(token: &str) -> [(&str, &str); 3] {
    [
        ("grant_type", "refresh_token"),
        ("refresh_token", token),
        ("client_id", CLIENT_ID),
    ]
}

#[actix_web::test]
async fn a_reused_refresh_token_disconnects_the_assistant() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    enable_mcp(&app, &db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;
    let code = param(&decide(&app, &cookie, authorization(), true).await, "code").unwrap();
    let first = exchange(&app, &code).await.1;
    // Whoever refreshes first gets a new pair...
    let old = first["refresh_token"].as_str().unwrap();
    let rotated = token(&app, &refresh_form(old)).await.1;
    let access = rotated["access_token"].as_str().unwrap();
    assert_eq!(list_projects(&app, access).await.0, 200);

    // ...and when the spent token comes back, one of the two holders stole
    // it. Which one cannot be told, so the connection ends for both.
    let (status, error) = token(&app, &refresh_form(old)).await;
    assert_eq!(
        (status, error["error"].clone()),
        (400, json!("invalid_grant"))
    );
    assert_eq!(list_projects(&app, access).await.0, 401);
    let newest = rotated["refresh_token"].as_str().unwrap();
    let (status, _) = token(&app, &refresh_form(newest)).await;
    assert_eq!(status, 400);
}

#[actix_web::test]
async fn consent_happens_where_the_dashboard_is_served() {
    let db = TestDb::new().await;
    let mut config = crate::integration::mcp_test::create_test_config();
    config.public_url = Some(ISSUER.into());
    config.dashboard.url = Some("https://dashboard.example".into());
    let app = app_configured(&db.pool, config).await;

    let server: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/.well-known/oauth-authorization-server")
            .to_request(),
    )
    .await;
    // The consent page is a dashboard route; the issuer and the token
    // endpoint stay with the API.
    assert_eq!(
        server["authorization_endpoint"],
        "https://dashboard.example/oauth/authorize"
    );
    assert_eq!(server["issuer"], ISSUER);
    assert_eq!(
        server["token_endpoint"],
        format!("{ISSUER}/api/oauth/token")
    );
}

#[actix_web::test]
async fn a_refresh_token_spent_generations_ago_still_disconnects() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    enable_mcp(&app, &db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;
    let code = param(&decide(&app, &cookie, authorization(), true).await, "code").unwrap();
    let stolen = exchange(&app, &code).await.1["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();

    // The thief rotates twice before the client next refreshes...
    let second = token(&app, &refresh_form(&stolen)).await.1;
    let third = token(
        &app,
        &refresh_form(second["refresh_token"].as_str().unwrap()),
    )
    .await
    .1;
    let access = third["access_token"].as_str().unwrap();
    assert_eq!(list_projects(&app, access).await.0, 200);

    // ...and the client's old token still gives the theft away.
    assert_eq!(token(&app, &refresh_form(&stolen)).await.0, 400);
    assert_eq!(list_projects(&app, access).await.0, 401);
}

#[actix_web::test]
async fn two_refreshes_with_one_token_at_once_disconnect_the_assistant() {
    let db = TestDb::new().await;
    let app = app(&db.pool).await;
    enable_mcp(&app, &db.pool).await;
    let cookie = signed_in_member(&app, &db.pool).await;
    let code = param(&decide(&app, &cookie, authorization(), true).await, "code").unwrap();
    let refresh = exchange(&app, &code).await.1["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();

    // One of the two is not the client. Whichever wins, the token was
    // presented twice, so neither keeps the connection.
    let form = refresh_form(&refresh);
    let (a, b) = futures_util::join!(token(&app, &form), token(&app, &form));
    let winner = [&a, &b]
        .into_iter()
        .find(|(status, _)| *status == 200)
        .expect("one refresh succeeds");
    assert!([a.0, b.0].contains(&400));
    let access = winner.1["access_token"].as_str().unwrap();
    assert_eq!(list_projects(&app, access).await.0, 401);
}
