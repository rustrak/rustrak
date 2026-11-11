use async_trait::async_trait;
use bytes::Bytes;
use rustrak::error::AppResult;
use rustrak::services::sourcemap::{
    normalize_sentry_position, rewrite_frames, SourceMapEntry, SourceMapProvider,
};
use serde_json::json;

// ---------------------------------------------------------------------------
// FakeSourceMapProvider — returns a fixed source map regardless of debug_id
// ---------------------------------------------------------------------------

struct FakeSourceMapProvider {
    data: Option<Bytes>,
}

impl FakeSourceMapProvider {
    fn with_data(data: Bytes) -> Self {
        Self { data: Some(data) }
    }

    fn empty() -> Self {
        Self { data: None }
    }
}

#[async_trait]
impl SourceMapProvider for FakeSourceMapProvider {
    async fn fetch_sourcemap(
        &self,
        _project_id: i32,
        _debug_id: &str,
        _file_type: &str,
    ) -> AppResult<Option<SourceMapEntry>> {
        Ok(self
            .data
            .as_ref()
            .map(|d| SourceMapEntry { data: d.clone() }))
    }
}

// ---------------------------------------------------------------------------
// Source map fixtures
// ---------------------------------------------------------------------------

/// Simple 1-source, 5-line source map.
///
/// Generated lines 0-4 map to "src/app/page.tsx" lines 0-4 respectively.
/// sourcesContent: "line 1\nline 2\nline 3\nline 4\nline 5"
fn make_simple_sourcemap() -> Bytes {
    let json = r#"{
        "version": 3,
        "sources": ["src/app/page.tsx"],
        "sourcesContent": ["line 1\nline 2\nline 3\nline 4\nline 5"],
        "mappings": "AAAA;AACA;AACA;AACA;AACA"
    }"#;
    Bytes::from(json)
}

/// Source map with 20 sources, where generated line 4 → source index 15.
///
/// VLQ ";;;;AeAA":
///   4 semicolons skip generated lines 0-3,
///   "AeAA" = [col_delta=0, src_idx_delta=15, src_line_delta=0, src_col_delta=0]
///
/// sourcesContent[0]  = "wrong content at source 0\n..."
/// sourcesContent[15] = "correct line from source 15\nother line"
/// All other entries  = null
fn make_multi_source_sourcemap() -> Bytes {
    let mut sources = Vec::new();
    let mut contents = Vec::new();
    for i in 0..20usize {
        sources.push(format!("\"src/{i}.ts\""));
        match i {
            0 => contents.push("\"wrong content at source 0\\nline 2\"".to_string()),
            15 => contents.push("\"correct line from source 15\\nother line\"".to_string()),
            _ => contents.push("null".to_string()),
        }
    }
    let json = format!(
        r#"{{"version":3,"sources":[{}],"sourcesContent":[{}],"mappings":";;;;AeAA"}}"#,
        sources.join(","),
        contents.join(",")
    );
    Bytes::from(json)
}

/// Source map with null sourcesContent — get_source_contents returns None.
fn make_null_contents_sourcemap() -> Bytes {
    let json = r#"{
        "version": 3,
        "sources": ["src/app/page.tsx"],
        "sourcesContent": [null],
        "mappings": "AAAA;AACA;AACA;AACA;AACA"
    }"#;
    Bytes::from(json)
}

/// Source map mapping generated line 0 to source line 0 — tests pre_context at file start.
fn make_line_zero_sourcemap() -> Bytes {
    let json = r#"{
        "version": 3,
        "sources": ["src/app/page.tsx"],
        "sourcesContent": ["const x = 1;\nconst y = 2;"],
        "mappings": "AAAA"
    }"#;
    Bytes::from(json)
}

/// Builds a minimal event JSON with debug_meta + one frame.
fn make_event(code_file: &str, debug_id: &str, lineno: u64, colno: u64) -> serde_json::Value {
    json!({
        "debug_meta": {
            "images": [{"code_file": code_file, "debug_id": debug_id}]
        },
        "exception": {
            "values": [{
                "stacktrace": {
                    "frames": [{
                        "filename": code_file,
                        "lineno": lineno,
                        "colno": colno
                    }]
                }
            }]
        }
    })
}

