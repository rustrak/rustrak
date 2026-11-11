//! Custom Webhook notification dispatcher.
//!
//! Like [`super::webhook`], sends alerts as HTTP POST requests with JSON
//! bodies and optional HMAC-SHA256 signatures — but the body is rendered
//! from a user-supplied [Minijinja](https://docs.rs/minijinja) template with
//! the [`AlertPayload`] as context. The rendered result must be valid JSON;
//! that is what lets a single template target the fixed message schemas of
//! WeCom, DingTalk and Feishu bots, which reject foreign payloads.
//!
//! Template variables are the serialised payload fields:
//! `alert_id`, `alert_type`, `triggered_at`, `project.{id,name,slug}`,
//! `issue.{id,short_id,title,level,first_seen,last_seen,event_count}`,
//! `issue_url`, `actor`. Values are escaped for where they sit (see
//! [`super::json_template`]): inside a string literal as string content, in a
//! value position as a JSON value. Unknown paths are a render error.
//!
//! Routing override is the same flat shape as the webhook's:
//! `{"url": "...", "extra_headers": {...}}`.
//! Effective URL (K5): routing.url ?? credentials.url

use async_trait::async_trait;
use chrono::Utc;

use super::json_template;
use super::webhook::{own_headers, request_headers, validate_headers, WebhookNotifier};
use super::{deliver, http_result, NotificationDispatcher, NotificationResult};
use crate::error::{AppError, AppResult};
use crate::models::{AlertIntegration, AlertPayload, CustomWebhookConfig, WebhookRoutingOverride};

pub struct CustomWebhookNotifier {
    client: reqwest::Client,
}

/// Upper bound for the body a template may render. Generous next to any bot
/// message, small next to what an unbounded loop produces; the writer stops at
/// it so the bytes past the bound are never allocated.
const MAX_RENDERED_BODY_BYTES: usize = 1024 * 1024;

/// Instruction budget for one render. A preset costs a few dozen; a loop over
/// a hundred issues with formatting costs thousands. A template that wants
/// more than this is not formatting an alert.
const RENDER_FUEL: u64 = 500_000;

/// A sink that refuses to grow past `limit`, so a runaway template fails at
/// the bound instead of allocating whatever it decided to produce.
struct LimitedWriter {
    buffer: Vec<u8>,
    limit: usize,
}

