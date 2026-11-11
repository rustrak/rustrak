//! User feedback inside the error digest: an event of `type: "feedback"`
//! whose `contexts.feedback` holds what the user wrote.
//!
//! Relay passes the context through untouched; the checks here are Sentry's
//! (`feedback/usecases/ingest/create_feedback.py`, `should_filter_feedback`).

use crate::error::{AppError, AppResult};
use serde_json::Value;
use uuid::Uuid;

/// Sentry's `feedback.message.max-size`, in characters. Longer is rejected,
/// not truncated.
const MAX_MESSAGE_CHARS: usize = 4096;

/// The issue type and category a feedback issue carries.
pub(crate) const CATEGORY: &str = "feedback";

/// Where ingest puts an event's attachments in the stored event, until the
/// digest moves them to `event_attachments`. Never stored in `events.data`.
pub(crate) const ATTACHMENTS_KEY: &str = "_rustrak_attachments";

/// An attachment on its way to `event_attachments`.
#[derive(Debug, PartialEq)]
pub(crate) struct NewAttachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

/// Removes the attachments ingest stored in `event`, whatever the event is,
/// and returns them. One that does not decode is skipped.
pub(crate) fn take_attachments(event: &mut Value) -> Vec<NewAttachment> {
    use base64::Engine as _;
    let Some(Value::Array(items)) = event
        .as_object_mut()
        .and_then(|o| o.remove(ATTACHMENTS_KEY))
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let text = |key: &str| item.get(key).and_then(Value::as_str);
            Some(NewAttachment {
                // Cut to the VARCHAR(255) columns, which Postgres enforces.
                filename: crate::protocol::clamp(text("filename")?, 255),
                content_type: crate::protocol::clamp(text("content_type")?, 255),
                data: base64::engine::general_purpose::STANDARD
                    .decode(text("data")?)
                    .ok()?,
            })
        })
        .collect()
}

/// What a user submitted.
#[derive(Debug, PartialEq)]
pub(crate) struct Feedback {
    pub message: String,
    pub name: String,
    pub contact_email: String,
    /// The error event the user was reporting on, if any.
    pub associated_event_id: Option<Uuid>,
}

impl Feedback {
    /// The feedback an event carries: `None` for any other event, an error
    /// for a feedback Sentry would drop.
    pub(crate) fn from_event(event: &Value) -> AppResult<Option<Self>> {
        if event.get("type").and_then(Value::as_str) != Some("feedback") {
            return Ok(None);
        }
        let invalid = |why: &str| AppError::Validation(format!("feedback dropped: {why}"));
        let context = event
            .pointer("/contexts/feedback")
            .ok_or_else(|| invalid("no feedback context"))?;
        let text = |key: &str| context.get(key).and_then(Value::as_str).unwrap_or("");

        let message = text("message");
        if message.trim().is_empty() {
            return Err(invalid("empty message"));
        }
        if message.chars().count() > MAX_MESSAGE_CHARS {
            return Err(invalid("message too long"));
        }
        let associated_event_id = match context.get("associated_event_id") {
            None | Some(Value::Null) => None,
            Some(id) => Some(
                id.as_str()
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .ok_or_else(|| invalid("invalid associated_event_id"))?,
            ),
        };
        Ok(Some(Self {
            message: message.to_string(),
            name: text("name").to_string(),
            contact_email: text("contact_email").to_string(),
            associated_event_id,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(feedback: Value) -> Value {
        json!({ "type": "feedback", "contexts": { "feedback": feedback } })
    }

    #[test]
    fn other_events_carry_no_feedback() {
        assert_eq!(
            Feedback::from_event(&json!({ "message": "x" })).unwrap(),
            None
        );
    }

    #[test]
    fn reads_every_field() {
        let id = Uuid::new_v4();
        let feedback = Feedback::from_event(&event(json!({
            "message": "Broken",
            "name": "Jane",
            "contact_email": "jane@example.com",
            "associated_event_id": id.simple().to_string(),
        })))
        .unwrap()
        .unwrap();
        assert_eq!(
            feedback,
            Feedback {
                message: "Broken".into(),
                name: "Jane".into(),
                contact_email: "jane@example.com".into(),
                associated_event_id: Some(id),
            }
        );
    }

    #[test]
    fn rejects_what_sentry_filters() {
        for bad in [
            json!({ "type": "feedback" }),
            event(json!({})),
            event(json!({ "message": " \n " })),
            event(json!({ "message": "x".repeat(4097) })),
            event(json!({ "message": "ok", "associated_event_id": "nope" })),
            event(json!({ "message": "ok", "associated_event_id": 7 })),
        ] {
            assert!(Feedback::from_event(&bad).is_err(), "{bad} must be dropped");
        }
    }

    #[test]
    fn attachments_are_taken_out_of_the_event() {
        let mut event = json!({
            "type": "feedback",
            ATTACHMENTS_KEY: [
                { "filename": "a.png", "content_type": "image/png", "data": "AAEC" },
                { "filename": "bad", "content_type": "x", "data": "not base64!" },
            ],
        });
        let taken = take_attachments(&mut event);
        assert_eq!(
            taken,
            [NewAttachment {
                filename: "a.png".into(),
                content_type: "image/png".into(),
                data: vec![0, 1, 2],
            }]
        );
        assert!(event.get(ATTACHMENTS_KEY).is_none());
    }

    #[test]
    fn the_limit_counts_characters() {
        let at_limit = event(json!({ "message": "é".repeat(4096) }));
        assert!(Feedback::from_event(&at_limit).unwrap().is_some());
    }
}
