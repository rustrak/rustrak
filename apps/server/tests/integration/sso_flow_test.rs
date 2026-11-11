//! The whole SSO round trip over HTTP, against a small OpenID provider that
//! runs on loopback inside the test: `/auth/sso/start`, the provider's
//! authorize redirect, `/auth/sso/callback`, and the session that results.

use crate::common::TestDb;
use actix_web::cookie::Cookie;
use actix_web::dev::{Service, ServiceResponse};
use actix_web::{test, web, App, HttpRequest, HttpResponse, HttpServer};
use base64::Engine;
use openidconnect::core::{
    CoreEdDsaPrivateSigningKey, CoreGenderClaim, CoreJsonWebKeySet,
    CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm, CoreProviderMetadata,
    CoreResponseType, CoreSubjectIdentifierType, CoreTokenType,
};
use openidconnect::{
    AccessToken, Audience, AuthUrl, EmptyAdditionalProviderMetadata, EmptyExtraTokenFields,
    EndUserEmail, IdToken, IdTokenClaims, IdTokenFields, IssuerUrl, JsonWebKeyId, JsonWebKeySetUrl,
    Nonce, PrivateSigningKey, ResponseTypes, StandardClaims, StandardTokenResponse,
    SubjectIdentifier, TokenUrl,
};
use rustrak::auth::{OidcService, SsoClaims};
use rustrak::config::OidcConfig;
use rustrak::services::UsersService;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const CLIENT_ID: &str = "rustrak";
const CLIENT_SECRET: &str = "secret";
const REDIRECT_URL: &str = "http://localhost:8080/auth/sso/callback";

/// The person the provider signs in next.
#[derive(Clone)]
struct ProviderUser {
    subject: String,
    email: String,
    email_verified: bool,
    groups: Vec<String>,
}

struct PendingCode {
    nonce: String,
    code_challenge: String,
}

struct ProviderState {
    issuer: String,
    kid: String,
    signing_key: Arc<CoreEdDsaPrivateSigningKey>,
    user: ProviderUser,
    pending: HashMap<String, PendingCode>,
    /// Signs this nonce instead of the one the client sent.
    forged_nonce: Option<String>,
}

struct FakeProvider {
    issuer: String,
    state: Arc<Mutex<ProviderState>>,
}

/// An Ed25519 key in PKCS#8 PEM, built from a random seed: the DER prefix is
/// fixed for Ed25519, so no key-generation dependency is needed.
fn ed25519_key(kid: &str) -> CoreEdDsaPrivateSigningKey {
    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    der.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    der.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        base64::engine::general_purpose::STANDARD.encode(der)
    );
    CoreEdDsaPrivateSigningKey::from_ed25519_pem(&pem, Some(JsonWebKeyId::new(kid.to_string())))
        .expect("a valid Ed25519 key")
}

impl FakeProvider {
    async fn start(user: ProviderUser) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let issuer = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let state = Arc::new(Mutex::new(ProviderState {
            issuer: issuer.clone(),
            kid: "key-1".to_string(),
            signing_key: Arc::new(ed25519_key("key-1")),
            user,
            pending: HashMap::new(),
            forged_nonce: None,
        }));

