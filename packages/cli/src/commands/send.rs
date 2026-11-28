//! Send a test event to a DSN, the way an SDK would.
//!
//! This is the one command that does not use the management API: it talks to
//! the ingest endpoint with the DSN's key, exactly as `sentry_sdk` would. That
//! makes it the fastest way to prove an SDK integration is wired correctly
//! before an app is deployed, and it works with `--dsn` alone, so a user who
//! has only copied the DSN out of the dashboard needs nothing else.

use chrono::Utc;
use serde_json::json;

use crate::cli::SendArgs;
use crate::commands::Context;
use crate::error::{Error, Result};

/// A DSN, split the way the Sentry protocol splits it:
/// `http://<key>@<host>/<project_id>`. The scheme and host are the ingest
/// origin, the user part is the project's public key, and the path is its id.
struct Dsn {
    origin: String,
    key: String,
    project_id: String,
}

impl Dsn {
    fn parse(dsn: &str) -> Result<Self> {
        let url =
            url::Url::parse(dsn).map_err(|e| Error::Invalid(format!("that is not a DSN: {e}")))?;

        let key = if url.username().is_empty() {
            return Err(Error::Invalid(
                "the DSN has no public key; it should look like http://<key>@host/<project>"
                    .to_string(),
            ));
        } else {
            url.username().to_string()
        };

        let project_id = url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Error::Invalid("the DSN has no project id in its path".to_string()))?
            .to_string();

        let origin = format!(
            "{}://{}",
            url.scheme(),
            url.host_str()
                .map(|host| match url.port() {
                    Some(port) => format!("{host}:{port}"),
                    None => host.to_string(),
                })
                .ok_or_else(|| Error::Invalid("the DSN has no host".to_string()))?
        );

        Ok(Self {
            origin,
            key,
            project_id,
        })
    }

    /// Where Relay-compatible SDKs POST an event.
    fn store_url(&self) -> String {
        format!("{}/api/{}/store/", self.origin, self.project_id)
    }
}

pub async fn run(args: SendArgs, ctx: &Context) -> Result<()> {
    // A DSN, or the project's DSN looked up with the token.
    let dsn = match args.dsn {
        Some(dsn) => dsn,
        None => {
            ctx.require_auth()?;
            let project = ctx.project(None).await?;
            project.dsn.ok_or_else(|| {
                Error::Invalid(
                    "that project has no DSN you can read; pass --dsn with the project's DSN"
                        .to_string(),
                )
            })?
        }
    };

    let parsed = Dsn::parse(&dsn)?;
    let event_id = uuid_like();

    let mut event = json!({
        "event_id": event_id,
        "timestamp": Utc::now().to_rfc3339(),
        "platform": "other",
        "level": args.level,
        "message": args.message,
        "logger": "rustrak-cli",
        "server_name": hostname(),
        "sdk": { "name": "rustrak.cli", "version": env!("CARGO_PKG_VERSION") },
        "tags": { "sent-by": "rustrak-cli" },
    });

    if let Some(environment) = &args.environment {
        event["environment"] = json!(environment);
    }
    if let Some(release) = &args.release {
        event["release"] = json!(release);
    }

    // The ingest endpoint authenticates with the DSN's key in the
    // `X-Sentry-Auth` header, not the management API's Bearer token.
    let http = reqwest::Client::new();
    let auth_header = format!(
        "Sentry sentry_version=7, sentry_client=rustrak-cli/{}, sentry_key={}",
        env!("CARGO_PKG_VERSION"),
        parsed.key
    );

    let response = http
        .post(parsed.store_url())
        .header("X-Sentry-Auth", auth_header)
        .header("Content-Type", "application/json")
        .body(event.to_string())
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(Error::Api {
            status: status.as_u16(),
            kind: "ingest".to_string(),
            message: format!("the instance rejected the event: {}", body.trim()),
            fields: Vec::new(),
        });
    }

    if ctx.output.is_json() {
        return ctx.output.json(&json!({
            "sent": true,
            "event_id": event_id,
            "project_id": parsed.project_id,
            "url": parsed.origin,
        }));
    }

    println!("Sent event {event_id} to project {}.", parsed.project_id);
    Ok(())
}

/// 32 lowercase hex digits, which is the shape Sentry event ids take.
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let mixed = nanos
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(pid << 64);
    format!("{mixed:032x}")
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dsn_splits_into_origin_key_and_project() {
        let dsn = Dsn::parse("http://abc123@localhost:8080/7").unwrap();
        assert_eq!(dsn.origin, "http://localhost:8080");
        assert_eq!(dsn.key, "abc123");
        assert_eq!(dsn.project_id, "7");
        assert_eq!(dsn.store_url(), "http://localhost:8080/api/7/store/");
    }

    #[test]
    fn a_https_dsn_keeps_its_scheme() {
        let dsn = Dsn::parse("https://key@rustrak.example.com/42").unwrap();
        assert_eq!(dsn.origin, "https://rustrak.example.com");
        assert_eq!(dsn.store_url(), "https://rustrak.example.com/api/42/store/");
    }

    #[test]
    fn a_dsn_without_a_key_is_refused() {
        assert!(Dsn::parse("http://localhost:8080/7").is_err());
    }

    #[test]
    fn a_dsn_without_a_project_is_refused() {
        assert!(Dsn::parse("http://key@localhost:8080").is_err());
    }

    #[test]
    fn an_event_id_is_32_hex_digits() {
        let id = uuid_like();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