// ---------------------------------------------------------------------------
// normalize_sentry_position tests
// ---------------------------------------------------------------------------

#[test]
fn test_normalize_lineno_none() {
    assert_eq!(normalize_sentry_position(None, None), None);
}

#[test]
fn test_normalize_lineno_zero() {
    assert_eq!(normalize_sentry_position(Some(0), Some(5)), None);
}

#[test]
fn test_normalize_lineno_one() {
    assert_eq!(normalize_sentry_position(Some(1), Some(0)), Some((0, 0)));
}

#[test]
fn test_normalize_lineno_valid() {
    assert_eq!(normalize_sentry_position(Some(5), Some(10)), Some((4, 10)));
}

#[test]
fn test_normalize_colno_absent() {
    assert_eq!(normalize_sentry_position(Some(3), None), Some((2, 0)));
}

#[test]
fn test_normalize_large_lineno_no_overflow() {
    assert_eq!(
        normalize_sentry_position(Some(u32::MAX), Some(0)),
        Some((u32::MAX - 1, 0))
    );
}

// ---------------------------------------------------------------------------
// rewrite_frames tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rewrite_hit() {
    let provider = FakeSourceMapProvider::with_data(make_simple_sourcemap());
    let mut event = make_event("_next/static/chunks/app.js", "abc123", 5, 0);

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"].as_str().unwrap(), "src/app/page.tsx");
    assert_eq!(frame["lineno"].as_u64().unwrap(), 5);
    let ctx = frame["context_line"].as_str().unwrap_or("");
    assert!(
        !ctx.is_empty(),
        "context_line must be non-empty after rewrite"
    );
    assert_eq!(ctx, "line 5");
}

#[tokio::test]
async fn test_rewrite_miss() {
    let provider = FakeSourceMapProvider::empty();
    let mut event = make_event("_next/static/chunks/app.js", "abc123", 5, 0);
    let orig = event["exception"]["values"][0]["stacktrace"]["frames"][0].clone();

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"], orig["filename"]);
    assert_eq!(frame["lineno"], orig["lineno"]);
}

#[tokio::test]
async fn test_rewrite_unmapped_token() {
    // ";;;;A" = 4 semicolons (skip lines 0-3) + single-part segment at line 4 col 0.
    // A single-part segment has no source info → token with src_line == u32::MAX.
    let sm_json = r#"{"version":3,"sources":["src/app.js"],"mappings":";;;;A"}"#;
    let provider = FakeSourceMapProvider::with_data(Bytes::from(sm_json));
    let mut event = make_event("bundle.js", "abc123", 5, 0);
    let orig_filename =
        event["exception"]["values"][0]["stacktrace"]["frames"][0]["filename"].clone();

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    // Frame unchanged — unmapped token must be skipped
    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"], orig_filename);
}

#[tokio::test]
async fn test_rewrite_lineno_zero() {
    let provider = FakeSourceMapProvider::with_data(make_simple_sourcemap());
    // lineno: 0 → normalize returns None → frame left unchanged
    let mut event = make_event("_next/static/chunks/app.js", "abc123", 0, 0);
    let orig = event["exception"]["values"][0]["stacktrace"]["frames"][0].clone();

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"], orig["filename"]);
    assert_eq!(frame["lineno"], orig["lineno"]);
}

#[tokio::test]
async fn test_rewrite_lineno_none() {
    let provider = FakeSourceMapProvider::with_data(make_simple_sourcemap());
    let mut event = json!({
        "debug_meta": {
            "images": [{"code_file": "bundle.js", "debug_id": "abc123"}]
        },
        "exception": {
            "values": [{
                "stacktrace": {
                    "frames": [{"filename": "bundle.js", "colno": 0}]
                }
            }]
        }
    });
    let orig = event["exception"]["values"][0]["stacktrace"]["frames"][0].clone();

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"], orig["filename"]);
}

#[tokio::test]
async fn test_rewrite_multi_source() {
    let provider = FakeSourceMapProvider::with_data(make_multi_source_sourcemap());
    // lineno=5 → normalize → (4,0) → maps to source index 15
    let mut event = make_event("bundle.js", "abc123", 5, 0);

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    let ctx = frame["context_line"].as_str().unwrap_or("");
    assert_eq!(
        ctx, "correct line from source 15",
        "context_line must come from source index 15, not index 0"
    );
    assert_ne!(ctx, "wrong content at source 0");
}