impl LimitedWriter {
    fn new(limit: usize) -> Self {
        Self {
            buffer: Vec::new(),
            limit,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.buffer
    }
}

impl std::io::Write for LimitedWriter {
    fn write(&mut self, chunk: &[u8]) -> std::io::Result<usize> {
        if self.buffer.len() + chunk.len() > self.limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "rendered body exceeds the configured limit",
            ));
        }
        self.buffer.extend_from_slice(chunk);
        Ok(chunk.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl CustomWebhookNotifier {
    pub fn new() -> Self {
        Self {
            client: super::shared_http_client().clone(),
        }
    }

    /// Computes the effective URL: routing.url ?? credentials.url (K5).
    pub fn effective_url<'a>(
        routing: &'a WebhookRoutingOverride,
        credentials: &'a CustomWebhookConfig,
    ) -> Option<&'a str> {
        routing.url.as_deref().or(credentials.url.as_deref())
    }

    /// Validates a credential-level URL if present. Empty or non-http(s)
    /// URLs are rejected; absent URLs are legal because routing may supply one.
    fn validate_url(url: &Option<String>) -> AppResult<()> {
        let Some(url) = url.as_deref() else {
            return Ok(());
        };
        if url.is_empty() {
            return Err(AppError::Validation(
                "Custom webhook URL cannot be empty if provided".to_string(),
            ));
        }
        let parsed = url::Url::parse(url)
            .map_err(|_| AppError::Validation("Invalid custom webhook URL format".to_string()))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(AppError::Validation(
                "Custom webhook URL must use HTTP or HTTPS".to_string(),
            ));
        }
        Ok(())
    }

    /// Renders the template against the payload and returns the body bytes
    /// exactly as rendered, or a human-readable failure reason.
    ///
    /// The bytes are checked to be JSON and then sent as they are: the keys in
    /// the order the writer put them, the whitespace they chose. Re-serialising
    /// through a `Value` would reorder the keys, and a preview that shows one
    /// order while the wire carries another is a preview that lies.
    ///
    /// Two guards bound the work, because a template is user input that runs
    /// on the notification worker once per delivery. Fuel stops a template
    /// that burns instructions without producing output (minijinja bounds a
    /// single `range()`, not the product of two nested ones), and the writer
    /// stops one that produces output faster than it spends fuel, before the
    /// bytes are ever allocated.
    fn render_body(template: &str, payload: &AlertPayload) -> Result<Vec<u8>, String> {
        let env = Self::template_env(template)?;
        let mut rendered = LimitedWriter::new(MAX_RENDERED_BODY_BYTES);
        env.get_template("body")
            .map_err(|e| format!("Invalid template: {e}"))?
            .render_captured_to(minijinja::value::Serde(payload), &mut rendered)
            .map_err(|e| Self::describe_render_error(&e))?;
        let body = rendered.into_inner();
        serde_json::from_slice::<serde::de::IgnoredAny>(&body)
            .map_err(|e| format!("Rendered template is not valid JSON: {e}"))?;
        Ok(body)
    }

    /// The environment every render and every save-time check uses, so the
    /// limits cannot drift between validation and delivery.
    ///
    /// The template is rewritten by [`json_template::autoescape`] before it
    /// compiles, so a value is escaped for wherever the writer put it. Unknown
    /// fields fail rather than print as nothing: a typo is a save-time error
    /// in words, not a hole in a message that goes out.
    fn template_env(template: &str) -> Result<minijinja::Environment<'static>, String> {
        let mut env = minijinja::Environment::new();
        env.set_fuel(Some(RENDER_FUEL));
        env.set_undefined_behavior(minijinja::UndefinedBehavior::SemiStrict);
        env.add_filter(json_template::STRING_FILTER, json_template::json_string);
        env.add_filter(json_template::VALUE_FILTER, json_template::json_value);
        env.add_template_owned("body", json_template::autoescape(template))
            .map_err(|e| format!("Invalid template: {e}"))?;
        Ok(env)
    }

    /// Turns a render failure into a reason a dashboard can show. The byte cap
    /// surfaces as an io error wrapped by minijinja, and exhausted fuel as
    /// minijinja's own error; both are the template asking for too much, so
    /// both are reported as such rather than as an engine internal.
    fn describe_render_error(error: &minijinja::Error) -> String {
        if error.kind() == minijinja::ErrorKind::WriteFailure {
            return format!("Rendered body exceeds the {MAX_RENDERED_BODY_BYTES}-byte limit");
        }
        if error.kind() == minijinja::ErrorKind::OutOfFuel {
            return "Template is too complex to render within the instruction budget".to_string();
        }
        format!("Template rendering failed: {error}")
    }

    /// The payloads `validate_config` renders against: one with every field
    /// populated, one with the optional issue level absent. Kept beside the
    /// dispatcher so save-time validation and delivery always agree on the
    /// context shape.
    fn validation_samples() -> [AlertPayload; 2] {
        let now = Utc::now();
        let full = AlertPayload {
            alert_id: "00000000-0000-0000-0000-000000000000".to_string(),
            alert_type: "new_issue".to_string(),
            triggered_at: now,
            project: crate::models::ProjectInfo {
                id: 1,
                name: "Sample Project".to_string(),
                slug: "sample-project".to_string(),
            },
            issue: crate::models::IssueInfo {
                id: "00000000-0000-0000-0000-000000000000".to_string(),
                short_id: "SAMPLE-1".to_string(),
                title: "Sample issue".to_string(),
                level: Some("error".to_string()),
                first_seen: now,
                last_seen: now,
                event_count: 1,
            },
            issue_url: "https://rustrak.example/issues/sample".to_string(),
            actor: "Rustrak".to_string(),
        };
        let without_level = AlertPayload {
            issue: crate::models::IssueInfo {
                level: None,
                ..full.issue.clone()
            },
            ..full.clone()
        };
        [full, without_level]
    }

    /// Renders `template` against the fully populated sample payload and
    /// returns the body byte for byte as it would be sent.
    ///
    /// The dashboard cannot run minijinja, so it asks for this while the
    /// reader types. It goes through [`Self::render_body`] rather than
    /// reimplementing it, so the preview cannot disagree with a delivery, and
    /// the render budget applies here too: a preview is not a way to spend
    /// what a delivery may not.
    pub fn preview_template(template: &str) -> Result<String, String> {
        let [sample, _] = Self::validation_samples();
        let body = Self::render_body(template, &sample)?;
        String::from_utf8(body).map_err(|e| format!("Rendered body is not UTF-8: {e}"))
    }
}

impl Default for CustomWebhookNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NotificationDispatcher for CustomWebhookNotifier {
    async fn send(
        &self,
        integration: &AlertIntegration,
        routing: &serde_json::Value,
        payload: &AlertPayload,
    ) -> NotificationResult {
        let credentials: CustomWebhookConfig =
            match serde_json::from_value(integration.credentials.clone()) {
                Ok(c) => c,
                Err(e) => {
                    return NotificationResult::failure(
                        format!("Invalid custom webhook credentials: {}", e),
                        None,
                    )
                }
            };

        let routing_override: WebhookRoutingOverride = serde_json::from_value(routing.clone())
            .unwrap_or(WebhookRoutingOverride {
                url: None,
                extra_headers: None,
            });

        let url = match Self::effective_url(&routing_override, &credentials) {
            Some(u) => u.to_string(),
            None => {
                return NotificationResult::failure(
                    "Custom webhook URL not configured in credentials or routing_override"
                        .to_string(),
                    None,
                )
            }
        };

        let body = match Self::render_body(&credentials.template, payload) {
            Ok(b) => b,
            Err(e) => return NotificationResult::failure(e, None),
        };

        let timestamp = Utc::now().timestamp().to_string();

        let signature = credentials
            .secret
            .as_deref()
            .map(|secret| WebhookNotifier::generate_signature(secret, &timestamp, &body));
        let own = match own_headers(&timestamp, &payload.alert_id, signature) {
            Ok(h) => h,
            Err(e) => return NotificationResult::failure(e, None),
        };
        // Same precedence as the plain webhook dispatcher.
        let headers = match request_headers(
            [
                credentials.headers.as_ref(),
                routing_override.extra_headers.as_ref(),
            ],
            own,
        ) {
            Ok(h) => h,
            Err(e) => return NotificationResult::failure(e, None),
        };

        match deliver(self.client.post(&url).headers(headers).body(body)).await {
            Ok((status, body)) => http_result(status, body),
            Err(failure) => failure,
        }
    }

