//! OAuth 2.1 for `/mcp`, with Rustrak as the authorization server of its own
//! MCP endpoint (see `routes::oauth`).
//!
//! - Clients identify themselves with a Client ID Metadata Document: the
//!   `client_id` is an HTTPS URL serving their name and redirect URIs. Nothing
//!   is registered ahead of time.
//! - Codes and tokens are random, opaque, and stored only as SHA-256 hashes.
//!   An access token lasts an hour; refreshing replaces both tokens, so a
//!   refresh token works once.
//! - A token is bound to the resource it was issued for and is accepted only
//!   at `/mcp`, never at `/api`.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use sqlx::FromRow;

use crate::db::DbPool;
use crate::error::{AppError, AppResult};

pub const ACCESS_TTL_SECS: i64 = 3600;
const REFRESH_TTL_DAYS: i64 = 30;
const CODE_TTL_SECS: i64 = 300;
/// Prefix of an access token, which also keeps it apart from a 40-hex API token.
pub const ACCESS_PREFIX: &str = "rto_";
const REFRESH_PREFIX: &str = "rtr_";

/// The fields of a client's metadata document that the flow reads.
#[derive(Debug, Clone, Deserialize)]
pub struct ClientMetadata {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub token_endpoint_auth_method: Option<String>,
}

/// Where a client's metadata document comes from. Production fetches it over
/// HTTPS; tests serve one directly.
#[async_trait::async_trait]
pub trait ClientMetadataSource: Send + Sync {
    async fn fetch(&self, client_id: &str) -> AppResult<ClientMetadata>;
}

/// Checks a client and the redirect it asked for against its document.
pub async fn verify_client(
    source: &dyn ClientMetadataSource,
    client_id: &str,
    redirect_uri: &str,
) -> AppResult<ClientMetadata> {
    // The columns these are stored in hold 2048 and 255 characters; checked
    // here, so a client outgrowing them is refused before consent rather
    // than failing the insert after it.
    if client_id.chars().count() > 2048 || redirect_uri.chars().count() > 2048 {
        return Err(AppError::Validation(
            "client_id and redirect_uri must be at most 2048 characters".into(),
        ));
    }
    let client = source.fetch(client_id).await?;
    let name = client.client_name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(AppError::Validation(
            "The client's name must be 1 to 255 characters".into(),
        ));
    }
    if client.client_id != client_id {
        return Err(AppError::Validation(
            "The client's metadata document names a different client_id".into(),
        ));
    }
    // Public clients only: there is no secret a metadata document could hold.
    if client
        .token_endpoint_auth_method
        .as_deref()
        .is_some_and(|m| m != "none")
    {
        return Err(AppError::Validation(
            "Only public clients (token_endpoint_auth_method \"none\") are supported".into(),
        ));
    }
    if !redirect_allowed(redirect_uri) {
        return Err(AppError::Validation(
            "redirect_uri must be https, http on loopback, or an app's own scheme".into(),
        ));
    }
    if !client
        .redirect_uris
        .iter()
        .any(|uri| redirect_matches(uri, redirect_uri))
    {
        return Err(AppError::Validation(
            "redirect_uri is not one of the client's registered redirect URIs".into(),
        ));
    }
    Ok(client)
}

/// Whether the browser may be sent to `redirect_uri` after consent. The
/// client's own document registers it, so it must not be able to name
/// something the browser runs in the dashboard's origin (`javascript:`) or
/// reads locally: https, plain http only to a loopback address (RFC 8252 7.3),
/// or a native app's own scheme such as `cursor://` (RFC 8252 7.1).
pub fn redirect_allowed(redirect_uri: &str) -> bool {
    let Ok(url) = url::Url::parse(redirect_uri) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
        "javascript" | "data" | "vbscript" | "file" | "blob" | "about" | "filesystem"
        | "view-source" => false,
        _ => !url.cannot_be_a_base(),
    }
}

