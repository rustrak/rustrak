use actix_web::body::MessageBody;
use actix_web::http::header::CONTENT_TYPE;
use actix_web::{web, HttpResponse};
use base64::Engine as _;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars::{self, JsonSchema};
use rmcp::{tool, tool_router};
use serde::Deserialize;

use super::issues::OnIssue;
use super::{from_error, refuse, reply, uuid, RustrakMcp};
use crate::routes::events;

#[derive(Deserialize, JsonSchema)]
pub struct AttachmentArgs {
    /// Project ID
    project_id: i32,
    /// Attachment ID, as list_feedback lists it
    attachment_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct EventArgs {
    /// Project ID
    project_id: i32,
    /// Issue ID
    issue_id: String,
    /// Event ID
    event_id: String,
}

#[tool_router(router = event_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "List raw events for a specific issue. Events are individual error occurrences within a grouped issue.",
        annotations(read_only_hint = true)
    )]
    async fn list_events(
        &self,
        Parameters(a): Parameters<OnIssue<events::ListEventsQuery>>,
    ) -> CallToolResult {
        reply(async {
            events::list_events(
                self.pool(),
                a.issue.path()?,
                web::Query(a.rest),
                self.actor(),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "Get full detail for a single event, including the complete Sentry envelope data.",
        annotations(read_only_hint = true)
    )]
    async fn get_event(&self, Parameters(a): Parameters<EventArgs>) -> CallToolResult {
        reply(async {
            let path = web::Path::from((
                a.project_id,
                uuid("issue_id", &a.issue_id)?,
                uuid("event_id", &a.event_id)?,
            ));
            events::get_event(self.pool(), path, self.actor()).await
        })
        .await
    }

    #[tool(
        description = "Get a file sent with an event, such as the screenshot of a user feedback. A PNG, JPEG, GIF or WebP image comes back as an image; any other file is only described.",
        annotations(read_only_hint = true)
    )]
    async fn get_attachment(&self, Parameters(a): Parameters<AttachmentArgs>) -> CallToolResult {
        let path = match uuid("attachment_id", &a.attachment_id) {
            Ok(id) => web::Path::from((a.project_id, id)),
            Err(e) => return from_error(e),
        };
        match events::download_attachment(self.pool(), path, self.actor()).await {
            Ok(response) => attachment_content(response),
            Err(e) => from_error(e),
        }
    }
}

/// The handler's file as tool content. The handler serves a file as an image
/// only when its bytes are one, so its `Content-Type` decides; anything else
/// is described rather than handed to the model as bytes.
fn attachment_content(response: HttpResponse) -> CallToolResult {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let Ok(bytes) = response.into_body().try_into_bytes() else {
        return refuse("The attachment could not be read.");
    };
    let content = if content_type.starts_with("image/") {
        ContentBlock::image(
            base64::engine::general_purpose::STANDARD.encode(&bytes),
            content_type,
        )
    } else {
        ContentBlock::text(format!(
            "The attachment is not an image ({content_type}, {} bytes), so it is not returned.",
            bytes.len()
        ))
    };
    CallToolResult::success(vec![content])
}