        let data = web::Data::from(state.clone());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(data.clone())
                .route(
                    "/.well-known/openid-configuration",
                    web::get().to(discovery),
                )
                .route("/jwks", web::get().to(jwks))
                .route("/authorize", web::get().to(authorize))
                .route("/token", web::post().to(token))
        })
        .workers(1)
        .listen(listener)
        .unwrap()
        .run();
        actix_web::rt::spawn(server);

        FakeProvider { issuer, state }
    }

    fn set_user(&self, user: ProviderUser) {
        self.state.lock().unwrap().user = user;
    }

    fn forge_nonce(&self) {
        self.state.lock().unwrap().forged_nonce = Some("not-the-client-nonce".to_string());
    }

    /// Replace the signing key, as providers do on a schedule.
    fn rotate_key(&self) {
        let mut state = self.state.lock().unwrap();
        state.kid = format!("key-{}", uuid::Uuid::new_v4());
        state.signing_key = Arc::new(ed25519_key(&state.kid));
    }

    fn config(&self) -> OidcConfig {
        OidcConfig {
            issuer_url: self.issuer.clone(),
            client_id: CLIENT_ID.to_string(),
            client_secret: CLIENT_SECRET.to_string(),
            redirect_url: REDIRECT_URL.to_string(),
            provider_name: "Pocket ID".to_string(),
            scopes: vec!["openid".into(), "email".into(), "profile".into()],
            allowed_domains: Vec::new(),
            auto_provision: true,
            require_email_verified: true,
            link_existing_accounts: false,
            groups_claim: "groups".to_string(),
            admin_groups: Vec::new(),
            disable_password_login: false,
        }
    }

    /// The same provider, mapping one group to the global admin role.
    fn config_with_admin_group(&self, group: &str) -> OidcConfig {
        let mut config = self.config();
        config.admin_groups = vec![group.to_string()];
        config
    }
}

async fn discovery(state: web::Data<Mutex<ProviderState>>) -> HttpResponse {
    let issuer = state.lock().unwrap().issuer.clone();
    let metadata = CoreProviderMetadata::new(
        IssuerUrl::new(issuer.clone()).unwrap(),
        AuthUrl::new(format!("{issuer}/authorize")).unwrap(),
        JsonWebKeySetUrl::new(format!("{issuer}/jwks")).unwrap(),
        vec![ResponseTypes::new(vec![CoreResponseType::Code])],
        vec![CoreSubjectIdentifierType::Public],
        vec![CoreJwsSigningAlgorithm::EdDsa],
        EmptyAdditionalProviderMetadata {},
    )
    .set_token_endpoint(Some(TokenUrl::new(format!("{issuer}/token")).unwrap()));
    HttpResponse::Ok().json(metadata)
}

async fn jwks(state: web::Data<Mutex<ProviderState>>) -> HttpResponse {
    let key = state.lock().unwrap().signing_key.clone();
    HttpResponse::Ok().json(CoreJsonWebKeySet::new(vec![key.as_verification_key()]))
}

/// Signs the user straight in and redirects back with a code, the way a
/// provider does once its own login succeeds.
async fn authorize(
    state: web::Data<Mutex<ProviderState>>,
    query: web::Query<HashMap<String, String>>,
) -> HttpResponse {
    let code = uuid::Uuid::new_v4().to_string();
    state.lock().unwrap().pending.insert(
        code.clone(),
        PendingCode {
            nonce: query["nonce"].clone(),
            code_challenge: query["code_challenge"].clone(),
        },
    );
    let location = format!(
        "{}?code={code}&state={}",
        query["redirect_uri"], query["state"]
    );
    HttpResponse::Found()
        .insert_header(("Location", location))
        .finish()
}