/// Whether `requested` is the redirect `registered` stands for: the same
/// string, except that a loopback redirect takes any port. A native client
/// (Claude Code) listens on whatever port is free, so it registers
/// `http://localhost/callback` and asks for `http://localhost:49581/callback`
/// (RFC 8252 7.3). Scheme, host, path and query must still match exactly.
pub fn redirect_matches(registered: &str, requested: &str) -> bool {
    if registered == requested {
        return true;
    }
    let (Ok(registered), Ok(mut requested)) =
        (url::Url::parse(registered), url::Url::parse(requested))
    else {
        return false;
    };
    let loopback = registered.scheme() == "http"
        && registered.port().is_none()
        && matches!(
            registered.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        );
    loopback && requested.set_port(None).is_ok() && requested == registered
}

/// Fetches metadata documents from the public internet, and nowhere else.
pub struct HttpClientMetadata;

const MAX_DOCUMENT_BYTES: usize = 64 * 1024;

#[async_trait::async_trait]
impl ClientMetadataSource for HttpClientMetadata {
    async fn fetch(&self, client_id: &str) -> AppResult<ClientMetadata> {
        // One budget for the whole lookup: a DNS server that never answers
        // must not hold the consent page past it.
        tokio::time::timeout(Duration::from_secs(5), fetch_metadata(client_id))
            .await
            .map_err(|_| {
                AppError::Validation("client_id: its metadata document timed out".into())
            })?
    }
}

async fn fetch_metadata(client_id: &str) -> AppResult<ClientMetadata> {
    let invalid = |why: &str| AppError::Validation(format!("client_id: {why}"));
    let url = url::Url::parse(client_id).map_err(|_| invalid("not a URL"))?;
    if url.scheme() != "https" || url.path() == "/" || url.fragment().is_some() {
        return Err(invalid("must be an https URL with a path"));
    }
    let host = url.host_str().ok_or_else(|| invalid("has no host"))?;
    let port = url.port_or_known_default().unwrap_or(443);

    // The URL comes from whoever starts an authorization, so it must not
    // reach anything a public request could not. The address checked is
    // the one connected to: resolving again at connect time would let a
    // rebinding DNS answer differently the second time.
    let addr: SocketAddr = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| invalid("its host does not resolve"))?
        .next()
        .ok_or_else(|| invalid("its host does not resolve"))?;
    if !is_public(addr.ip()) {
        return Err(invalid("its host is not on the public internet"));
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(host, addr)
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client: {e}")))?;
    let mut response = client
        .get(url.clone())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| invalid("its metadata document could not be fetched"))?;
    if !response.status().is_success() {
        return Err(invalid("its metadata document could not be fetched"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| invalid("its metadata document could not be read"))?
    {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid("its metadata document is too large"));
        }
    }
    serde_json::from_slice(&body)
        .map_err(|_| invalid("its metadata document is not valid client metadata"))
}

/// Whether an address is reachable from the public internet: not loopback,
/// private, link-local (cloud metadata lives there), shared, or reserved.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || a == 0
                || (a == 100 && (64..128).contains(&b))
                || (a == 192 && b == 0)
                || (a == 198 && (18..=19).contains(&b))
                || a >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let s = v6.segments();
            let embedded = |hi: u16, lo: u16| {
                let [a, b] = hi.to_be_bytes();
                let [c, d] = lo.to_be_bytes();
                IpAddr::V4(std::net::Ipv4Addr::new(a, b, c, d))
            };
            // Addresses that carry an IPv4 one are as public as it is: NAT64
            // (64:ff9b::/96, which DNS64 also hands out for public hosts on an
            // IPv6-only network), IPv4-compatible (::a.b.c.d) and 6to4.
            if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                return is_public(embedded(s[6], s[7]));
            }
            if s[..6] == [0; 6] && !v6.is_loopback() && !v6.is_unspecified() {
                return is_public(embedded(s[6], s[7]));
            }
            if s[0] == 0x2002 {
                return is_public(embedded(s[1], s[2]));
            }
            // Teredo and local-use NAT64 hide or vary where they lead.
            if (s[0] == 0x2001 && s[1] == 0) || (s[0] == 0x64 && s[1] == 0xff9b && s[2] == 1) {
                return false;
            }
            let first = s[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || first == 0x2001 && v6.segments()[1] == 0x0db8)
        }
    }
}

/// Whether `verifier` is the one `challenge` was made from (PKCE, S256).
pub fn pkce_matches(verifier: &str, challenge: &str) -> bool {
    let valid = (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b));
    valid
        && base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(sha2::Sha256::digest(verifier.as_bytes()))
            == challenge
}

