//! Calls to the GitHub REST API on behalf of the instance's GitHub App.
//!
//! Two kinds of credential: a short JWT signed with the app's private key,
//! which only reaches the `/app/...` endpoints, and an installation token
//! traded for it, which acts on the repositories of one installation.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use reqwest::Method;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::signature::{SignatureEncoding, Signer};
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::models::github::{GitHubApp, GitHubRepository};
use crate::services::notification::shared_http_client;

const API_VERSION: &str = "2022-11-28";

/// Signs the JWT GitHub accepts as the app itself. Valid for nine minutes and
/// backdated one, which is what GitHub recommends against clock drift.
pub fn app_jwt(app_id: i64, private_key_pem: &str, now: DateTime<Utc>) -> AppResult<String> {
    let key = rsa::RsaPrivateKey::from_pkcs1_pem(private_key_pem)
        .map_err(|e| AppError::Internal(format!("GitHub App private key is unreadable: {e}")))?;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = URL_SAFE_NO_PAD.encode(
        json!({
            "iat": (now - Duration::seconds(60)).timestamp(),
            "exp": (now + Duration::seconds(540)).timestamp(),
            "iss": app_id.to_string(),
        })
        .to_string(),
    );
    let signing_input = format!("{header}.{claims}");
    let signer = rsa::pkcs1v15::SigningKey::<rsa::sha2::Sha256>::new(key);
    let signature = signer.sign(signing_input.as_bytes()).to_bytes();
    Ok(format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature)
    ))
}

/// Installation tokens live an hour; one is reused until five minutes before
/// it expires. Keyed by API address too, so two apps never share one.
type TokenCache = Mutex<HashMap<(String, i64), (String, DateTime<Utc>)>>;

fn token_cache() -> &'static TokenCache {
    static CACHE: OnceLock<TokenCache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Forgets every cached installation token, for when the app is disconnected.
pub fn clear_token_cache() {
    if let Ok(mut cache) = token_cache().lock() {
        cache.clear();
    }
}

async fn send(
    method: Method,
    url: &str,
    authorization: &str,
    body: Option<&Value>,
    accept: &str,
) -> AppResult<(u16, String)> {
    let mut request = shared_http_client()
        .request(method, url)
        .header("Accept", accept)
        .header("User-Agent", "rustrak")
        .header("X-GitHub-Api-Version", API_VERSION);
    if !authorization.is_empty() {
        request = request.header("Authorization", authorization);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("GitHub request to {url} failed: {e}")))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    Ok((status, text))
}

/// Turns a GitHub answer into JSON, or into an error a person can act on. A
/// 4xx is the caller's to fix (wrong repository, missing permission) and keeps
/// GitHub's own message; anything else is ours.
fn parse(status: u16, text: &str) -> AppResult<Value> {
    if (200..300).contains(&status) {
        return Ok(if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(text)
                .map_err(|e| AppError::Internal(format!("GitHub sent invalid JSON: {e}")))?
        });
    }
    let message = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| text.chars().take(200).collect());
    match status {
        404 => Err(AppError::NotFound(format!("GitHub: {message}"))),
        400..=499 => Err(AppError::Validation(format!("GitHub: {message}"))),
        _ => Err(AppError::Internal(format!(
            "GitHub answered {status}: {message}"
        ))),
    }
}

/// Trades the temporary code from the manifest flow for the new app's
/// credentials. The one call that needs no credential at all.
pub async fn convert_manifest(api_url: &str, code: &str) -> AppResult<Value> {
    let url = format!("{api_url}/app-manifests/{code}/conversions");
    let (status, text) = send(Method::POST, &url, "", None, "application/vnd.github+json").await?;
    parse(status, &text)
}

/// A client acting as one GitHub App.
pub struct GitHubClient<'a> {
    app: &'a GitHubApp,
}