async fn token(
    req: HttpRequest,
    state: web::Data<Mutex<ProviderState>>,
    form: web::Form<HashMap<String, String>>,
) -> HttpResponse {
    let expected_auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{CLIENT_ID}:{CLIENT_SECRET}"))
    );
    let authorized = req
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        == Some(expected_auth.as_str());
    if !authorized {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "invalid_client"}));
    }

    let mut state = state.lock().unwrap();
    let Some(pending) = state.pending.remove(&form["code"]) else {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "invalid_grant"}));
    };
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(form["code_verifier"].as_bytes()));
    if challenge != pending.code_challenge {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "invalid_grant"}));
    }

    let now = chrono::Utc::now();
    let nonce = state.forged_nonce.clone().unwrap_or(pending.nonce);
    let mut extra = SsoClaims::default();
    extra.insert("groups", serde_json::json!(state.user.groups));
    let claims = IdTokenClaims::<SsoClaims, CoreGenderClaim>::new(
        IssuerUrl::new(state.issuer.clone()).unwrap(),
        vec![Audience::new(CLIENT_ID.to_string())],
        now + chrono::Duration::minutes(5),
        now,
        StandardClaims::new(SubjectIdentifier::new(state.user.subject.clone()))
            .set_email(Some(EndUserEmail::new(state.user.email.clone())))
            .set_email_verified(Some(state.user.email_verified)),
        extra,
    )
    .set_nonce(Some(Nonce::new(nonce)));
    let id_token = IdToken::<
        SsoClaims,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm,
    >::new(
        claims,
        state.signing_key.as_ref(),
        CoreJwsSigningAlgorithm::EdDsa,
        None,
        None,
    )
    .unwrap();

    HttpResponse::Ok().json(StandardTokenResponse::<
        IdTokenFields<
            SsoClaims,
            EmptyExtraTokenFields,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
        >,
        CoreTokenType,
    >::new(
        AccessToken::new("access-token".to_string()),
        CoreTokenType::Bearer,
        IdTokenFields::new(Some(id_token), EmptyExtraTokenFields {}),
    ))
}

fn person(subject: &str, email: &str) -> ProviderUser {
    ProviderUser {
        subject: subject.to_string(),
        email: email.to_string(),
        email_verified: true,
        groups: Vec::new(),
    }
}

/// The same person, carrying a group list the provider will sign into the ID
/// token.
fn person_in_groups(subject: &str, email: &str, groups: &[&str]) -> ProviderUser {
    ProviderUser {
        groups: groups.iter().map(|group| group.to_string()).collect(),
        ..person(subject, email)
    }
}

async fn rustrak_app(
    pool: &rustrak::db::DbPool,
    oidc: Option<OidcService>,
) -> impl Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error> {
    let mut state = crate::common::state(pool, crate::common::config()).await;
    state.oidc = web::Data::new(oidc);
    crate::common::app_with(&state).await
}

fn session_cookie(resp: &ServiceResponse) -> Option<Cookie<'static>> {
    resp.response()
        .cookies()
        .find(|cookie| cookie.name() == rustrak::app::SESSION_COOKIE)
        .map(|cookie| cookie.into_owned())
}

/// What the browser holds after `/auth/sso/start`: its cookie and the query
/// the provider will send back.
struct StartedLogin {
    cookie: Cookie<'static>,
    callback_query: String,
}

async fn start_login<S>(app: &S) -> StartedLogin
where
    S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    let resp = test::call_service(
        app,
        test::TestRequest::post()
            .uri("/auth/sso/start")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let cookie = session_cookie(&resp).expect("start stores the login protections");
    let body: Value = test::read_body_json(resp).await;
    let authorization_url = body["authorization_url"].as_str().unwrap().to_string();

    // The browser follows the redirect to the provider, which answers with a
    // redirect back to our callback.
    let provider = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let redirect = provider.get(&authorization_url).send().await.unwrap();
    assert_eq!(redirect.status(), 302);
    let location = redirect.headers()["location"].to_str().unwrap();
    let callback_query = location
        .strip_prefix(&format!("{REDIRECT_URL}?"))
        .expect("the provider redirects to the configured callback")
        .to_string();

    StartedLogin {
        cookie,
        callback_query,
    }
}

/// Deliver the provider's redirect to the callback as a browser would.
async fn browser_callback<S>(app: &S, cookie: Cookie<'static>, query: &str) -> ServiceResponse
where
    S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    test::call_service(
        app,
        test::TestRequest::get()
            .uri(&format!("/auth/sso/callback?{query}"))
            .insert_header(("Accept", "text/html,application/xhtml+xml"))
            .cookie(cookie)
            .to_request(),
    )
    .await
}

fn location(resp: &ServiceResponse) -> &str {
    resp.headers()
        .get("location")
        .expect("a redirect")
        .to_str()
        .unwrap()
}