    fn validate_config(&self, config: &serde_json::Value) -> AppResult<()> {
        let config: CustomWebhookConfig = serde_json::from_value(config.clone())
            .map_err(|e| AppError::Validation(format!("Invalid custom webhook config: {}", e)))?;

        Self::validate_url(&config.url)?;
        validate_headers(config.headers.as_ref())?;

        if config.template.trim().is_empty() {
            return Err(AppError::Validation("Template cannot be empty".to_string()));
        }

        // Compile check first, so a syntax error reads as one rather than as a
        // render failure.
        Self::template_env(&config.template)
            .map_err(|e| AppError::Validation(format!("Invalid template syntax: {}", e)))?;

        // Then render it, because compiling proves nothing about the output.
        // The common mistake is a bare interpolation in a value position: valid
        // syntax, never valid JSON, and before this it only surfaced when a
        // real incident fired. Both samples must render, so a template that
        // holds together only while an optional field is present is caught too.
        for sample in Self::validation_samples() {
            Self::render_body(&config.template, &sample)
                .map_err(|e| AppError::Validation(format!("Template check failed: {e}")))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{IssueInfo, ProjectInfo};
    use serde_json::json;

    fn make_config(template: &str, url: Option<&str>) -> serde_json::Value {
        json!({
            "url": url,
            "template": template,
        })
    }

    fn make_integration(credentials: serde_json::Value, is_enabled: bool) -> AlertIntegration {
        AlertIntegration {
            id: 4,
            name: "Test Custom Webhook".to_string(),
            provider_type: crate::models::ProviderType::CustomWebhook,
            credentials,
            is_enabled,
            failure_count: 0,
            last_failure_at: None,
            last_failure_message: None,
            last_success_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn create_test_payload() -> AlertPayload {
        AlertPayload {
            alert_id: "test-123".to_string(),
            alert_type: "new_issue".to_string(),
            triggered_at: Utc::now(),
            project: ProjectInfo {
                id: 1,
                name: "Test Project".to_string(),
                slug: "test-project".to_string(),
            },
            issue: IssueInfo {
                id: "abc-123".to_string(),
                short_id: "TEST-1".to_string(),
                title: "Test error".to_string(),
                level: Some("error".to_string()),
                first_seen: Utc::now(),
                last_seen: Utc::now(),
                event_count: 1,
            },
            issue_url: "https://example.com/issues/abc-123".to_string(),
            actor: "Rustrak".to_string(),
        }
    }

    #[test]
    fn test_render_dingtalk_style_template() {
        let template = r#"{"msgtype":"text","text":{"content":"Rustrak: {{ issue.title }} ({{ issue.short_id }}) in {{ project.name }}"}}"#;
        let body = CustomWebhookNotifier::render_body(template, &create_test_payload())
            .expect("dingtalk text template must render to valid JSON");
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["msgtype"], "text");
        assert_eq!(
            parsed["text"]["content"],
            "Rustrak: Test error (TEST-1) in Test Project"
        );
    }

    #[test]
    fn test_render_wecom_markdown_template_with_url_field() {
        let template = r#"{"msgtype":"markdown","markdown":{"content":"**{{ issue.title }}** {{ issue_url }}"}}"#;
        let body = CustomWebhookNotifier::render_body(template, &create_test_payload())
            .expect("wecom markdown template must render");
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            parsed["markdown"]["content"],
            "**Test error** https://example.com/issues/abc-123"
        );
    }

    #[test]
    fn test_render_result_is_sent_as_written() {
        // Key order and whitespace are the writer's. A body that came back
        // reordered would not be the one the preview showed.
        let template = "{\n  \"z\": 1,\n  \"a\": \"{{ project.slug }}\"\n}";
        let body =
            CustomWebhookNotifier::render_body(template, &create_test_payload()).expect("renders");
        assert_eq!(
            String::from_utf8(body).unwrap(),
            "{\n  \"z\": 1,\n  \"a\": \"test-project\"\n}"
        );
    }

    #[test]
    fn test_a_body_that_is_not_json_is_reported_not_sent() {
        // A comma the writer forgot. Rendering cannot fix the JSON around the
        // expressions, so the failure is surfaced with the parser's reason.
        let template = r#"{"msgtype":"text" "text":{"content":"{{ issue.title }}"}}"#;
        let err = CustomWebhookNotifier::render_body(template, &create_test_payload())
            .expect_err("a broken body must fail JSON validation");
        assert!(err.contains("not valid JSON"), "got: {err}");
    }

    #[test]
    fn test_tojson_filter_quotes_strings() {
        let payload = AlertPayload {
            issue: IssueInfo {
                title: r#"He said "boom""#.to_string(),
                ..create_test_payload().issue
            },
            ..create_test_payload()
        };
        let template = r#"{"content":{{ issue.title | tojson }}}"#;
        let body = CustomWebhookNotifier::render_body(template, &payload).expect("tojson renders");
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["content"], r#"He said "boom""#);
    }

    // -------------------------------------------------------------------------
    // Context-aware escaping: the body is written the natural way
    // -------------------------------------------------------------------------

    fn quoted_title_payload() -> AlertPayload {
        AlertPayload {
            issue: IssueInfo {
                title: "He said \"boom\"\nthen left".to_string(),
                ..create_test_payload().issue
            },
            ..create_test_payload()
        }
    }

    #[test]
    fn test_a_value_inside_a_string_is_escaped_for_the_writer() {
        // The body most people write first. It has to survive a title with a
        // quote and a newline in it without anyone learning a filter.
        let template = r#"{"msgtype":"text","text":{"content":"Rustrak: {{ issue.title }}"}}"#;
        let body = CustomWebhookNotifier::render_body(template, &quoted_title_payload())
            .expect("a quoted title inside a string must render");
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            parsed["text"]["content"],
            "Rustrak: He said \"boom\"\nthen left"
        );
    }

    #[test]
    fn test_a_value_in_value_position_becomes_a_json_value() {
        let template = r#"{"title": {{ issue.title }}, "count": {{ issue.event_count }}}"#;
        let body = CustomWebhookNotifier::render_body(template, &quoted_title_payload())
            .expect("bare interpolations in value position must render");
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["title"], "He said \"boom\"\nthen left");
        assert_eq!(parsed["count"], 1);
    }