impl<'a> GitHubClient<'a> {
    pub fn new(app: &'a GitHubApp) -> Self {
        Self { app }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.app.api_url.trim_end_matches('/'), path)
    }

    async fn as_app(&self, method: Method, path: &str) -> AppResult<Value> {
        let jwt = app_jwt(self.app.app_id, &self.app.private_key, Utc::now())?;
        let (status, text) = send(
            method,
            &self.url(path),
            &format!("Bearer {jwt}"),
            None,
            "application/vnd.github+json",
        )
        .await?;
        parse(status, &text)
    }

    async fn installation_token(&self, installation_id: i64) -> AppResult<String> {
        let key = (self.app.api_url.clone(), installation_id);
        if let Ok(cache) = token_cache().lock() {
            if let Some((token, expires_at)) = cache.get(&key) {
                if *expires_at - Duration::minutes(5) > Utc::now() {
                    return Ok(token.clone());
                }
            }
        }
        let answer = self
            .as_app(
                Method::POST,
                &format!("/app/installations/{installation_id}/access_tokens"),
            )
            .await?;
        let token = answer
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Internal("GitHub sent no installation token".into()))?
            .to_string();
        let expires_at = answer
            .get("expires_at")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() + Duration::minutes(30));
        if let Ok(mut cache) = token_cache().lock() {
            cache.insert(key, (token.clone(), expires_at));
        }
        Ok(token)
    }

    async fn as_installation(
        &self,
        installation_id: i64,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> AppResult<Value> {
        let token = self.installation_token(installation_id).await?;
        let (status, text) = send(
            method,
            &self.url(path),
            &format!("token {token}"),
            body,
            "application/vnd.github+json",
        )
        .await?;
        parse(status, &text)
    }

    /// The account an installation belongs to: `(login, "User" | "Organization")`.
    pub async fn installation_account(&self, installation_id: i64) -> AppResult<(String, String)> {
        let answer = self
            .as_app(
                Method::GET,
                &format!("/app/installations/{installation_id}"),
            )
            .await?;
        let account = answer.get("account").cloned().unwrap_or(Value::Null);
        Ok((
            account
                .get("login")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            account
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("User")
                .to_string(),
        ))
    }

    /// Every repository the installation was granted, up to 20,000 (Sentry's
    /// cap: 200 pages of 100).
    pub async fn repositories(&self, installation_id: i64) -> AppResult<Vec<GitHubRepository>> {
        use futures_util::{StreamExt, TryStreamExt};

        let page = |page: u64| async move {
            self.as_installation(
                installation_id,
                Method::GET,
                &format!("/installation/repositories?per_page=100&page={page}"),
                None,
            )
            .await
        };
        let first = page(1).await?;
        let total = first
            .get("total_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        // The first page says how many follow; fetching them one after
        // another outlasts the dashboard's request timeout on a large org.
        let rest: Vec<Value> = futures_util::stream::iter(2..=total.div_ceil(100).min(200))
            .map(page)
            .buffered(8)
            .try_collect()
            .await?;
        Ok(std::iter::once(first)
            .chain(rest)
            .flat_map(|answer| {
                answer
                    .get("repositories")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            })
            .map(|repo| repository_from_json(installation_id, &repo))
            .collect())
    }

    pub async fn repository(
        &self,
        installation_id: i64,
        full_name: &str,
    ) -> AppResult<GitHubRepository> {
        let answer = self
            .as_installation(
                installation_id,
                Method::GET,
                &format!("/repos/{full_name}"),
                None,
            )
            .await?;
        Ok(repository_from_json(installation_id, &answer))
    }

    /// Opens an issue and answers `(number, title, html_url, state)`.
    pub async fn create_issue(
        &self,
        installation_id: i64,
        repo: &str,
        title: &str,
        body: &str,
        labels: &[String],
        assignee: Option<&str>,
    ) -> AppResult<RemoteIssue> {
        let mut issue = json!({ "title": title, "body": body });
        if !labels.is_empty() {
            issue["labels"] = json!(labels);
        }
        if let Some(assignee) = assignee {
            issue["assignees"] = json!([assignee]);
        }
        let answer = self
            .as_installation(
                installation_id,
                Method::POST,
                &format!("/repos/{repo}/issues"),
                Some(&issue),
            )
            .await?;
        Ok(RemoteIssue::from_json(&answer))
    }

    /// The logins `repo`'s issues can be assigned to.
    // ponytail: first 100 only; page through them if a repository has more.
    pub async fn assignees(&self, installation_id: i64, repo: &str) -> AppResult<Vec<String>> {
        let answer = self
            .as_installation(
                installation_id,
                Method::GET,
                &format!("/repos/{repo}/assignees?per_page=100"),
                None,
            )
            .await?;
        Ok(answer
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|user| user["login"].as_str().map(String::from))
            .collect())
    }

    /// The issues opened in `repo` since `since`, newest first, each with
    /// its body.
    pub async fn issues_since(
        &self,
        installation_id: i64,
        repo: &str,
        since: DateTime<Utc>,
    ) -> AppResult<Vec<(String, RemoteIssue)>> {
        let answer = self
            .as_installation(
                installation_id,
                Method::GET,
                &format!(
                    "/repos/{repo}/issues?state=all&sort=created&direction=desc&per_page=100&since={}",
                    since.format("%Y-%m-%dT%H:%M:%SZ")
                ),
                None,
            )
            .await?;
        Ok(answer
            .as_array()
            .into_iter()
            .flatten()
            .map(|issue| {
                let body = issue
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                (body.to_string(), RemoteIssue::from_json(issue))
            })
            .collect())
    }

    pub async fn issue(
        &self,
        installation_id: i64,
        repo: &str,
        number: i64,
    ) -> AppResult<RemoteIssue> {
        let answer = self
            .as_installation(
                installation_id,
                Method::GET,
                &format!("/repos/{repo}/issues/{number}"),
                None,
            )
            .await?;
        Ok(RemoteIssue::from_json(&answer))
    }

    /// `state` is `open` or `closed`.
    pub async fn set_issue_state(
        &self,
        installation_id: i64,
        repo: &str,
        number: i64,
        state: &str,
    ) -> AppResult<()> {
        self.as_installation(
            installation_id,
            Method::PATCH,
            &format!("/repos/{repo}/issues/{number}"),
            Some(&json!({ "state": state })),
        )
        .await?;
        Ok(())
    }

    pub async fn set_assignees(
        &self,
        installation_id: i64,
        repo: &str,
        number: i64,
        logins: &[String],
    ) -> AppResult<()> {
        self.as_installation(
            installation_id,
            Method::PATCH,
            &format!("/repos/{repo}/issues/{number}"),
            Some(&json!({ "assignees": logins })),
        )
        .await?;
        Ok(())
    }

    pub async fn add_comment(
        &self,
        installation_id: i64,
        repo: &str,
        number: i64,
        body: &str,
    ) -> AppResult<()> {
        self.as_installation(
            installation_id,
            Method::POST,
            &format!("/repos/{repo}/issues/{number}/comments"),
            Some(&json!({ "body": body })),
        )
        .await?;
        Ok(())
    }

    /// A file's text on the default branch, or `None` when it does not exist.
    pub async fn file(
        &self,
        installation_id: i64,
        repo: &str,
        path: &str,
    ) -> AppResult<Option<String>> {
        let token = self.installation_token(installation_id).await?;
        let (status, text) = send(
            Method::GET,
            &self.url(&format!("/repos/{repo}/contents/{path}")),
            &format!("token {token}"),
            None,
            "application/vnd.github.raw+json",
        )
        .await?;
        match status {
            200 => Ok(Some(text)),
            404 => Ok(None),
            _ => parse(status, &text).map(|_| None),
        }
    }
}