async fn current_user<S>(app: &S, cookie: Cookie<'static>) -> Option<Value>
where
    S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    let resp = test::call_service(
        app,
        test::TestRequest::get()
            .uri("/auth/me")
            .cookie(cookie)
            .to_request(),
    )
    .await;
    if resp.status() == 200 {
        Some(test::read_body_json(resp).await)
    } else {
        None
    }
}

#[actix_web::test]
async fn test_sso_config_reports_the_provider_only_when_configured() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();

    let app = rustrak_app(&db.pool, Some(service)).await;
    let body: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/auth/sso/config")
            .to_request(),
    )
    .await;
    assert_eq!(body["enabled"], true);
    assert_eq!(body["provider_name"], "Pocket ID");

    let app = rustrak_app(&db.pool, None).await;
    let body: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/auth/sso/config")
            .to_request(),
    )
    .await;
    assert_eq!(body["enabled"], false);
    let start = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/sso/start")
            .to_request(),
    )
    .await;
    assert_eq!(start.status(), 404);
}

#[actix_web::test]
async fn test_sso_round_trip_provisions_the_account_and_signs_it_in() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;

    assert_eq!(resp.status(), 302);
    assert_eq!(location(&resp), "/");
    let cookie = session_cookie(&resp).expect("the callback signs the browser in");
    let me = current_user(&app, cookie)
        .await
        .expect("the new session is authenticated");
    assert_eq!(me["email"], "owner@example.com");
    assert_eq!(
        me["is_admin"], true,
        "the first account bootstraps as admin"
    );
}

#[actix_web::test]
async fn test_sso_callback_answers_json_to_a_non_browser_client() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/auth/sso/callback?{}", login.callback_query))
            .insert_header(("Accept", "application/json"))
            .cookie(login.cookie)
            .to_request(),
    )
    .await;

    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["user"]["email"], "owner@example.com");
}

#[actix_web::test]
async fn test_sso_callback_rejects_a_state_it_did_not_issue() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    let code = login.callback_query.split('&').next().unwrap();
    let resp = browser_callback(&app, login.cookie, &format!("{code}&state=forged")).await;

    assert_eq!(resp.status(), 302);
    assert_eq!(location(&resp), "/login?error=sso");
    let cookie = session_cookie(&resp).expect("the one-time values are cleared");
    assert!(current_user(&app, cookie).await.is_none());
    assert_eq!(UsersService::user_count(&db.pool).await.unwrap(), 0);
}

#[actix_web::test]
async fn test_sso_callback_cannot_be_replayed() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    let first = browser_callback(&app, login.cookie.clone(), &login.callback_query).await;
    assert_eq!(location(&first), "/");

    // The same cookie and query again, as an attacker holding both would send.
    let replay = browser_callback(&app, login.cookie, &login.callback_query).await;
    assert_eq!(location(&replay), "/login?error=sso");
}

#[actix_web::test]
async fn test_sso_rejects_an_id_token_signed_for_another_login() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    provider.forge_nonce();
    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;

    assert_eq!(location(&resp), "/login?error=sso");
    assert_eq!(UsersService::user_count(&db.pool).await.unwrap(), 0);
}

async fn create_password_account(pool: &rustrak::db::DbPool, email: &str) -> rustrak::models::User {
    UsersService::create_user(
        pool,
        &rustrak::models::CreateUserRequest {
            email: email.to_string(),
            password: "password123".to_string(),
        },
        rustrak::models::UserRole::Admin,
    )
    .await
    .unwrap()
}

