use super::ProcessorCtx;
use crate::error::AppResult;
use crate::models::log::{LogContainer, LogItem};
use crate::protocol::{clamp, clamp_opt};
use bytes::Bytes;
use sha1::{Digest as _, Sha1};
use uuid::Uuid;

/// Processor for standalone logs (Sentry "log" item type).
///
/// Expands the item container (`{"items":[OurLog, ...]}`) into individual rows
/// in the dedicated `logs` table. Mirrors Relay's `LogsProcessor`, minus the
/// Relay-only concerns (dynamic sampling, PII scrubbing, metric extraction) —
/// Rustrak is the terminal store, not a forwarding Relay.
pub struct LogsProcessor;

impl LogsProcessor {
    pub async fn process(&self, work: Bytes, ctx: &ProcessorCtx) -> AppResult<()> {
        // Reject a malformed container instead of silently dropping the batch.
        let logs = LogContainer::parse(&work)?;

        // Batch the whole container in one DB transaction: a partially-stored
        // batch is harder to reason about than an all-or-nothing one, and the
        // SDK will resend on failure.
        let mut tx = ctx.pool.begin().await?;
        let container_hash = hex::encode(Sha1::digest(&work));

        for (index, log) in logs.into_iter().enumerate() {
            let id = Uuid::new_v4();
            let dedupe_key = dedupe_key(&container_hash, index, ctx.event_id);
            let timestamp = log.timestamp.unwrap_or(ctx.ingested_at);
            let level = normalize_level(&log.level);
            let severity_number = severity_number(&level);

            sqlx::query(
                r#"
                INSERT INTO logs (
                    id, project_id, dedupe_key,
                    trace_id, span_id,
                    level, severity_number, body, attributes,
                    timestamp, ingested_at
                ) VALUES (
                    $1, $2, $3,
                    $4, $5,
                    $6, $7, $8, $9,
                    $10, $11
                )
                ON CONFLICT (project_id, dedupe_key)
                    WHERE dedupe_key IS NOT NULL DO NOTHING
                "#,
            )
            .bind(id)
            .bind(ctx.project_id)
            .bind(dedupe_key)
            .bind(clamp_opt(empty_to_none(&log.trace_id), 64))
            .bind(clamp_opt(log.span_id.clone(), 32))
            .bind(&level)
            .bind(severity_number)
            .bind(&log.body)
            .bind(attributes_json(&log))
            .bind(timestamp)
            .bind(ctx.ingested_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

fn dedupe_key(container_hash: &str, index: usize, delivery_id: Uuid) -> String {
    format!("{container_hash}:{delivery_id}:{index}")
}

/// Normalizes the log level, defaulting to "info" when the SDK omits it.
/// Unknown values are kept (forward-compat with Relay's `OurLogLevel`), cut
/// to the VARCHAR(20) column, which Postgres enforces.
fn normalize_level(level: &str) -> String {
    if level.is_empty() {
        "info".to_string()
    } else {
        clamp(level, 20)
    }
}

/// Maps a level to its OTel severity number for ordering/filtering.
/// Unknown levels get no number (NULL).
fn severity_number(level: &str) -> Option<i16> {
    match level {
        "trace" => Some(1),
        "debug" => Some(5),
        "info" => Some(9),
        "warn" => Some(13),
        "error" => Some(17),
        "fatal" => Some(21),
        _ => None,
    }
}

/// The OTel attribute map, stored verbatim. Only JSON objects are kept; a
/// non-conforming SDK that sends an array or primitive is coerced to an empty
/// object so the column always holds the `{key: value}` shape the read API and
/// client schema expect (otherwise one bad row would break the Logs page).
fn attributes_json(log: &LogItem) -> serde_json::Value {
    if log.attributes.is_object() {
        log.attributes.clone()
    } else {
        serde_json::json!({})
    }
}

fn empty_to_none(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::dedupe_key;
    use uuid::Uuid;

    #[test]
    fn log_dedupe_key_distinguishes_deliveries() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();

        assert_eq!(
            dedupe_key("container", 0, first),
            dedupe_key("container", 0, first)
        );
        assert_ne!(
            dedupe_key("container", 0, first),
            dedupe_key("container", 0, second)
        );
    }
}