fn hash(secret: &str) -> String {
    hex::encode(sha2::Sha256::digest(secret.as_bytes()))
}

fn secret(prefix: &str) -> String {
    use rand::RngExt;
    let bytes: [u8; 32] = rand::rng().random();
    format!("{prefix}{}", hex::encode(bytes))
}

/// What a code stands for, read back when it is redeemed.
#[derive(Debug, FromRow)]
pub struct AuthorizationCode {
    pub user_id: i32,
    pub client_id: String,
    pub client_name: String,
    pub redirect_uri: String,
    pub resource: String,
    pub code_challenge: String,
    pub expires_at: DateTime<Utc>,
}

/// A fresh access and refresh token, as the token endpoint returns them.
#[derive(Debug, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub refresh_token: String,
}

/// A connected client, as its user sees it.
#[derive(Debug, Serialize, FromRow)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OAuthGrant {
    pub id: i32,
    pub client_id: String,
    pub client_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct GrantRow {
    id: i32,
    user_id: i32,
    client_id: String,
    resource: String,
    access_expires_at: DateTime<Utc>,
    refresh_expires_at: DateTime<Utc>,
}

pub struct OAuthService;

impl OAuthService {
    /// Issues a code for an approved authorization, valid for five minutes.
    pub async fn create_code(
        pool: &DbPool,
        user_id: i32,
        client: &ClientMetadata,
        redirect_uri: &str,
        resource: &str,
        code_challenge: &str,
    ) -> AppResult<String> {
        let code = secret("");
        // Abandoned authorizations leave codes nobody redeems; the next one
        // sweeps them, so the table never outgrows a few minutes of traffic.
        sqlx::query("DELETE FROM oauth_codes WHERE expires_at < $1")
            .bind(Utc::now())
            .execute(pool)
            .await?;
        sqlx::query(
            "INSERT INTO oauth_codes (code_hash, user_id, client_id, client_name, redirect_uri, \
             resource, code_challenge, expires_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(hash(&code))
        .bind(user_id)
        .bind(&client.client_id)
        .bind(&client.client_name)
        .bind(redirect_uri)
        .bind(resource)
        .bind(code_challenge)
        .bind(Utc::now() + chrono::Duration::seconds(CODE_TTL_SECS))
        .execute(pool)
        .await?;
        Ok(code)
    }

    /// Takes a code out of circulation and returns what it was issued for,
    /// whether or not it is still valid: a code is spent by being presented.
    pub async fn redeem_code(pool: &DbPool, code: &str) -> AppResult<Option<AuthorizationCode>> {
        Ok(sqlx::query_as::<_, AuthorizationCode>(
            "DELETE FROM oauth_codes WHERE code_hash = $1 RETURNING user_id, client_id, \
             client_name, redirect_uri, resource, code_challenge, expires_at",
        )
        .bind(hash(code))
        .fetch_optional(pool)
        .await?)
    }

    /// Connects a client: a new grant with its first pair of tokens.
    pub async fn create_grant(pool: &DbPool, code: &AuthorizationCode) -> AppResult<TokenPair> {
        // Connections whose refresh token ran out can never act again.
        sqlx::query("DELETE FROM oauth_grants WHERE refresh_expires_at < $1")
            .bind(Utc::now())
            .execute(pool)
            .await?;
        let (pair, access_expires, refresh_expires) = Self::new_pair();
        sqlx::query(
            "INSERT INTO oauth_grants (user_id, client_id, client_name, resource, access_hash, \
             access_expires_at, refresh_hash, refresh_expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(code.user_id)
        .bind(&code.client_id)
        .bind(&code.client_name)
        .bind(&code.resource)
        .bind(hash(&pair.access_token))
        .bind(access_expires)
        .bind(hash(&pair.refresh_token))
        .bind(refresh_expires)
        .execute(pool)
        .await?;
        Ok(pair)
    }

    /// Replaces a grant's tokens. `None` when the refresh token is unknown,
    /// already used, expired, or held by another client.
    ///
    /// A refresh token presented again after it was rotated, however many
    /// rotations ago, means two parties hold it, and the server cannot tell
    /// the thief from the client, so the grant is revoked for both
    /// (RFC 9700 4.14.2). The user approves again.
    pub async fn refresh(
        pool: &DbPool,
        refresh_token: &str,
        client_id: &str,
    ) -> AppResult<Option<TokenPair>> {
        let old = hash(refresh_token);
        // Read, rotate and record the spent token in one write transaction,
        // so there is no moment where it is neither current nor recorded.
        let mut tx = crate::db::begin_write(pool).await?;
        let Some(grant) = sqlx::query_as::<_, GrantRow>(
            "SELECT id, user_id, client_id, resource, access_expires_at, refresh_expires_at \
             FROM oauth_grants WHERE refresh_hash = $1",
        )
        .bind(&old)
        .fetch_optional(&mut *tx)
        .await?
        else {
            sqlx::query(
                "DELETE FROM oauth_grants WHERE id = \
                 (SELECT grant_id FROM oauth_spent_refresh_tokens WHERE refresh_hash = $1)",
            )
            .bind(&old)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(None);
        };
        if grant.client_id != client_id || grant.refresh_expires_at <= Utc::now() {
            return Ok(None);
        }
        let (pair, access_expires, refresh_expires) = Self::new_pair();
        let rotated = sqlx::query(
            "UPDATE oauth_grants SET access_hash = $1, access_expires_at = $2, \
             refresh_hash = $3, refresh_expires_at = $4 WHERE id = $5 AND refresh_hash = $6",
        )
        .bind(hash(&pair.access_token))
        .bind(access_expires)
        .bind(hash(&pair.refresh_token))
        .bind(refresh_expires)
        .bind(grant.id)
        .bind(&old)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if rotated != 1 {
            // Another request rotated this same token first: it was presented
            // twice, which is reuse like any other.
            sqlx::query("DELETE FROM oauth_grants WHERE id = $1")
                .bind(grant.id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(None);
        }
        // Kept for the grant's lifetime (deleted with it), so that any spent
        // token coming back is recognised.
        sqlx::query(
            "INSERT INTO oauth_spent_refresh_tokens (refresh_hash, grant_id) VALUES ($1, $2)",
        )
        .bind(&old)
        .bind(grant.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(pair))
    }

    /// The user an access token acts for, if it is current and was issued for
    /// `resource`.
    pub async fn authenticate(
        pool: &DbPool,
        access_token: &str,
        resource: &str,
    ) -> AppResult<Option<i32>> {
        let grant = sqlx::query_as::<_, GrantRow>(
            "SELECT id, user_id, client_id, resource, access_expires_at, refresh_expires_at \
             FROM oauth_grants WHERE access_hash = $1",
        )
        .bind(hash(access_token))
        .fetch_optional(pool)
        .await?;
        Ok(grant
            .filter(|g| g.access_expires_at > Utc::now() && same_resource(&g.resource, resource))
            .map(|g| g.user_id))
    }

    pub async fn list_grants(pool: &DbPool, user_id: i32) -> AppResult<Vec<OAuthGrant>> {
        Ok(sqlx::query_as::<_, OAuthGrant>(
            "SELECT id, client_id, client_name, created_at FROM oauth_grants \
             WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?)
    }

    /// Disconnects a client. Only its own user can.
    pub async fn revoke(pool: &DbPool, user_id: i32, grant_id: i32) -> AppResult<()> {
        let deleted = sqlx::query("DELETE FROM oauth_grants WHERE id = $1 AND user_id = $2")
            .bind(grant_id)
            .bind(user_id)
            .execute(pool)
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(AppError::NotFound("Connection not found".into()));
        }
        Ok(())
    }

    fn new_pair() -> (TokenPair, DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();
        (
            TokenPair {
                access_token: secret(ACCESS_PREFIX),
                token_type: "Bearer",
                expires_in: ACCESS_TTL_SECS,
                refresh_token: secret(REFRESH_PREFIX),
            },
            now + chrono::Duration::seconds(ACCESS_TTL_SECS),
            now + chrono::Duration::days(REFRESH_TTL_DAYS),
        )
    }
}

/// Resource URIs compared as RFC 8707 asks: scheme and host ignore case, and
/// a trailing slash is not a different resource.
pub fn same_resource(a: &str, b: &str) -> bool {
    let canonical = |s: &str| {
        url::Url::parse(s)
            .map(|u| u.as_str().trim_end_matches('/').to_string())
            .unwrap_or_default()
    };
    let (a, b) = (canonical(a), canonical(b));
    !a.is_empty() && a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_addresses_are_fetched() {
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            // Benchmarking (198.18.0.0/15), in Sentry's SENTRY_DISALLOWED_IPS.
            "198.18.0.1",
            "198.19.255.255",
            "::ffff:198.18.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            // NAT64, 6to4 and IPv4-compatible addresses carry an IPv4 one.
            "64:ff9b::a00:1",
            "2002:a00:1::1",
            "::10.0.0.1",
            // Teredo and local-use NAT64 hide or vary where they lead.
            "2001:0:4136:e378::1",
            "64:ff9b:1::a00:1",
        ] {
            assert!(!is_public(private.parse().unwrap()), "{private}");
        }
        for public in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            // On an IPv6-only network DNS64 answers this for an IPv4 host.
            "64:ff9b::808:808",
        ] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
    }

    /// Talks to the network, so it is run by hand: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn fetching_reaches_the_internet_and_nothing_private() {
        let source = HttpClientMetadata;
        // Fetched and parsed, but a package.json is not client metadata.
        let err = source
            .fetch("https://raw.githubusercontent.com/rustrak/rustrak/main/package.json")
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("not valid client metadata"),
            "{err}"
        );
        let err = source
            .fetch("https://localhost/client.json")
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("not on the public internet"),
            "{err}"
        );
    }

    #[test]
    fn only_redirects_that_load_a_page_or_open_an_app_are_allowed() {
        for allowed in [
            "https://claude.ai/api/mcp/auth_callback",
            "http://localhost:49581/callback",
            "http://127.0.0.1/callback",
            "cursor://anysphere.cursor-mcp/oauth/callback",
            "com.example.app:/oauth2redirect",
        ] {
            assert!(redirect_allowed(allowed), "{allowed}");
        }
        for refused in [
            "javascript:alert(document.domain)//",
            "javascript://%0aalert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox(1)",
            "file:///etc/passwd",
            "blob:https://evil.example/x",
            "about:blank",
            "http://evil.example/callback",
            "not a url",
        ] {
            assert!(!redirect_allowed(refused), "{refused}");
        }
    }

    #[test]
    fn a_loopback_redirect_matches_on_any_port() {
        // RFC 8252 7.3: a native client cannot know its port in advance.
        // Claude Code registers exactly these two.
        for (registered, requested) in [
            (
                "http://localhost/callback",
                "http://localhost:49581/callback",
            ),
            (
                "http://127.0.0.1/callback",
                "http://127.0.0.1:3000/callback",
            ),
            ("http://[::1]/callback", "http://[::1]:8080/callback"),
            ("https://client.example/cb", "https://client.example/cb"),
        ] {
            assert!(redirect_matches(registered, requested), "{requested}");
        }
        for (registered, requested) in [
            ("http://localhost/callback", "http://localhost:49581/other"),
            (
                "http://localhost/callback",
                "https://localhost:49581/callback",
            ),
            (
                "http://localhost/callback",
                "http://evil.example:49581/callback",
            ),
            (
                "http://localhost/callback",
                "http://localhost.evil.example/callback",
            ),
            (
                "http://localhost/callback",
                "http://localhost:49581/callback?x=1",
            ),
            (
                "https://client.example/cb",
                "https://client.example:8443/cb",
            ),
            ("https://client.example/cb", "https://client.example/cb/"),
        ] {
            assert!(!redirect_matches(registered, requested), "{requested}");
        }
    }

    #[test]
    fn pkce_checks_the_s256_challenge() {
        // RFC 7636, Appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(pkce_matches(verifier, challenge));
        assert!(!pkce_matches("short", challenge));
        assert!(!pkce_matches(&verifier.replace('d', "e"), challenge));
    }

    #[test]
    fn resources_compare_as_rfc_8707_asks() {
        assert!(same_resource(
            "https://Rustrak.example/mcp",
            "https://rustrak.example/mcp/"
        ));
        assert!(!same_resource(
            "https://rustrak.example/mcp",
            "https://other.example/mcp"
        ));
        assert!(!same_resource("not a url", "not a url"));
    }
}