async fn confirm_link<S>(app: &S, cookie: Cookie<'static>, password: &str) -> ServiceResponse
where
    S: Service<actix_http::Request, Response = ServiceResponse, Error = actix_web::Error>,
{
    test::call_service(
        app,
        test::TestRequest::post()
            .uri("/auth/sso/link")
            .cookie(cookie)
            .set_json(serde_json::json!({ "password": password }))
            .to_request(),
    )
    .await
}

#[actix_web::test]
async fn test_sso_asks_the_owner_of_an_existing_account_for_its_password() {
    let db = TestDb::new().await;
    let existing = create_password_account(&db.pool, "owner@example.com").await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;
    assert_eq!(location(&resp), "/link-account");
    let cookie = session_cookie(&resp).expect("the pending link lives in the session");
    assert!(
        current_user(&app, cookie.clone()).await.is_none(),
        "nobody is signed in until the password is given"
    );

    let pending: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/auth/sso/link")
            .cookie(cookie.clone())
            .to_request(),
    )
    .await;
    assert_eq!(pending["email"], "owner@example.com");
    assert_eq!(pending["provider_name"], "Pocket ID");

    let wrong = confirm_link(&app, cookie.clone(), "not-the-password").await;
    assert_eq!(wrong.status(), 401);

    let right = confirm_link(&app, cookie.clone(), "password123").await;
    assert_eq!(right.status(), 200);
    let signed_in = session_cookie(&right).expect("confirming signs the browser in");
    let me = current_user(&app, signed_in).await.unwrap();
    assert_eq!(me["id"], existing.id);

    // Linked for good: the next SSO login goes straight in.
    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;
    assert_eq!(location(&resp), "/");
}

#[actix_web::test]
async fn test_sso_link_confirmation_needs_a_pending_link() {
    let db = TestDb::new().await;
    create_password_account(&db.pool, "owner@example.com").await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    // A fresh browser that never went through the provider.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/sso/link")
            .set_json(serde_json::json!({ "password": "password123" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/auth/sso/link").to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_sso_links_an_existing_account_directly_when_the_operator_allows_it() {
    let db = TestDb::new().await;
    let existing = create_password_account(&db.pool, "owner@example.com").await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let mut config = provider.config();
    config.link_existing_accounts = true;
    let app = rustrak_app(&db.pool, Some(OidcService::discover(config).await.unwrap())).await;

    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;
    let me = current_user(&app, session_cookie(&resp).unwrap())
        .await
        .expect("with linking enabled the existing account signs in");
    assert_eq!(me["id"], existing.id);
}

#[actix_web::test]
async fn test_sso_keeps_working_after_the_provider_rotates_its_keys() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    // Discovery ran at startup; the provider rotates afterwards.
    provider.rotate_key();
    provider.set_user(person("sub-1", "owner@example.com"));
    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;

    assert_eq!(
        location(&resp),
        "/",
        "an ID token signed with the new key must verify"
    );
}

#[actix_web::test]
async fn test_sso_turns_away_an_email_outside_the_allowed_domains() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "someone@elsewhere.org")).await;
    let mut config = provider.config();
    config.allowed_domains = vec!["example.com".to_string()];
    let app = rustrak_app(&db.pool, Some(OidcService::discover(config).await.unwrap())).await;

    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;
    assert_eq!(location(&resp), "/login?error=sso");

    provider.set_user(person("sub-2", "someone@Example.com"));
    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;
    assert_eq!(location(&resp), "/", "the domain comparison ignores case");
}

#[actix_web::test]
async fn test_sso_turns_away_an_unverified_email_when_verification_is_required() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(ProviderUser {
        email_verified: false,
        ..person("sub-1", "owner@example.com")
    })
    .await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;

    assert_eq!(location(&resp), "/login?error=sso");
    assert_eq!(UsersService::user_count(&db.pool).await.unwrap(), 0);
}

#[actix_web::test]
async fn test_sso_callback_handles_the_provider_refusing_the_login() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    let state = login
        .callback_query
        .split('&')
        .find(|pair| pair.starts_with("state="))
        .unwrap();
    let resp = browser_callback(&app, login.cookie, &format!("error=access_denied&{state}")).await;

    assert_eq!(location(&resp), "/login?error=sso");
}

#[actix_web::test]
async fn test_sso_does_not_sign_in_a_disabled_account() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    browser_callback(&app, login.cookie, &login.callback_query).await;
    sqlx::query("UPDATE users SET is_active = FALSE")
        .execute(&db.pool)
        .await
        .unwrap();

    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;
    assert_eq!(location(&resp), "/login?error=sso");
}