    #[test]
    fn test_an_absent_optional_field_reads_as_nothing_or_null() {
        let payload = AlertPayload {
            issue: IssueInfo {
                level: None,
                ..create_test_payload().issue
            },
            ..create_test_payload()
        };
        let template = r#"{"line": "Level: {{ issue.level }}", "level": {{ issue.level }}}"#;
        let body = CustomWebhookNotifier::render_body(template, &payload).expect("renders");
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["line"], "Level: ");
        assert!(parsed["level"].is_null());
    }

    #[test]
    fn test_explicit_tojson_still_means_what_it_says() {
        let template = r#"{"content":{{ issue.title | tojson }}}"#;
        let body = CustomWebhookNotifier::render_body(template, &quoted_title_payload())
            .expect("explicit tojson renders");
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["content"], "He said \"boom\"\nthen left");
    }

    #[test]
    fn test_a_field_that_does_not_exist_is_named_not_blanked() {
        // Lenient rendering would print "" and the body would go out with a
        // hole in it. A typo has to fail at save time, in words.
        for (template, named) in [
            (r#"{"t": "{{ issue.titel }}"}"#, "`issue.titel`"),
            (r#"{"t": {{ nope }}}"#, "`nope`"),
        ] {
            let err = CustomWebhookNotifier::render_body(template, &create_test_payload())
                .expect_err("an unknown field must not render");
            assert!(err.contains("undefined"), "got: {err}");
            assert!(
                err.contains(named),
                "the failure must name what was typed, got: {err}"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Render limits
    // -------------------------------------------------------------------------

    #[test]
    fn test_render_aborts_a_template_that_burns_cpu_without_output() {
        // Nested ranges: minijinja bounds a single range() but not the product
        // of two, so the only guard is fuel. Unguarded this renders ~1 MB of
        // "x" through a million iterations; the point is that it stops.
        let template = r#"{"a":"{% for i in range(1000) %}{% for j in range(1000) %}x{% endfor %}{% endfor %}"}"#;
        let err = CustomWebhookNotifier::render_body(template, &create_test_payload())
            .map(|b| b.len())
            .expect_err("an unbounded template must not render");
        assert!(
            err.contains("too complex"),
            "the failure must name the resource limit, got: {err}"
        );
    }

    #[test]
    fn test_render_aborts_a_template_whose_body_exceeds_the_cap() {
        // Cheap in instructions, enormous in bytes: fuel alone would let this
        // through, so the byte cap is what stops it.
        let filler = "x".repeat(4096);
        let template = format!(r#"{{"a":"{{% for i in range(1000) %}}{filler}{{% endfor %}}"}}"#);
        let err = CustomWebhookNotifier::render_body(&template, &create_test_payload())
            .map(|b| b.len())
            .expect_err("a body past the cap must not render");
        assert!(
            err.contains("exceeds"),
            "the failure must name the size limit, got: {err}"
        );
    }

    #[test]
    fn test_render_limits_leave_a_realistic_template_alone() {
        // The guards must not fire on the kind of template the feature exists
        // for: a loop with formatting, well inside both bounds.
        let template = r#"{"lines":[{% for i in range(50) %}{{ (issue.title ~ i) | tojson }}{% if not loop.last %},{% endif %}{% endfor %}]}"#;
        let body = CustomWebhookNotifier::render_body(template, &create_test_payload())
            .expect("a legitimate loop must render");
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["lines"].as_array().unwrap().len(), 50);
    }

    #[test]
    fn test_status_is_success_matches_2xx() {
        assert!(super::super::http_result(200, String::new()).success);
        assert!(super::super::http_result(204, String::new()).success);
        assert!(!super::super::http_result(301, String::new()).success);
        assert!(!super::super::http_result(500, String::new()).success);
    }

    // -------------------------------------------------------------------------
    // Presets and documented example bodies
    //
    // Rustrak carries no per-service message shapes in code; the bodies below
    // are the "Start from" presets the dashboard offers (message-template.ts
    // `TEMPLATE_PRESETS`) and the examples in the docs. They are rendered here
    // so a preset that would hand someone an invalid template fails this test
    // instead of failing in their alert history.
    // -------------------------------------------------------------------------

    const PRESET_WECOM_TEXT: &str = r#"{"msgtype":"text","text":{"content":"Rustrak: {{ issue.title }} ({{ issue.short_id }})\n{{ issue_url }}"}}"#;

    // r#### because the body contains `"###`, which would close an r#"…"#.
    const PRESET_WECOM_MARKDOWN: &str = r####"{"msgtype":"markdown","markdown":{"content":"### {{ issue.title }}\n> Project: {{ project.name }}\n> Level: {{ issue.level }}\n> [View issue]({{ issue_url }})"}}"####;

    const PRESET_DINGTALK_TEXT: &str = PRESET_WECOM_TEXT;

    const PRESET_FEISHU_TEXT: &str = r#"{"msg_type":"text","content":{"text":"Rustrak: {{ issue.title }} ({{ issue.short_id }})\n{{ issue_url }}"}}"#;

    const PRESET_DISCORD: &str =
        r#"{"content":"**{{ issue.title }}** ({{ issue.short_id }})\n{{ issue_url }}"}"#;

    const PRESET_SLACK_COMPATIBLE: &str =
        r#"{"text":"*{{ issue.title }}* ({{ issue.short_id }})\n{{ issue_url }}"}"#;

    const PRESET_TEAMS_ADAPTIVE_CARD: &str = r#"{"type":"message","attachments":[{"contentType":"application/vnd.microsoft.card.adaptive","content":{"type":"AdaptiveCard","version":"1.4","body":[{"type":"TextBlock","text":"{{ issue.title }}","weight":"Bolder","size":"Medium","wrap":true},{"type":"TextBlock","text":"{{ project.name }} · {{ issue.short_id }} · {{ issue.event_count }} events","isSubtle":true,"wrap":true}],"actions":[{"type":"Action.OpenUrl","title":"View issue","url":"{{ issue_url }}"}]}}]}"#;

    const PRESET_STRUCTURED: &str = r#"{"source":"rustrak","event":{{ alert_type }},"severity":{{ issue.level }},"title":{{ issue.title }},"url":{{ issue_url }},"occurrences":{{ issue.event_count }},"project":{{ project.slug }}}"#;

    // The dashboard's editor placeholder (message-template.ts
    // `templatePlaceholder`); mirrored so a copy that would teach users an
    // invalid template fails this test instead.
    const PLACEHOLDER_EXAMPLE: &str = r#"{"text":"Rustrak: {{ issue.title }}"}"#;

    #[test]
    fn test_placeholder_example_renders_valid_json() {
        let parsed: serde_json::Value = serde_json::from_slice(
            &CustomWebhookNotifier::render_body(PLACEHOLDER_EXAMPLE, &create_test_payload())
                .expect("placeholder example must render to valid JSON"),
        )
        .unwrap();
        assert_eq!(parsed["text"], "Rustrak: Test error");
    }

    #[test]
    fn test_every_preset_renders_with_a_quoted_title_and_no_level() {
        // The hostile payload: a title with a quote and a newline, no level.
        // Every preset has to come out as valid JSON with the title intact.
        let payload = AlertPayload {
            issue: IssueInfo {
                level: None,
                ..quoted_title_payload().issue
            },
            ..quoted_title_payload()
        };
        for (name, preset) in [
            ("wecom text", PRESET_WECOM_TEXT),
            ("wecom markdown", PRESET_WECOM_MARKDOWN),
            ("dingtalk text", PRESET_DINGTALK_TEXT),
            ("feishu text", PRESET_FEISHU_TEXT),
            ("discord", PRESET_DISCORD),
            ("slack compatible", PRESET_SLACK_COMPATIBLE),
            ("teams adaptive card", PRESET_TEAMS_ADAPTIVE_CARD),
            ("structured", PRESET_STRUCTURED),
        ] {
            let body = CustomWebhookNotifier::render_body(preset, &payload)
                .unwrap_or_else(|e| panic!("{name} preset must render: {e}"));
            let text = String::from_utf8(body).unwrap();
            assert!(
                text.contains(r#"He said \"boom\"\nthen left"#),
                "{name}: the title must survive escaping, got: {text}"
            );
            assert!(
                !text.contains("None"),
                "{name}: an absent level must not leak: {text}"
            );
        }
    }

    #[test]
    fn test_wecom_text_preset_renders_the_documented_shape() {
        let parsed: serde_json::Value = serde_json::from_slice(
            &CustomWebhookNotifier::render_body(PRESET_WECOM_TEXT, &create_test_payload())
                .expect("wecom text preset must render"),
        )
        .unwrap();
        assert_eq!(parsed["msgtype"], "text");
        assert_eq!(
            parsed["text"]["content"],
            "Rustrak: Test error (TEST-1)\nhttps://example.com/issues/abc-123"
        );
    }

    #[test]
    fn test_markdown_preset_carries_real_newlines_and_a_blank_for_no_level() {
        let payload = AlertPayload {
            issue: IssueInfo {
                level: None,
                ..create_test_payload().issue
            },
            ..create_test_payload()
        };
        let parsed: serde_json::Value = serde_json::from_slice(
            &CustomWebhookNotifier::render_body(PRESET_WECOM_MARKDOWN, &payload)
                .expect("markdown preset must render without a level"),
        )
        .unwrap();
        let content = parsed["markdown"]["content"].as_str().unwrap();
        assert!(
            content.starts_with("### Test error\n> Project: Test Project\n> Level: \n"),
            "got: {content:?}"
        );
    }

    #[test]
    fn test_feishu_preset_uses_its_own_key_names() {
        let parsed: serde_json::Value = serde_json::from_slice(
            &CustomWebhookNotifier::render_body(PRESET_FEISHU_TEXT, &create_test_payload())
                .expect("feishu text preset must render"),
        )
        .unwrap();
        assert_eq!(parsed["msg_type"], "text");
        assert!(parsed["content"]["text"]
            .as_str()
            .unwrap()
            .starts_with("Rustrak: Test error (TEST-1)"));
    }

    #[test]
    fn test_structured_preset_keeps_json_types() {
        let parsed: serde_json::Value = serde_json::from_slice(
            &CustomWebhookNotifier::render_body(PRESET_STRUCTURED, &create_test_payload())
                .expect("structured preset must render"),
        )
        .unwrap();
        assert_eq!(parsed["occurrences"], 1);
        assert_eq!(parsed["severity"], "error");
        assert_eq!(parsed["event"], "new_issue");
    }

    #[test]
    fn test_teams_preset_nests_the_title_three_levels_deep() {
        let parsed: serde_json::Value = serde_json::from_slice(
            &CustomWebhookNotifier::render_body(
                PRESET_TEAMS_ADAPTIVE_CARD,
                &quoted_title_payload(),
            )
            .expect("teams preset must render"),
        )
        .unwrap();
        assert_eq!(
            parsed["attachments"][0]["content"]["body"][0]["text"],
            "He said \"boom\"\nthen left"
        );
        assert_eq!(
            parsed["attachments"][0]["content"]["actions"][0]["url"],
            "https://example.com/issues/abc-123"
        );
    }

    #[test]
    fn test_preview_shows_the_body_that_would_be_sent() {
        // The dashboard cannot render minijinja, so it asks the server. The
        // answer has to come from the same code path as a delivery or the
        // preview is a second opinion nobody asked for.
        let rendered = CustomWebhookNotifier::preview_template(
            "{\n  \"text\": \"{{ issue.title }}\",\n  \"count\": {{ issue.event_count }}\n}",
        )
        .expect("a valid template previews");
        // Byte for byte what a delivery would carry: the writer's own layout.
        assert_eq!(
            rendered,
            "{\n  \"text\": \"Sample issue\",\n  \"count\": 1\n}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed["text"], "Sample issue");
        assert_eq!(parsed["count"], 1);
    }

    #[test]
    fn test_preview_reports_why_a_template_would_fail() {
        let err = CustomWebhookNotifier::preview_template(r#"{"a": "{{ issue.title }}"#)
            .expect_err("a body that is not JSON must report why");
        assert!(err.contains("valid JSON"), "got: {err}");
    }

    #[test]
    fn test_preview_applies_the_same_limits_as_delivery() {
        let err = CustomWebhookNotifier::preview_template(
            r#"{"a":"{% for i in range(1000) %}{% for j in range(1000) %}x{% endfor %}{% endfor %}"}"#,
        )
        .expect_err("the preview must not be a way around the render budget");
        assert!(err.contains("too complex"), "got: {err}");
    }

    #[test]
    fn test_validate_accepts_valid_config() {
        let notifier = CustomWebhookNotifier::new();
        let config = make_config(
            r#"{"msgtype":"text","text":{"content":"{{ issue.title }}"}}"#,
            Some("https://example.com/hook"),
        );
        notifier
            .validate_config(&config)
            .expect("valid config must pass");
    }

    #[test]
    fn test_validate_rejects_empty_template() {
        let notifier = CustomWebhookNotifier::new();
        let err = notifier
            .validate_config(&make_config("   ", Some("https://example.com/hook")))
            .expect_err("blank template must be rejected");
        assert!(err.to_string().contains("Template cannot be empty"));
    }

    #[test]
    fn test_validate_rejects_template_syntax_error() {
        let notifier = CustomWebhookNotifier::new();
        let err = notifier
            .validate_config(&make_config("{% for x in %}", None))
            .expect_err("syntax error must be caught at save time");
        assert!(err.to_string().contains("Invalid template syntax"));
    }

    #[test]
    fn test_validate_requires_template_field() {
        let notifier = CustomWebhookNotifier::new();
        let err = notifier
            .validate_config(&json!({"url": "https://example.com/hook"}))
            .expect_err("missing template field must be rejected");
        assert!(err.to_string().contains("Invalid custom webhook config"));
    }

    #[test]
    fn test_validate_rejects_a_template_that_cannot_render_json() {
        // Syntactically valid as a template, never valid JSON: a trailing
        // comma. Compiling does not catch it, so before this the integration
        // saved fine and only failed when a real incident fired.
        let notifier = CustomWebhookNotifier::new();
        let err = notifier
            .validate_config(&make_config(r#"{"a": "{{ issue.title }}",}"#, None))
            .expect_err("a template that renders invalid JSON must be rejected at save time");
        assert!(
            err.to_string().contains("valid JSON"),
            "the message must say what is wrong, got: {err}"
        );
    }

    #[test]
    fn test_validate_rejects_a_template_that_breaks_on_an_absent_optional_field() {
        // issue.level is optional. A template that only produces JSON when it
        // is present is broken for half the alerts, so validation renders an
        // issue without a level too.
        let notifier = CustomWebhookNotifier::new();
        let err = notifier
            .validate_config(&make_config(
                r#"{"level":"{% if issue.level %}{{ issue.level }}{% endif %}"{% if issue.level %},"has_level":true{% endif %}}"#,
                None,
            ))
            .map(|_| ());
        assert!(
            err.is_ok(),
            "this one is valid JSON either way and must pass: {err:?}"
        );
        let err = notifier
            .validate_config(&make_config(
                r#"{"level":{% if issue.level %}{{ issue.level | tojson }}{% endif %}}"#,
                None,
            ))
            .expect_err("a template invalid when level is absent must be rejected");
        assert!(err.to_string().contains("valid JSON"), "got: {err}");
    }

    #[test]
    fn test_validate_rejects_a_template_past_the_render_limits() {
        let notifier = CustomWebhookNotifier::new();
        let err = notifier
            .validate_config(&make_config(
                r#"{"a":"{% for i in range(1000) %}{% for j in range(1000) %}x{% endfor %}{% endfor %}"}"#,
                None,
            ))
            .expect_err("the save-time check must apply the same limits as delivery");
        assert!(err.to_string().contains("too complex"), "got: {err}");
    }

    #[test]
    fn test_validate_rejects_non_http_url() {
        let notifier = CustomWebhookNotifier::new();
        let err = notifier
            .validate_config(&make_config("{}", Some("ftp://example.com/hook")))
            .expect_err("ftp url must be rejected");
        assert!(err.to_string().contains("HTTP or HTTPS"));
    }

    #[test]
    fn test_validate_allows_missing_url_for_routing_override() {
        let notifier = CustomWebhookNotifier::new();
        notifier
            .validate_config(&make_config(r#"{"a":1}"#, None))
            .expect("url is optional like the plain webhook");
    }

    #[test]
    fn test_validate_rejects_invalid_headers() {
        let notifier = CustomWebhookNotifier::new();
        let err = notifier
            .validate_config(&json!({
                "url": "https://example.com/hook",
                "headers": {"Authorization": "Bearer x\nX-Injected: 1"},
                "template": r#"{"a":1}"#,
            }))
            .expect_err("header injection must be rejected");
        assert!(err.to_string().contains("Authorization"), "got: {err}");
    }

    #[test]
    fn test_validate_accepts_authorization_header() {
        let notifier = CustomWebhookNotifier::new();
        notifier
            .validate_config(&json!({
                "url": "https://example.com/hook",
                "headers": {"Authorization": "Bearer re_123"},
                "template": r#"{"a":1}"#,
            }))
            .expect("an Authorization header is the point of #419");
    }

    // -------------------------------------------------------------------------
    // Dispatch guards
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_missing_url_fails_before_render() {
        let notifier = CustomWebhookNotifier::new();
        let integration = make_integration(make_config("{% invalid", None), true);
        let result = notifier
            .send(&integration, &json!({}), &create_test_payload())
            .await;
        assert!(!result.success);
        assert!(result
            .error_message
            .unwrap_or_default()
            .contains("URL not configured"));
    }

    #[tokio::test]
    async fn test_render_failure_is_reported_not_sent() {
        let notifier = CustomWebhookNotifier::new();
        let integration = make_integration(
            make_config(
                r#"{"broken": "{{ nope }}"}"#,
                Some("https://127.0.0.1:1/hook"),
            ),
            true,
        );
        let result = notifier
            .send(&integration, &json!({}), &create_test_payload())
            .await;
        assert!(!result.success);
        assert!(result
            .error_message
            .unwrap_or_default()
            .contains("undefined"));
    }

    #[tokio::test]
    async fn test_delivers_rendered_body_over_http() {
        // Full network path: a listener that reads the request, replies 200,
        // and hands the raw bytes back for assertion.
        // A local listener stands in for the endpoint.
        super::super::allow_private_destinations(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind delivery listener");
        let addr = listener.local_addr().expect("listener addr");

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            // Read until the rendered marker has arrived (well under the
            // request head's own size, so one or two reads suffice).
            while !String::from_utf8_lossy(&buf).contains("Rustrak: Test error") {
                let n = socket.read(&mut chunk).await.expect("read request");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
            let _ = tx.send(String::from_utf8_lossy(&buf).to_string());
        });

        let notifier = CustomWebhookNotifier::new();
        let credentials = json!({
            "url": format!("http://{addr}/hook"),
            "secret": "s3cret",
            // Stored without validation (pre-#419 rows, or written straight
            // to the database): a forged signature must still lose.
            "headers": {
                "X-Custom": "cred-level",
                "Authorization": "Bearer cred-token",
                "X-Rustrak-Signature": "sha256=forged",
            },
            "template": PRESET_DINGTALK_TEXT,
        });
        let integration = make_integration(credentials, true);
        let routing = json!({"extra_headers": {
            "X-Extra": "route-level",
            "authorization": "Bearer route-token",
        }});

        let result = notifier
            .send(&integration, &routing, &create_test_payload())
            .await;
        assert!(
            result.success,
            "delivery must succeed: {:?}",
            result.error_message
        );
        assert_eq!(result.http_status, Some(200));

        let raw = rx.await.expect("listener captured a request");
        // Rendered JSON body, not the raw AlertPayload.
        assert!(raw.contains(r#""msgtype":"text""#), "got: {raw}");
        assert!(raw.contains("Rustrak: Test error (TEST-1)"), "got: {raw}");
        assert!(!raw.contains("alert_type"), "raw payload must not be sent");
        // Headers: HMAC signature, both custom levels, standard metadata.
        let lower = raw.to_ascii_lowercase();
        assert!(lower.contains("x-rustrak-signature: sha256="), "got: {raw}");
        assert!(lower.contains("x-custom: cred-level"), "got: {raw}");
        assert!(lower.contains("x-extra: route-level"), "got: {raw}");
        // Routing replaces the credential value instead of sending both.
        assert!(
            lower.contains("authorization: bearer route-token"),
            "got: {raw}"
        );
        assert!(!lower.contains("cred-token"), "got: {raw}");
        assert_eq!(
            lower.matches("x-rustrak-signature:").count(),
            1,
            "got: {raw}"
        );
        assert!(!lower.contains("forged"), "got: {raw}");
        assert!(
            lower.contains("x-rustrak-request-id: test-123"),
            "got: {raw}"
        );
    }

    /// Serves a fixed raw HTTP response (after reading one request chunk) on
    /// a random localhost port; returns the URL. The responder task owns the
    /// bytes, so callers can build responses at runtime.
    async fn raw_responder(response: Vec<u8>) -> String {
        // A local listener stands in for the endpoint.
        super::super::allow_private_destinations(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind responder listener");
        let addr = listener.local_addr().expect("listener addr");
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut sink = [0u8; 1024];
            let _ = socket.read(&mut sink).await;
            let _ = socket.write_all(&response).await;
            let _ = socket.flush().await;
        });
        format!("http://{addr}/hook")
    }

    fn custom_integration(url: String) -> AlertIntegration {
        make_integration(
            json!({
                "url": url,
                "template": r#"{"msgtype":"text","text":{"content":"hi"}}"#,
            }),
            true,
        )
    }

    #[tokio::test]
    async fn test_a_2xx_is_a_delivery_whatever_the_body_says() {
        // A generic webhook judges by the HTTP status, the way Grafana,
        // Alertmanager and Uptime Kuma do. Fields an endpoint happens to use
        // for its own business status are its business, not a verdict.
        let body = r#"{"code":1,"msg":"ok"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let url = raw_responder(response.into_bytes()).await;
        let notifier = CustomWebhookNotifier::new();
        let result = notifier
            .send(&custom_integration(url), &json!({}), &create_test_payload())
            .await;
        assert!(
            result.success,
            "a generic 2xx must stay delivered: {:?}",
            result.error_message
        );
    }

    #[tokio::test]
    async fn test_a_delivery_reports_what_the_endpoint_answered() {
        // Rustrak does not interpret the reply, so it has to show it. A bot
        // that answers 200 with `{"errcode":93000}` has refused the message,
        // and the only way the reader can find that out is by seeing it.
        let body = r#"{"errcode":93000,"errmsg":"invalid webhook url"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let url = raw_responder(response.into_bytes()).await;
        let notifier = CustomWebhookNotifier::new();
        let result = notifier
            .send(&custom_integration(url), &json!({}), &create_test_payload())
            .await;
        assert!(result.success, "a 2xx is still a delivery");
        assert_eq!(result.response_body.as_deref(), Some(body));
    }

    #[tokio::test]
    async fn test_an_empty_answer_reports_no_body_rather_than_an_empty_one() {
        let url =
            raw_responder(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n".to_vec()).await;
        let notifier = CustomWebhookNotifier::new();
        let result = notifier
            .send(&custom_integration(url), &json!({}), &create_test_payload())
            .await;
        assert!(result.success);
        assert_eq!(result.response_body, None);
    }

    #[tokio::test]
    async fn test_oversized_response_body_is_a_failure() {
        let big = vec![b'x'; super::super::MAX_RESPONSE_BODY_BYTES + 4096];
        let head = format!(
            "HTTP/1.1 200 OK
content-type: text/plain
content-length: {}
connection: close

",
            big.len()
        );
        let mut response = head.into_bytes();
        response.extend_from_slice(&big);
        let url = raw_responder(response.to_vec()).await;
        let notifier = CustomWebhookNotifier::new();
        let result = notifier
            .send(&custom_integration(url), &json!({}), &create_test_payload())
            .await;
        assert!(
            !result.success,
            "a body past the limit must fail rather than buffer"
        );
        assert!(result
            .error_message
            .unwrap_or_default()
            .contains("failed to read response body"));
    }

    #[tokio::test]
    async fn test_truncated_response_body_is_a_failure() {
        // Declares 200 bytes, sends 10, then closes: the capped read errors,
        // and a 2xx whose body cannot be inspected must not read as delivered.
        let response = b"HTTP/1.1 200 OK
content-length: 200
connection: close

truncated!";
        let url = raw_responder(response.to_vec()).await;
        let notifier = CustomWebhookNotifier::new();
        let result = notifier
            .send(&custom_integration(url), &json!({}), &create_test_payload())
            .await;
        assert!(!result.success, "an unreadable 200 body is a failure");
        assert_eq!(result.http_status, Some(200));
        assert!(result
            .error_message
            .unwrap_or_default()
            .contains("failed to read response body"));
    }
}
