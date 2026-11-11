use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreErrorResponseType, CoreGenderClaim, CoreJsonWebKey,
    CoreJsonWebKeySet, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
    CoreProviderMetadata, CoreResponseType, CoreRevocableToken, CoreRevocationErrorResponse,
    CoreTokenIntrospectionResponse, CoreTokenType,
};
use openidconnect::{
    AdditionalClaims, AsyncHttpClient, AuthenticationFlow, AuthorizationCode, Client, ClientId,
    ClientSecret, CsrfToken, EmptyExtraTokenFields, EndpointMaybeSet, EndpointNotSet, EndpointSet,
    HttpRequest, HttpResponse, IdTokenFields, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, StandardErrorResponse, StandardTokenResponse,
};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

use crate::config::OidcConfig;
use crate::error::{AppError, AppResult};
use crate::models::user::OidcIdentity;
use crate::services::OidcLinkPolicy;

/// The claims this server keeps beyond the registered set.
///
/// It is a map rather than a fixed field because the claim that carries groups
/// is operator-configurable (`OIDC_GROUPS_CLAIM`): providers call it `groups`,
/// `roles`, `memberOf` and more. Only the names the operator asks about are
/// read; everything else stays untyped, as the provider sent it.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SsoClaims {
    #[serde(flatten)]
    claims: serde_json::Map<String, serde_json::Value>,
}

impl AdditionalClaims for SsoClaims {}

impl SsoClaims {
    /// Values of a claim that is expected to be a group list.
    ///
    /// A JSON array is taken entry by entry: a group called `staff,admins` is
    /// one group, and no entry is ever split. A single string is the one form
    /// that has to be separated, because that is what a comma-separated claim
    /// looks like on the wire. Anything else is no groups.
    pub fn groups(&self, claim: &str) -> Vec<String> {
        match self.claims.get(claim) {
            Some(serde_json::Value::Array(values)) => values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|group| !group.is_empty())
                .map(str::to_string)
                .collect(),
            Some(serde_json::Value::String(value)) => value
                .split(',')
                .map(str::trim)
                .filter(|group| !group.is_empty())
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Insert a claim. Used by the tests' fake provider to sign a group list.
    pub fn insert(&mut self, name: &str, value: serde_json::Value) {
        self.claims.insert(name.to_string(), value);
    }
}

/// Token response whose ID token keeps the provider's extra claims, so the
/// configured groups claim survives validation.
type SsoTokenResponse = StandardTokenResponse<
    IdTokenFields<
        SsoClaims,
        EmptyExtraTokenFields,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm,
    >,
    CoreTokenType,
>;

/// The core client with extra claims: identical to [`CoreClient`] except for
/// the claim type, which is what makes `groups` readable after verification.
type SsoClient = Client<
    SsoClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    SsoTokenResponse,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    // The state `from_provider_metadata` returns: discovery found the
    // authorization endpoint, and the token endpoint may or may not be there.
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// Values that must be retained in the encrypted browser session while the
/// user authenticates with the identity provider.
pub struct OidcAuthorization {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

/// OIDC provider metadata and the client used for authorization and token exchange.
#[derive(Clone)]
pub struct OidcService {
    metadata: CoreProviderMetadata,
    http_client: HttpsClient,
    config: OidcConfig,
}

/// HTTP client that rejects cleartext requests (loopback aside) before any
/// OIDC data is sent. It wraps the server's own `reqwest` rather than the one
/// `openidconnect` would bring, so the binary carries one HTTP stack.
#[derive(Clone)]
struct HttpsClient(reqwest::Client);

#[derive(Debug, thiserror::Error)]
enum OidcHttpError {
    #[error("OIDC endpoint must use HTTPS: {0}")]
    InsecureEndpoint(String),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

impl<'c> AsyncHttpClient<'c> for HttpsClient {
    type Error = OidcHttpError;
    type Future =
        Pin<Box<dyn Future<Output = Result<HttpResponse, Self::Error>> + Send + Sync + 'c>>;

