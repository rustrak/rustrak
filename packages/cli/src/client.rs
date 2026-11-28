//! The one place requests are made.
//!
//! Every method returns the parsed body or an [`Error`]; error documents the
//! server sends (`{"error": {"type", "message", "fields"}}`) become
//! [`Error::Api`] with the fields intact, so a command can print what the
//! server rejected rather than a bare status.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Error, FieldError, Result};

const TIMEOUT: Duration = Duration::from_secs(30);

/// A thin client over one instance. Cloned per request rather than shared: the
/// inner `reqwest::Client` pools its connections, so this is cheap.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    token: Option<String>,
}

/// The error document the server sends, which is what a 4xx or 5xx body is.
#[derive(serde::Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(serde::Deserialize)]
struct ErrorDetail {
    #[serde(rename = "type", default)]
    kind: String,
    message: String,
    #[serde(default)]
    fields: Vec<serde_json::Value>,
}

impl Client {
    /// A client for `base` (already normalised: no trailing slash). `token` is
    /// attached to every request as a Bearer credential.
    pub fn new(base: impl Into<String>, token: Option<String>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&format!("rustrak-cli/{}", env!("CARGO_PKG_VERSION")))
                .expect("a static user agent is a valid header"),
        );

        // `cookie_store(true)` is what makes login work: `POST /auth/login`
        // answers with a `rustrak_session` cookie, and `POST /api/tokens`
        // authorizes from it. Without a jar the client would drop that cookie
        // and the token call would answer 401. Every other command carries a
        // Bearer token and never needs the jar.
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .default_headers(headers)
            .cookie_store(true)
            .build()?;

        Ok(Self {
            http,
            base: base.into(),
            token,
        })
    }

    /// The same client without a credential: for login and health, which the
    /// server answers without one.
    pub fn anonymous(mut self) -> Self {
        self.token = None;
        self
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut builder = self.http.request(method, self.url(path));
        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }
        builder
    }

    /// A `GET` whose body is `T`.
    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let query: Vec<_> = query.iter().filter(|(_, v)| !v.is_empty()).collect();
        let request = if query.is_empty() {
            self.request(reqwest::Method::GET, path)
        } else {
            self.request(reqwest::Method::GET, path).query(&query)
        };
        self.send(request, path).await
    }

    /// A `GET` with no meaningful body: returns the raw status and text, for
    /// endpoints whose 204 or plain-text answer is the point.
    pub async fn get_text(&self, path: &str) -> Result<String> {
        let request = self.request(reqwest::Method::GET, path);
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(parse_error(status.as_u16(), &text));
        }
        Ok(text)
    }

    /// A `POST` with a JSON body whose answer is `T`.
    pub async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let request = self.request(reqwest::Method::POST, path).json(body);
        self.send(request, path).await
    }

    /// A `POST` where the status is the answer (usually 204).
    pub async fn post_empty<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let request = self.request(reqwest::Method::POST, path).json(body);
        self.send_empty(request, path).await
    }

    /// A `PATCH` with a JSON body whose answer is `T`.
    pub async fn patch<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let request = self.request(reqwest::Method::PATCH, path).json(body);
        self.send(request, path).await
    }

    /// A `PUT` with a JSON body whose answer is `T`.
    pub async fn put<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let request = self.request(reqwest::Method::PUT, path).json(body);
        self.send(request, path).await
    }

    /// A `PUT` where the status is the answer.
    pub async fn put_empty<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let request = self.request(reqwest::Method::PUT, path).json(body);
        self.send_empty(request, path).await
    }

    /// A `DELETE`. `T` is the body when the endpoint sends one; use `()` for a
    /// 204, which is deserialized as `None` and yields `()`.
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let request = self.request(reqwest::Method::DELETE, path);
        self.send(request, path).await
    }

    /// A `DELETE` with a JSON body (bulk delete), whose answer is `T`.
    pub async fn delete_with<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let request = self.request(reqwest::Method::DELETE, path).json(body);
        self.send(request, path).await
    }

    async fn send<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
        path: &str,
    ) -> Result<T> {
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            return Err(parse_error(status.as_u16(), &text));
        }

        // A 204 has no body, and `()` is the only type that asks for one.
        if text.trim().is_empty() {
            return serde_json::from_str("null").map_err(|source| Error::Decode {
                path: path.to_string(),
                source,
            });
        }

        serde_json::from_str(&text).map_err(|source| Error::Decode {
            path: path.to_string(),
            source,
        })
    }

    async fn send_empty(&self, request: reqwest::RequestBuilder, _path: &str) -> Result<()> {
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(parse_error(status.as_u16(), &text));
        }
        Ok(())
    }
}

/// Turns an error body into [`Error::Api`], keeping the fields the server
/// named. A body that is not the documented error document (a proxy's HTML
/// 502, say) still yields a useful message rather than a decode failure.
fn parse_error(status: u16, body: &str) -> Error {
    let parsed = serde_json::from_str::<ErrorBody>(body).ok();

    match parsed {
        Some(ErrorBody { error }) => {
            let fields = error
                .fields
                .iter()
                .filter_map(|value| {
                    let object = value.as_object()?;
                    let field = object.get("field")?.as_str()?.to_string();
                    let message = object
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("invalid")
                        .to_string();
                    Some(FieldError { field, message })
                })
                .collect();
            Error::Api {
                status,
                kind: error.kind,
                message: error.message,
                fields,
            }
        }
        None => {
            let trimmed = body.trim();
            let message = if trimmed.is_empty() {
                format!("HTTP {status}")
            } else {
                // Bounded: a stray HTML page should not dump a whole document
                // into a terminal.
                let snippet: String = trimmed.chars().take(200).collect();
                format!("HTTP {status}: {snippet}")
            };
            Error::Api {
                status,
                kind: "http".to_string(),
                message,
                fields: Vec::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_document_keeps_its_fields() {
        let body = r#"{"error":{"type":"validation","message":"Invalid","fields":[{"field":"per_page","message":"must be 1..100"}]}}"#;
        let error = parse_error(400, body);
        match error {
            Error::Api {
                status,
                kind,
                fields,
                ..
            } => {
                assert_eq!(status, 400);
                assert_eq!(kind, "validation");
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].field, "per_page");
            }
            other => panic!("expected an API error, got {other:?}"),
        }
    }

    #[test]
    fn a_non_json_body_still_produces_a_message() {
        let error = parse_error(502, "<html>Bad Gateway</html>");
        match error {
            Error::Api {
                status, message, ..
            } => {
                assert_eq!(status, 502);
                assert!(message.contains("502"));
                assert!(message.contains("Bad Gateway"));
            }
            other => panic!("expected an API error, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_body_reports_the_status() {
        let error = parse_error(500, "");
        assert!(matches!(error, Error::Api { status: 500, .. }));
    }

    #[test]
    fn a_401_sets_the_auth_exit_code() {
        let error = parse_error(401, "{}");
        assert_eq!(error.exit_code(), 3);
    }
}