#[tokio::test]
async fn test_rewrite_cross_project() {
    struct ProjectOneOnlyProvider {
        data: Bytes,
    }

    #[async_trait]
    impl SourceMapProvider for ProjectOneOnlyProvider {
        async fn fetch_sourcemap(
            &self,
            project_id: i32,
            _debug_id: &str,
            _file_type: &str,
        ) -> AppResult<Option<SourceMapEntry>> {
            if project_id == 1 {
                Ok(Some(SourceMapEntry {
                    data: self.data.clone(),
                }))
            } else {
                Ok(None)
            }
        }
    }

    let provider = ProjectOneOnlyProvider {
        data: make_simple_sourcemap(),
    };
    let mut event = make_event("_next/static/chunks/app.js", "abc123", 5, 0);
    let orig_filename =
        event["exception"]["values"][0]["stacktrace"]["frames"][0]["filename"].clone();
    let orig_lineno = event["exception"]["values"][0]["stacktrace"]["frames"][0]["lineno"].clone();

    // project_id=2 → provider returns None → frame unchanged
    rewrite_frames(&provider, 2, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"], orig_filename);
    assert_eq!(frame["lineno"], orig_lineno);
}

#[tokio::test]
async fn test_rewrite_null_sourcescontent_entry() {
    let provider = FakeSourceMapProvider::with_data(make_null_contents_sourcemap());
    let mut event = make_event("bundle.js", "abc123", 5, 0);

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    // No panic; context_line is absent or empty
    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    let ctx = frame["context_line"].as_str().unwrap_or("");
    assert_eq!(
        ctx, "",
        "context_line must be empty when sourcesContent is null"
    );
}

#[tokio::test]
async fn test_rewrite_parse_error() {
    let provider =
        FakeSourceMapProvider::with_data(Bytes::from_static(b"not a valid source map {{{{"));
    let mut event = make_event("bundle.js", "abc123", 5, 0);
    let orig = event["exception"]["values"][0]["stacktrace"]["frames"][0].clone();

    // Must return Ok(()) — parse errors are non-fatal
    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    // Frame unchanged after parse failure
    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"], orig["filename"]);
    assert_eq!(frame["lineno"], orig["lineno"]);
}

#[tokio::test]
async fn test_rewrite_pre_context_at_file_start() {
    let provider = FakeSourceMapProvider::with_data(make_line_zero_sourcemap());
    // lineno=1 → normalize → (0, 0) → token at src_line=0 → l=0, pre_start=0.saturating_sub(3)=0
    let mut event = make_event("bundle.js", "abc123", 1, 0);

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    let pre: &Vec<serde_json::Value> = frame["pre_context"].as_array().unwrap();
    assert!(
        pre.is_empty(),
        "pre_context must be empty when token is at line 0"
    );
    // Rewrite still succeeded
    assert_eq!(frame["filename"].as_str().unwrap(), "src/app/page.tsx");
}

#[tokio::test]
async fn test_rewrite_no_debug_meta() {
    let provider = FakeSourceMapProvider::with_data(make_simple_sourcemap());
    let mut event = json!({
        "exception": {
            "values": [{
                "stacktrace": {
                    "frames": [{"filename": "bundle.js", "lineno": 5, "colno": 0}]
                }
            }]
        }
    });
    let orig = event["exception"]["values"][0]["stacktrace"]["frames"][0].clone();

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"], orig["filename"]);
}

// ---------------------------------------------------------------------------
// abs_path cleared after rewrite
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rewrite_clears_abs_path() {
    // After a successful rewrite, abs_path must become null.
    // Sentry clears it so the UI shows the original source file, not the minified URL.
    let provider = FakeSourceMapProvider::with_data(make_simple_sourcemap());
    let mut event = json!({
        "debug_meta": {
            "images": [{"code_file": "https://cdn.example.com/bundle.min.js", "debug_id": "abc123"}]
        },
        "exception": {
            "values": [{
                "stacktrace": {
                    "frames": [{
                        "filename": "https://cdn.example.com/bundle.min.js",
                        "abs_path": "https://cdn.example.com/bundle.min.js",
                        "lineno": 5,
                        "colno": 0
                    }]
                }
            }]
        }
    });

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(
        frame["filename"].as_str().unwrap(),
        "src/app/page.tsx",
        "filename must be rewritten to original source"
    );
    assert!(
        frame["abs_path"].is_null(),
        "abs_path must be null after rewrite; got: {}",
        frame["abs_path"]
    );
}

// ---------------------------------------------------------------------------
// frame.data.sourcemap set after rewrite
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rewrite_sets_data_sourcemap() {
    // After rewrite, frame["data"]["sourcemap"] must be set to the debug_id.
    // Sentry uses this for UI observability (which source map was applied).
    let debug_id = "550e8400-e29b-41d4-a716-446655440000";
    let provider = FakeSourceMapProvider::with_data(make_simple_sourcemap());
    let mut event = make_event("_next/static/chunks/app.js", debug_id, 5, 0);

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(
        frame["data"]["sourcemap"].as_str(),
        Some(debug_id),
        "frame.data.sourcemap must be set to the debug_id after rewrite"
    );
}

#[tokio::test]
async fn test_rewrite_data_sourcemap_not_set_on_miss() {
    // When no source map found, frame.data.sourcemap must NOT be set.
    let provider = FakeSourceMapProvider::empty();
    let mut event = make_event("bundle.js", "abc123", 5, 0);

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    // data.sourcemap should not be present when no rewrite happened
    let sourcemap_field = frame.get("data").and_then(|d| d.get("sourcemap"));
    assert!(
        sourcemap_field.is_none() || sourcemap_field.unwrap().is_null(),
        "frame.data.sourcemap must not be set when no source map found"
    );
}

// ---------------------------------------------------------------------------
// test_rewrite_image_missing_code_file (existing)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rewrite_image_missing_code_file() {
    let provider = FakeSourceMapProvider::with_data(make_simple_sourcemap());
    // images entry has debug_id but no code_file → filter_map returns None → skipped
    let mut event = json!({
        "debug_meta": {
            "images": [{"debug_id": "abc123"}]
        },
        "exception": {
            "values": [{
                "stacktrace": {
                    "frames": [{"filename": "bundle.js", "lineno": 5, "colno": 0}]
                }
            }]
        }
    });
    let orig = event["exception"]["values"][0]["stacktrace"]["frames"][0].clone();

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"], orig["filename"]);
}

