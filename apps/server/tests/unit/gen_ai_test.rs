//! Unit tests for gen_ai.* span attribute normalization.
//!
//! Pure logic, no DB — mirrors Relay's normalize_ai() / infer_ai_operation_type().

use rustrak::services::gen_ai::{
    extract_gen_ai_columns, infer_operation_type, is_ai_span, normalize_gen_ai_attributes,
};
use serde_json::json;

#[test]
fn test_is_ai_span_true_when_operation_type_present() {
    let data = json!({"gen_ai.operation.type": "agent"});
    assert!(is_ai_span(&data, None));
}

#[test]
fn test_is_ai_span_true_when_operation_name_present() {
    let data = json!({"gen_ai.operation.name": "chat"});
    assert!(is_ai_span(&data, None));
}

#[test]
fn test_is_ai_span_true_when_op_starts_with_gen_ai_prefix() {
    let data = json!({});
    assert!(is_ai_span(&data, Some("gen_ai.chat")));
}

#[test]
fn test_is_ai_span_true_when_op_starts_with_legacy_ai_prefix() {
    let data = json!({});
    assert!(is_ai_span(&data, Some("ai.streamText")));
}

#[test]
fn test_is_ai_span_false_for_unrelated_span() {
    let data = json!({});
    assert!(!is_ai_span(&data, Some("http.client")));
}

#[test]
fn test_is_ai_span_false_for_op_containing_ai_not_as_prefix() {
    // "ai." appears, but not at the start — must not false-positive.
    let data = json!({});
    assert!(!is_ai_span(&data, Some("custom.ai.operation")));
}

#[test]
fn test_infer_operation_type_full_matches_to_agent() {
    for op in [
        "ai.run.generateText",
        "ai.run.generateObject",
        "gen_ai.invoke_agent",
        "ai.pipeline.generate_text",
        "ai.pipeline.generate_object",
        "ai.pipeline.stream_text",
        "ai.pipeline.stream_object",
        "gen_ai.create_agent",
        "invoke_agent",
        "create_agent",
    ] {
        assert_eq!(infer_operation_type(op), Some("agent"), "op={op}");
    }
}

#[test]
fn test_infer_operation_type_full_matches_to_tool() {
    for op in ["gen_ai.execute_tool", "execute_tool"] {
        assert_eq!(infer_operation_type(op), Some("tool"), "op={op}");
    }
}

#[test]
fn test_infer_operation_type_full_matches_to_handoff() {
    for op in ["gen_ai.handoff", "handoff"] {
        assert_eq!(infer_operation_type(op), Some("handoff"), "op={op}");
    }
}

#[test]
fn test_infer_operation_type_full_matches_to_other() {
    for op in ["ai.processor", "processor_run"] {
        assert_eq!(infer_operation_type(op), Some("other"), "op={op}");
    }
}

#[test]
fn test_infer_operation_type_prefix_matches() {
    assert_eq!(
        infer_operation_type("ai.streamText.doStream"),
        Some("ai_client")
    );
    assert_eq!(infer_operation_type("ai.streamText.foo"), Some("agent"));
    assert_eq!(infer_operation_type("ai.streamText"), Some("agent"));

    assert_eq!(
        infer_operation_type("ai.generateText.doGenerate"),
        Some("ai_client")
    );
    assert_eq!(infer_operation_type("ai.generateText.foo"), Some("agent"));

    assert_eq!(
        infer_operation_type("ai.generateObject.doGenerate"),
        Some("ai_client")
    );
    assert_eq!(infer_operation_type("ai.generateObject.foo"), Some("agent"));

    assert_eq!(infer_operation_type("ai.toolCall.foo"), Some("tool"));
}

#[test]
fn test_infer_operation_type_no_match_returns_none() {
    assert_eq!(infer_operation_type("http.client"), None);
    assert_eq!(infer_operation_type("gen_ai.chat.completions"), None);
}

#[test]
fn test_normalize_defaults_response_model_from_request_model() {
    let mut data = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.request.model": "gpt-4"
    });
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.response.model"], "gpt-4");
}

#[test]
fn test_normalize_does_not_overwrite_existing_response_model() {
    let mut data = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.request.model": "gpt-4",
        "gen_ai.response.model": "gpt-4-turbo-2024"
    });
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.response.model"], "gpt-4-turbo-2024");
}

#[test]
fn test_normalize_infers_operation_type_from_operation_name() {
    let mut data = json!({"gen_ai.operation.name": "invoke_agent"});
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.operation.type"], "agent");
}

#[test]
fn test_normalize_infers_operation_type_from_span_op() {
    let mut data = json!({});
    normalize_gen_ai_attributes(&mut data, Some("gen_ai.invoke_agent"));
    assert_eq!(data["gen_ai.operation.type"], "agent");
}

#[test]
fn test_normalize_defaults_operation_type_to_ai_client_when_no_match() {
    let mut data = json!({"gen_ai.operation.name": "embeddings"});
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.operation.type"], "ai_client");
}

#[test]
fn test_normalize_overwrites_existing_operation_type() {
    // operation.type is Sentry's derived field: Relay recomputes it from
    // operation.name even when the SDK already supplied a (disagreeing) one.
    let mut data = json!({
        "gen_ai.operation.type": "tool",
        "gen_ai.operation.name": "invoke_agent"
    });
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.operation.type"], "agent");
}

#[test]
fn test_normalize_keeps_operation_type_when_nothing_to_infer_from() {
    // No operation.name and no AI op: Relay's span pipeline wouldn't treat
    // this as an AI span at all, so the existing type must survive rather
    // than be clobbered with the ai_client default.
    let mut data = json!({"gen_ai.operation.type": "tool"});
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.operation.type"], "tool");

    let mut data = json!({"gen_ai.operation.type": "agent"});
    normalize_gen_ai_attributes(&mut data, Some("db.query"));
    assert_eq!(data["gen_ai.operation.type"], "agent");
}