#[actix_web::test]
async fn test_sso_callback_tells_a_non_browser_client_to_confirm_the_link() {
    let db = TestDb::new().await;
    create_password_account(&db.pool, "owner@example.com").await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/auth/sso/callback?{}", login.callback_query))
            .insert_header(("Accept", "application/json"))
            .cookie(login.cookie)
            .to_request(),
    )
    .await;

    assert_eq!(resp.status(), 409);
    let cookie = session_cookie(&resp).expect("the pending link is kept for the client");
    let resp = confirm_link(&app, cookie, "password123").await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_sso_groups_decide_the_role_of_the_account() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person_in_groups(
        "sub-1",
        "owner@example.com",
        &["rustrak-admins"],
    ))
    .await;
    let service = OidcService::discover(provider.config_with_admin_group("rustrak-admins"))
        .await
        .unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;
    assert_eq!(resp.status(), 302);
    let cookie = session_cookie(&resp).expect("the callback signs the browser in");
    let me = current_user(&app, cookie)
        .await
        .expect("the new session is authenticated");
    assert_eq!(me["role"], "admin", "a member of the admin group is admin");

    provider.set_user(person_in_groups("sub-2", "second@example.com", &["users"]));
    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;
    let cookie = session_cookie(&resp).expect("the callback signs the browser in");
    let me = current_user(&app, cookie)
        .await
        .expect("the new session is authenticated");
    assert_eq!(me["role"], "member", "everyone else is a member");

    provider.set_user(person_in_groups(
        "sub-2",
        "second@example.com",
        &["users", "rustrak-admins"],
    ));
    let login = start_login(&app).await;
    let resp = browser_callback(&app, login.cookie, &login.callback_query).await;
    let cookie = session_cookie(&resp).expect("the callback signs the browser in");
    let me = current_user(&app, cookie)
        .await
        .expect("the session is authenticated");
    assert_eq!(
        me["role"], "admin",
        "joining the group promotes on the next login"
    );
}