// ---------------------------------------------------------------------------
// Hermes / Metro maps (`x_facebook_sources`)
// ---------------------------------------------------------------------------

/// Minimal Hermes map: generated line 0 / col 0 → `src/App.tsx` line 4,
/// function name `foo` from `x_facebook_sources`.
///
/// Adapted from getsentry/rust-sourcemap tests/fixtures/react-native-hermes.
fn make_hermes_sourcemap() -> Bytes {
    let json = r#"{
        "version": 3,
        "sources": ["src/App.tsx"],
        "sourcesContent": ["line 1\nline 2\nline 3\nline 4\nfunction foo() {\n  throw new Error('boom');\n}"],
        "names": [],
        "mappings": "AAIA",
        "x_facebook_sources": [[{"names": ["foo"], "mappings": "AAA"}]]
    }"#;
    Bytes::from(json)
}

/// Index map shaped like Turbopack's output: embedded sections, the second
/// starting at generated line 3 and the third further along that same line.
fn make_index_sourcemap() -> Bytes {
    let json = r#"{
        "version": 3,
        "sections": [
            {
                "offset": {"line": 0, "column": 0},
                "map": {
                    "version": 3,
                    "sources": ["src/a.ts"],
                    "sourcesContent": ["a line 1\na line 2"],
                    "names": [],
                    "mappings": "AAAA;AACA"
                }
            },
            {
                "offset": {"line": 3, "column": 0},
                "map": {
                    "version": 3,
                    "sources": ["src/b.ts"],
                    "sourcesContent": ["b line 1\nb line 2\nb line 3"],
                    "names": [],
                    "mappings": "AAEA"
                }
            },
            {
                "offset": {"line": 3, "column": 40},
                "map": {
                    "version": 3,
                    "sources": ["src/c.ts"],
                    "sourcesContent": ["c line 1"],
                    "names": [],
                    "mappings": "AAAA"
                }
            }
        ]
    }"#;
    Bytes::from(json)
}