#[test]
fn test_normalize_defaults_agent_name_from_function_id() {
    let mut data = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.function_id": "my-agent"
    });
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.agent.name"], "my-agent");
}

#[test]
fn test_normalize_does_not_overwrite_existing_agent_name() {
    let mut data = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.function_id": "fallback-name",
        "gen_ai.agent.name": "real-agent"
    });
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.agent.name"], "real-agent");
}

#[test]
fn test_normalize_computes_total_tokens_from_input_and_output() {
    let mut data = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.usage.input_tokens": 100,
        "gen_ai.usage.output_tokens": 50
    });
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.usage.total_tokens"], 150.0);
}

#[test]
fn test_normalize_does_not_overwrite_existing_total_tokens() {
    let mut data = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.usage.input_tokens": 100,
        "gen_ai.usage.output_tokens": 50,
        "gen_ai.usage.total_tokens": 999
    });
    normalize_gen_ai_attributes(&mut data, None);
    assert_eq!(data["gen_ai.usage.total_tokens"], 999);
}

#[test]
fn test_normalize_total_tokens_absent_when_neither_input_nor_output_present() {
    let mut data = json!({"gen_ai.operation.type": "ai_client"});
    normalize_gen_ai_attributes(&mut data, None);
    assert!(data.get("gen_ai.usage.total_tokens").is_none());
}

#[test]
fn test_normalize_is_noop_for_non_ai_span() {
    let mut data = json!({"foo": "bar"});
    normalize_gen_ai_attributes(&mut data, Some("http.client"));
    assert_eq!(data, json!({"foo": "bar"}));
}

#[test]
fn test_normalize_is_noop_when_data_is_not_an_object() {
    // An AI `op` alone passes is_ai_span, so a payload whose attributes bag
    // isn't an object still reaches normalization — it must not panic.
    for mut data in [json!("oops"), json!(42), json!([1, 2]), json!(true)] {
        normalize_gen_ai_attributes(&mut data, Some("gen_ai.invoke_agent"));
    }
}

// ── Cached / reasoning token extraction ─────────────────────────────────────

#[test]
fn test_extract_reads_cached_input_tokens_under_the_current_name() {
    // Relay emits `gen_ai.usage.cache_read.input_tokens` at c455da18 — the
    // `.cached` suffix spelling is gone from its normalization.
    let mut data = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.usage.input_tokens": 1000,
        "gen_ai.usage.cache_read.input_tokens": 400,
        "gen_ai.usage.output_tokens": 200
    });

    let columns = extract_gen_ai_columns(&mut data, None);

    assert_eq!(columns.usage_cached_input_tokens, Some(400.0));
}

#[test]
fn test_extract_falls_back_to_the_retired_cached_spelling() {
    // Spans ingested before the rename still carry the old attribute, and
    // Sentry's own frontend reads both. So does Rustrak.
    let mut data = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.usage.input_tokens": 1000,
        "gen_ai.usage.input_tokens.cached": 400
    });

    let columns = extract_gen_ai_columns(&mut data, None);

    assert_eq!(columns.usage_cached_input_tokens, Some(400.0));
}

#[test]
fn test_extract_reads_reasoning_output_tokens_under_both_spellings() {
    let mut current = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.usage.reasoning.output_tokens": 320
    });
    let mut retired = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.usage.output_tokens.reasoning": 320
    });

    assert_eq!(
        extract_gen_ai_columns(&mut current, None).usage_reasoning_output_tokens,
        Some(320.0)
    );
    assert_eq!(
        extract_gen_ai_columns(&mut retired, None).usage_reasoning_output_tokens,
        Some(320.0)
    );
}

#[test]
fn test_extract_leaves_cached_and_reasoning_none_when_absent() {
    // A model that reports neither must store NULL, not 0: the Token Types
    // widget has to tell "no prompt caching" from "caching reported zero".
    let mut data = json!({
        "gen_ai.operation.type": "ai_client",
        "gen_ai.usage.input_tokens": 100
    });

    let columns = extract_gen_ai_columns(&mut data, None);

    assert_eq!(columns.usage_cached_input_tokens, None);
    assert_eq!(columns.usage_reasoning_output_tokens, None);
}

#[test]
fn test_extract_cuts_names_to_255_bytes_on_a_char_boundary() {
    // 255 four-byte chars would be 1020 bytes per column; four of them in one
    // Postgres B-tree entry pass its ~2704-byte limit and the insert fails.
    let wide = "🦀".repeat(300);
    let mut data = json!({
        "gen_ai.operation.type": "agent",
        "gen_ai.agent.name": wide,
        "gen_ai.request.model": "gpt-4o",
    });
    let cols = extract_gen_ai_columns(&mut data, None);
    let agent = cols.agent_name.unwrap();
    assert_eq!(agent.len(), 252);
    assert!(agent.chars().all(|c| c == '🦀'));
    assert_eq!(cols.request_model.as_deref(), Some("gpt-4o"));
}

#[test]
fn test_extract_keeps_conversation_ids_to_255_chars() {
    // An identity, not a label: cutting it shorter than before would split an
    // existing conversation from its next turns. Its index holds no other
    // wide value, so it stays within the B-tree limit at 255 chars.
    let id = "会".repeat(100);
    let mut data = json!({
        "gen_ai.operation.type": "agent",
        "gen_ai.conversation.id": id,
    });
    let cols = extract_gen_ai_columns(&mut data, None);
    assert_eq!(cols.conversation_id.as_deref(), Some(id.as_str()));
}