    /// Execute an OIDC request only when its destination uses HTTPS.
    fn call(&'c self, request: HttpRequest) -> Self::Future {
        Box::pin(async move {
            let uri = request.uri().to_string();
            let secure =
                openidconnect::url::Url::parse(&uri).is_ok_and(|url| is_secure_transport(&url));
            if !secure {
                return Err(OidcHttpError::InsecureEndpoint(uri));
            }

            let response = self.0.execute(reqwest::Request::try_from(request)?).await?;
            let status = response.status();
            let headers = response.headers().clone();
            let mut converted = HttpResponse::new(response.bytes().await?.to_vec());
            *converted.status_mut() = status;
            *converted.headers_mut() = headers;
            Ok(converted)
        })
    }
}

/// HTTPS, or plain HTTP to this machine, where there is no network to
/// eavesdrop on. The exception is what lets a provider run next to the server
/// in development.
fn is_secure_transport(url: &openidconnect::url::Url) -> bool {
    use openidconnect::url::Host;

    match url.scheme() {
        "https" => true,
        "http" => match url.host() {
            Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
            Some(Host::Ipv4(ip)) => ip.is_loopback(),
            Some(Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        },
        _ => false,
    }
}

/// Reject an endpoint advertised by the provider unless it uses HTTPS.
fn require_https(url: &openidconnect::url::Url, endpoint: &str) -> AppResult<()> {
    if is_secure_transport(url) {
        Ok(())
    } else {
        Err(AppError::Internal(format!(
            "OIDC {endpoint} must use HTTPS"
        )))
    }
}

impl OidcService {
    /// Discover the provider once during startup. Startup fails when SSO is
    /// explicitly configured but its discovery document is unavailable or
    /// invalid, avoiding a login button that can never work.
    pub async fn discover(config: OidcConfig) -> AppResult<Self> {
        let issuer = IssuerUrl::new(config.issuer_url.clone())
            .map_err(|e| AppError::Internal(format!("Invalid OIDC issuer URL: {e}")))?;
        require_https(issuer.url(), "issuer URL")?;
        let http_client = HttpsClient(
            reqwest::ClientBuilder::new()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| {
                    AppError::Internal(format!("Failed to create OIDC HTTP client: {e}"))
                })?,
        );
        let metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
            .await
            .map_err(|e| AppError::Internal(format!("OIDC discovery failed: {e}")))?;
        require_https(
            metadata.authorization_endpoint().url(),
            "authorization endpoint",
        )?;
        if let Some(token_endpoint) = metadata.token_endpoint() {
            require_https(token_endpoint.url(), "token endpoint")?;
        }

        Ok(Self {
            metadata,
            http_client,
            config,
        })
    }

    /// Return the administrator-defined provider label shown on the login page.
    pub fn provider_name(&self) -> &str {
        &self.config.provider_name
    }

    /// What a login may do with an identity that is not linked yet.
    pub fn link_policy(&self) -> OidcLinkPolicy {
        OidcLinkPolicy {
            auto_provision: self.config.auto_provision,
            link_existing_accounts: self.config.link_existing_accounts,
            admin_groups: self.config.admin_groups.clone(),
        }
    }

    /// Whether the operator turned password sign-in off.
    pub fn password_login_disabled(&self) -> bool {
        self.config.disable_password_login
    }

    /// Build an authorization URL and fresh state, nonce, and PKCE protections.
    pub fn authorization_url(&self) -> AppResult<OidcAuthorization> {
        let client = SsoClient::from_provider_metadata(
            self.metadata.clone(),
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
        )
        .set_redirect_uri(
            RedirectUrl::new(self.config.redirect_url.clone())
                .map_err(|e| AppError::Internal(format!("Invalid OIDC redirect URL: {e}")))?,
        );

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client
            .authorize_url(
                AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(pkce_challenge);
        for scope in &self.config.scopes {
            if scope != "openid" {
                request = request.add_scope(Scope::new(scope.clone()));
            }
        }
        let (url, state, nonce) = request.url();

        Ok(OidcAuthorization {
            url: url.to_string(),
            state: state.secret().to_string(),
            nonce: nonce.secret().to_string(),
            pkce_verifier: pkce_verifier.secret().to_string(),
        })
    }

    /// Exchange an authorization code and return identity claims after validation.
    pub async fn exchange_code(
        &self,
        code: String,
        pkce_verifier: String,
        nonce: String,
    ) -> AppResult<OidcIdentity> {
        // Keys are fetched per login, not kept from discovery: providers rotate
        // them, and a set cached at startup would reject every token signed
        // after the rotation until a restart.
        let jwks = CoreJsonWebKeySet::fetch_async(self.metadata.jwks_uri(), &self.http_client)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to fetch OIDC signing keys: {e}")))?;
        let client = SsoClient::from_provider_metadata(
            self.metadata.clone().set_jwks(jwks),
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
        )
        .set_redirect_uri(
            RedirectUrl::new(self.config.redirect_url.clone())
                .map_err(|e| AppError::Internal(format!("Invalid OIDC redirect URL: {e}")))?,
        );

        let token = client
            .exchange_code(AuthorizationCode::new(code))
            .map_err(|_| AppError::Internal("OIDC provider has no token endpoint".to_string()))?
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
            .request_async(&self.http_client)
            .await
            .map_err(|e| {
                log::warn!("OIDC code exchange failed: {e}");
                AppError::Unauthorized("SSO authentication failed".to_string())
            })?;
        let id_token = token.extra_fields().id_token().ok_or_else(|| {
            AppError::Unauthorized("OIDC provider did not return an ID token".to_string())
        })?;
        let claims = id_token
            .claims(&client.id_token_verifier(), &Nonce::new(nonce))
            .map_err(|e| {
                log::warn!("OIDC ID token validation failed: {e}");
                AppError::Unauthorized("SSO identity token is invalid".to_string())
            })?;

        let email = claims
            .email()
            .map(|value| value.as_str().trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AppError::Forbidden("SSO provider did not supply an email address".to_string())
            })?;

        let email_verified = claims.email_verified() == Some(true);

        if self.config.require_email_verified && !email_verified {
            return Err(AppError::Forbidden(
                "SSO provider did not verify the email address".to_string(),
            ));
        }

        if !self.config.allowed_domains.is_empty() {
            let domain = email
                .rsplit_once('@')
                .map(|(_, domain)| domain.to_ascii_lowercase());
            if !domain.is_some_and(|domain| {
                self.config
                    .allowed_domains
                    .iter()
                    .any(|allowed| allowed == &domain)
            }) {
                return Err(AppError::Forbidden(
                    "Email domain is not allowed to use SSO".to_string(),
                ));
            }
        }

        Ok(OidcIdentity {
            issuer: claims.issuer().as_str().to_string(),
            subject: claims.subject().as_str().to_string(),
            email,
            email_verified,
            // From the validated ID token only: the groups decide a role, so
            // they must come from the same signed, audience-checked document
            // as the subject.
            groups: claims.additional_claims().groups(&self.config.groups_claim),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{require_https, HttpsClient, OidcHttpError, SsoClaims};
    use openidconnect::{http, url::Url, AsyncHttpClient};

    /// A group name that contains a comma is one group. Splitting entries would
    /// invent memberships the provider never asserted.
    #[test]
    fn groups_from_an_array_are_never_split() {
        let mut claims = SsoClaims::default();
        claims.insert("groups", serde_json::json!(["staff,admins", " ops "]));

        assert_eq!(claims.groups("groups"), ["staff,admins", "ops"]);
    }

    /// Some providers send one comma-separated string instead of an array, and
    /// that form does have to be split.
    #[test]
    fn groups_from_a_string_are_split_on_commas() {
        let mut claims = SsoClaims::default();
        claims.insert("groups", serde_json::json!("admins, ops ,,"));

        assert_eq!(claims.groups("groups"), ["admins", "ops"]);
    }

    #[test]
    fn groups_ignore_claims_that_are_not_group_lists() {
        let mut claims = SsoClaims::default();
        claims.insert("groups", serde_json::json!({"admins": true}));

        assert!(claims.groups("groups").is_empty());
        assert!(claims.groups("missing").is_empty());
    }

    #[test]
    fn oidc_endpoints_require_https() {
        let secure = Url::parse("https://id.example.com/authorize").unwrap();
        let insecure = Url::parse("http://id.example.com/authorize").unwrap();

        assert!(require_https(&secure, "authorization endpoint").is_ok());
        assert!(require_https(&insecure, "authorization endpoint").is_err());
    }

    #[test]
    fn oidc_endpoints_on_loopback_may_use_plain_http() {
        for local in [
            "http://localhost:1411/authorize",
            "http://127.0.0.1:1411/authorize",
            "http://[::1]:1411/authorize",
        ] {
            let url = Url::parse(local).unwrap();
            assert!(require_https(&url, "issuer URL").is_ok(), "{local}");
        }
        for remote in [
            "http://localhost.example.com/authorize",
            "http://10.0.0.5/authorize",
        ] {
            let url = Url::parse(remote).unwrap();
            assert!(require_https(&url, "issuer URL").is_err(), "{remote}");
        }
    }

    #[actix_rt::test]
    async fn oidc_http_client_allows_cleartext_to_loopback() {
        let client = HttpsClient(reqwest::Client::new());
        // Nothing listens on port 9; the point is that the scheme check lets
        // the request through to the network layer.
        let request = http::Request::builder()
            .uri("http://127.0.0.1:9/.well-known/openid-configuration")
            .body(Vec::new())
            .unwrap();

        let error = AsyncHttpClient::call(&client, request).await.unwrap_err();
        assert!(matches!(error, OidcHttpError::Request(_)));
    }

    #[actix_rt::test]
    async fn oidc_http_client_rejects_cleartext_requests() {
        let client = HttpsClient(reqwest::Client::new());
        let request = http::Request::builder()
            .uri("http://id.example.com/.well-known/openid-configuration")
            .body(Vec::new())
            .unwrap();

        let error = AsyncHttpClient::call(&client, request).await.unwrap_err();
        assert!(matches!(error, OidcHttpError::InsecureEndpoint(_)));
    }
}