#[tokio::test]
async fn test_rewrite_index_map_line_offset() {
    let provider = FakeSourceMapProvider::with_data(make_index_sourcemap());
    // Generated line 4 (1-based) is the second section's first line.
    let mut event = make_event("_next/server/chunks/app.js", "abc123", 4, 5);

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"].as_str().unwrap(), "src/b.ts");
    assert_eq!(frame["lineno"].as_u64().unwrap(), 3);
    assert_eq!(frame["context_line"].as_str().unwrap(), "b line 3");
}

#[tokio::test]
async fn test_rewrite_index_map_column_offset() {
    let provider = FakeSourceMapProvider::with_data(make_index_sourcemap());
    let mut event = make_event("_next/server/chunks/app.js", "abc123", 4, 45);

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"].as_str().unwrap(), "src/c.ts");
    assert_eq!(frame["context_line"].as_str().unwrap(), "c line 1");
}

#[tokio::test]
async fn test_rewrite_index_map_first_section() {
    let provider = FakeSourceMapProvider::with_data(make_index_sourcemap());
    let mut event = make_event("_next/server/chunks/app.js", "abc123", 2, 0);

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"].as_str().unwrap(), "src/a.ts");
    assert_eq!(frame["context_line"].as_str().unwrap(), "a line 2");
}

/// Index map whose second section maps nothing before column 20 of its
/// first line, and whose third section maps nothing at all.
fn make_index_sourcemap_with_unmapped_sections() -> Bytes {
    let json = r#"{
        "version": 3,
        "sections": [
            {
                "offset": {"line": 0, "column": 0},
                "map": {
                    "version": 3,
                    "sources": ["src/a.ts"],
                    "sourcesContent": ["a line 1\na line 2\na line 3"],
                    "names": [],
                    "mappings": "AAAA;AACA;AACA"
                }
            },
            {
                "offset": {"line": 3, "column": 0},
                "map": {
                    "version": 3,
                    "sources": ["src/b.ts"],
                    "sourcesContent": ["b line 1"],
                    "names": [],
                    "mappings": "oBAAA"
                }
            },
            {
                "offset": {"line": 5, "column": 0},
                "map": {"version": 3, "sources": [], "names": [], "mappings": ""}
            }
        ]
    }"#;
    Bytes::from(json)
}