fn repository_from_json(installation_id: i64, repo: &Value) -> GitHubRepository {
    let text = |key: &str| {
        repo.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    GitHubRepository {
        installation_id,
        full_name: text("full_name"),
        default_branch: repo
            .get("default_branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string(),
        private: repo
            .get("private")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        html_url: text("html_url"),
    }
}

/// The parts of a GitHub issue Rustrak keeps.
#[derive(Debug, Clone)]
pub struct RemoteIssue {
    pub number: i64,
    pub title: String,
    pub url: String,
    pub state: String,
}

impl RemoteIssue {
    pub fn from_json(issue: &Value) -> Self {
        let text = |key: &str| {
            issue
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            number: issue.get("number").and_then(Value::as_i64).unwrap_or(0),
            title: text("title"),
            url: text("html_url"),
            state: issue
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("open")
                .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_jwt_is_rs256_signed_and_names_the_app() {
        use rsa::signature::Verifier;
        let pem = include_str!("../../../tests/fixtures/github_app_test_key.txt");
        let now = Utc::now();
        let jwt = app_jwt(42, pem, now).unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["iss"], "42");
        assert_eq!(claims["exp"], (now + Duration::seconds(540)).timestamp());

        let key = rsa::RsaPrivateKey::from_pkcs1_pem(pem).unwrap();
        let verifier = rsa::pkcs1v15::VerifyingKey::<rsa::sha2::Sha256>::new(key.to_public_key());
        let signature = rsa::pkcs1v15::Signature::try_from(
            URL_SAFE_NO_PAD.decode(parts[2]).unwrap().as_slice(),
        )
        .unwrap();
        let signed = format!("{}.{}", parts[0], parts[1]);
        assert!(verifier.verify(signed.as_bytes(), &signature).is_ok());
        assert!(app_jwt(42, "not a key", now).is_err());
    }

    #[test]
    fn github_errors_keep_their_message_and_status_class() {
        let not_found = parse(404, r#"{"message":"Not Found"}"#).unwrap_err();
        assert!(matches!(not_found, AppError::NotFound(m) if m == "GitHub: Not Found"));
        let refused = parse(422, r#"{"message":"Validation Failed"}"#).unwrap_err();
        assert!(matches!(refused, AppError::Validation(_)));
        assert!(matches!(
            parse(502, "bad gateway"),
            Err(AppError::Internal(_))
        ));
        assert_eq!(parse(204, "").unwrap(), Value::Null);
    }
}