#[actix_web::test]
async fn test_password_login_can_be_disabled_without_blocking_the_superuser() {
    let db = TestDb::new().await;
    let primary = create_password_account(&db.pool, "owner@example.com").await;
    let member = UsersService::create_user(
        &db.pool,
        &rustrak::models::CreateUserRequest {
            email: "member@example.com".to_string(),
            password: "password123".to_string(),
        },
        rustrak::models::UserRole::Member,
    )
    .await
    .unwrap();

    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let mut config = provider.config();
    config.disable_password_login = true;
    let service = OidcService::discover(config).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/auth/sso/config")
            .to_request(),
    )
    .await;
    assert_eq!(body["enabled"], true);
    assert_eq!(body["password_login_enabled"], false);

    let denied = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(serde_json::json!({
                "email": member.email,
                "password": "password123",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(
        denied.status(),
        403,
        "a non-primary account must not sign in with a password"
    );

    // The documented break-glass: the account CREATE_SUPERUSER creates.
    let allowed = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(serde_json::json!({
                "email": primary.email,
                "password": "password123",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(
        allowed.status(),
        200,
        "the primary account stays as break-glass"
    );
}

/// Without auto-provisioning an invitation is the only way to create an
/// account, so SSO-only must still let it be accepted. What it must not do is
/// sign the browser in on a password: the account comes into being and the
/// person enters through the provider like everyone else.
#[actix_web::test]
async fn test_accepting_an_invitation_under_sso_only_does_not_sign_in() {
    let db = TestDb::new().await;
    create_password_account(&db.pool, "owner@example.com").await;
    sqlx::query(
        "INSERT INTO invitations (token, email, role, status, expires_at) VALUES ($1, $2, 'member', 'pending', $3)",
    )
    .bind("sso-only-invite")
    .bind("invitee@example.com")
    .bind(chrono::Utc::now() + chrono::Duration::days(1))
    .execute(&db.pool)
    .await
    .unwrap();

    let provider = FakeProvider::start(person("sub-1", "invitee@example.com")).await;
    let mut config = provider.config();
    config.disable_password_login = true;
    let service = OidcService::discover(config).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/accept-invitation")
            .set_json(serde_json::json!({
                "token": "sso-only-invite",
                "password": "password123",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let cookie = session_cookie(&resp);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["user"]["email"], "invitee@example.com");
    assert_eq!(body["signed_in"], false);

    let mut me = test::TestRequest::get().uri("/auth/me");
    if let Some(cookie) = cookie {
        me = me.cookie(cookie);
    }
    let me = test::call_service(&app, me.to_request()).await;
    assert_eq!(me.status(), 401, "no session comes out of a password");
}

/// A browser already signed in as someone else must not stay signed in as
/// them after creating a different account it cannot enter with a password:
/// the page says "now sign in through SSO" while the cookie still says A.
#[actix_web::test]
async fn test_accepting_an_invitation_under_sso_only_ends_an_existing_session() {
    let db = TestDb::new().await;
    let primary = create_password_account(&db.pool, "owner@example.com").await;
    sqlx::query(
        "INSERT INTO invitations (token, email, role, status, expires_at) VALUES ($1, $2, 'member', 'pending', $3)",
    )
    .bind("sso-only-invite")
    .bind("invitee@example.com")
    .bind(chrono::Utc::now() + chrono::Duration::days(1))
    .execute(&db.pool)
    .await
    .unwrap();

    let provider = FakeProvider::start(person("sub-1", "invitee@example.com")).await;
    let mut config = provider.config();
    config.disable_password_login = true;
    let service = OidcService::discover(config).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    // The primary account is the one that may still use its password.
    let login = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/login")
            .set_json(serde_json::json!({
                "email": primary.email,
                "password": "password123",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(login.status(), 200);
    let signed_in_as_primary = session_cookie(&login).expect("a session cookie");

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/accept-invitation")
            .cookie(signed_in_as_primary.clone())
            .set_json(serde_json::json!({
                "token": "sso-only-invite",
                "password": "password123",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let cookie = session_cookie(&resp).unwrap_or(signed_in_as_primary);

    let me = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/auth/me")
            .cookie(cookie)
            .to_request(),
    )
    .await;
    assert_eq!(me.status(), 401, "the previous session must end");
}

#[actix_web::test]
async fn test_accepting_an_invitation_signs_in_when_passwords_are_allowed() {
    let db = TestDb::new().await;
    create_password_account(&db.pool, "owner@example.com").await;
    sqlx::query(
        "INSERT INTO invitations (token, email, role, status, expires_at) VALUES ($1, $2, 'member', 'pending', $3)",
    )
    .bind("plain-invite")
    .bind("invitee@example.com")
    .bind(chrono::Utc::now() + chrono::Duration::days(1))
    .execute(&db.pool)
    .await
    .unwrap();

    let provider = FakeProvider::start(person("sub-1", "invitee@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/accept-invitation")
            .set_json(serde_json::json!({
                "token": "plain-invite",
                "password": "password123",
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let cookie = session_cookie(&resp).expect("a session cookie");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["signed_in"], true);

    let me = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/auth/me")
            .cookie(cookie)
            .to_request(),
    )
    .await;
    assert_eq!(me.status(), 200);
}

#[actix_web::test]
async fn test_sso_config_says_password_login_is_available_by_default() {
    let db = TestDb::new().await;
    let provider = FakeProvider::start(person("sub-1", "owner@example.com")).await;
    let service = OidcService::discover(provider.config()).await.unwrap();
    let app = rustrak_app(&db.pool, Some(service)).await;

    let body: Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/auth/sso/config")
            .to_request(),
    )
    .await;
    assert_eq!(body["password_login_enabled"], true);
}