#[tokio::test]
async fn test_rewrite_index_map_does_not_borrow_preceding_section() {
    let provider = FakeSourceMapProvider::with_data(make_index_sourcemap_with_unmapped_sections());

    // Line 4, column 5: inside section two, before its first mapping.
    let mut before_first_mapping = make_event("_next/server/chunks/app.js", "abc123", 4, 5);
    rewrite_frames(&provider, 1, &mut before_first_mapping)
        .await
        .unwrap();
    let frame = &before_first_mapping["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(
        frame["filename"].as_str().unwrap(),
        "_next/server/chunks/app.js"
    );
    assert!(frame.get("context_line").is_none());

    // Line 6: inside section three, which maps nothing.
    let mut empty_section = make_event("_next/server/chunks/app.js", "abc123", 6, 0);
    rewrite_frames(&provider, 1, &mut empty_section)
        .await
        .unwrap();
    let frame = &empty_section["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(
        frame["filename"].as_str().unwrap(),
        "_next/server/chunks/app.js"
    );

    // Line 4, column 25: section two's own mapping still resolves.
    let mut mapped = make_event("_next/server/chunks/app.js", "abc123", 4, 25);
    rewrite_frames(&provider, 1, &mut mapped).await.unwrap();
    let frame = &mapped["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(frame["filename"].as_str().unwrap(), "src/b.ts");
    assert_eq!(frame["context_line"].as_str().unwrap(), "b line 1");
}

#[tokio::test]
async fn test_rewrite_hermes_map() {
    let provider = FakeSourceMapProvider::with_data(make_hermes_sourcemap());
    // Hermes frames are reported as lineno=1, colno=bytecode_offset (here 0).
    let mut event = json!({
        "debug_meta": {
            "images": [{"code_file": "index.android.bundle", "debug_id": "abc123"}]
        },
        "exception": {
            "values": [{
                "stacktrace": {
                    "frames": [{
                        "filename": "index.android.bundle",
                        "lineno": 1,
                        "colno": 0,
                        "function": "?anon_0_"
                    }]
                }
            }]
        }
    });

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    let frame = &event["exception"]["values"][0]["stacktrace"]["frames"][0];
    assert_eq!(
        frame["filename"].as_str().unwrap(),
        "src/App.tsx",
        "Hermes map must rewrite filename to the original source"
    );
    assert_eq!(
        frame["lineno"].as_u64().unwrap(),
        5,
        "Hermes map must rewrite lineno (src_line 4 + 1)"
    );
    assert_eq!(
        frame["function"].as_str().unwrap(),
        "foo",
        "Hermes map must resolve the original function name from x_facebook_sources"
    );
    assert_eq!(
        frame["context_line"].as_str().unwrap(),
        "function foo() {",
        "context_line must come from the original Hermes source"
    );
}

// ---------------------------------------------------------------------------
// One fetch per source map per event, however many frames point at it
// ---------------------------------------------------------------------------

struct CountingProvider {
    data: Option<Bytes>,
    fetches: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl SourceMapProvider for CountingProvider {
    async fn fetch_sourcemap(
        &self,
        _project_id: i32,
        _debug_id: &str,
        _file_type: &str,
    ) -> AppResult<Option<SourceMapEntry>> {
        self.fetches
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self
            .data
            .as_ref()
            .map(|d| SourceMapEntry { data: d.clone() }))
    }
}

#[tokio::test]
async fn test_rewrite_fetches_each_sourcemap_once_per_event() {
    let provider = CountingProvider {
        data: Some(make_simple_sourcemap()),
        fetches: std::sync::atomic::AtomicUsize::new(0),
    };
    let frame = |lineno: u64| json!({"filename": "app.js", "lineno": lineno, "colno": 0});
    let mut event = json!({
        "debug_meta": {"images": [{"code_file": "app.js", "debug_id": "abc123"}]},
        "exception": {"values": [
            {"stacktrace": {"frames": [frame(1), frame(2), frame(3)]}},
            {"stacktrace": {"frames": [frame(4), frame(5)]}}
        ]},
        "threads": {"values": [
            {"stacktrace": {"frames": [frame(1), frame(2)]}}
        ]}
    });

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    assert_eq!(
        provider.fetches.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the same debug_id must be fetched and parsed once per event"
    );
    // Every frame still gets rewritten.
    for (v, n) in [(0, 3), (1, 2)] {
        for i in 0..n {
            let f = &event["exception"]["values"][v]["stacktrace"]["frames"][i];
            assert_eq!(f["filename"], "src/app/page.tsx", "exception frame {v}/{i}");
        }
    }
    for i in 0..2 {
        let f = &event["threads"]["values"][0]["stacktrace"]["frames"][i];
        assert_eq!(f["filename"], "src/app/page.tsx", "thread frame {i}");
    }
}

#[tokio::test]
async fn test_rewrite_asks_once_per_event_for_a_missing_sourcemap() {
    let provider = CountingProvider {
        data: None,
        fetches: std::sync::atomic::AtomicUsize::new(0),
    };
    let frame = |lineno: u64| json!({"filename": "app.js", "lineno": lineno, "colno": 0});
    let mut event = json!({
        "debug_meta": {"images": [{"code_file": "app.js", "debug_id": "missing"}]},
        "exception": {"values": [
            {"stacktrace": {"frames": [frame(1), frame(2), frame(3), frame(4), frame(5)]}}
        ]}
    });

    rewrite_frames(&provider, 1, &mut event).await.unwrap();

    assert_eq!(
        provider.fetches.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a miss is remembered for the rest of the event"
    );
    let f = &event["exception"]["values"][0]["stacktrace"]["frames"][4];
    assert_eq!(f["filename"], "app.js", "frames stay untouched on a miss");
}
