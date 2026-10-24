//! JVM deobfuscation: what Sentry hands to Symbolicator for an Android event,
//! done in process.
//!
//! The event-level glue (which frames go in, how results merge back,
//! `raw_stacktrace`, exception and error fields) follows Sentry's
//! `sentry/lang/java/processing.py` and `exceptions.py` (2458718b2c). The
//! remapping itself is a port of Symbolicator's
//! `symbolicator-proguard/src/symbolication.rs` (799f5d9696), on the same
//! `proguard` crate.

use std::collections::HashMap;
use std::sync::LazyLock;

use proguard::{ProguardCache, ProguardMapping};

use crate::services::source_bundle::{get_context_lines, SourceBundle};
use serde_json::{json, Value};

/// `source_file_metadata.file_type` of an uploaded ProGuard mapping, as uploaded.
pub const PROGUARD_FILE_TYPE: &str = "proguard";
/// `source_file_metadata.file_type` of the cache built from a mapping at
/// upload, which is what events are remapped with.
pub const PROGUARD_CACHE_FILE_TYPE: &str = "proguard_cache";
/// `source_file_metadata.file_type` of a JVM source bundle.
pub const SOURCE_BUNDLE_FILE_TYPE: &str = "source_bundle";

const JAVA_PLATFORMS: &[&str] = &["java"];

/// The debug files an event's `debug_meta` names, as loaded from storage.
pub struct JvmFiles<'a> {
    /// Each `proguard` image's UUID with its cache, `None` when there is none.
    pub proguard: Vec<(String, Option<&'a [u8]>)>,
    /// The source bundles of the event's `jvm` images that were found.
    pub source_bundles: Vec<&'a [u8]>,
}

/// Builds the cache events are remapped with, the way Symbolicator does on
/// first use. Refuses a file that is not a ProGuard mapping.
///
/// This is the expensive step (memory grows with the mapping, ~1.4 GB for a
/// 33 MB one), which is why it runs once, in the assembly worker.
pub fn build_proguard_cache(mapping: &[u8]) -> Result<Vec<u8>, String> {
    let mapping = ProguardMapping::new(mapping);
    if !mapping.is_valid() {
        return Err("The file is not a valid ProGuard file".into());
    }
    let mut out = Vec::new();
    ProguardCache::write(&mapping, &mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Sentry's `is_jvm_event`: a Java platform on the event or any frame, or a
/// valid `proguard`/`jvm` image.
pub fn is_jvm_event(event: &Value) -> bool {
    let platform = event_platform(event);
    if JAVA_PLATFORMS.contains(&platform) {
        return true;
    }
    let java_frame = stacktraces(event).iter().any(|st| {
        frames_of(st)
            .iter()
            .any(|f| JAVA_PLATFORMS.contains(&frame_platform(f, platform)))
    });
    java_frame || !proguard_ids(event).is_empty() || !source_bundle_ids(event).is_empty()
}

/// UUIDs of the event's `proguard` images, lowercased and deduplicated.
pub fn proguard_ids(event: &Value) -> Vec<String> {
    image_ids(event, "proguard", "uuid")
}

/// Debug IDs of the event's `jvm` images (source bundles), lowercased and deduplicated.
pub fn source_bundle_ids(event: &Value) -> Vec<String> {
    image_ids(event, "jvm", "debug_id")
}

fn image_ids(event: &Value, ty: &str, key: &str) -> Vec<String> {
    let mut ids: Vec<String> = event["debug_meta"]["images"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|img| img["type"] == ty)
        .filter_map(|img| img[key].as_str())
        .map(str::to_lowercase)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// The package Sentry's `Release.package` holds: the part of
/// `<package>@<version>` before the `@`.
pub fn release_package(release: Option<&str>) -> Option<&str> {
    release?.split_once('@').map(|(package, _)| package)
}

/// Deobfuscates `event` with the project's stored files when it is a JVM
/// event. Never fails: what cannot be loaded is reported in `errors`.
pub async fn process_event(
    provider: &dyn crate::services::sourcemap::SourceMapProvider,
    project_id: i32,
    event: &mut Value,
) {
    if !is_jvm_event(event) {
        return;
    }
    // ponytail: each file is read whole for every event (a large app's cache
    // is tens of MB); keep them mapped or cached across events if digest
    // memory or throughput says so.
    let load = |id: String, file_type: &'static str| async move {
        let data = match provider.fetch_sourcemap(project_id, &id, file_type).await {
            Ok(entry) => entry.map(|e| e.data),
            Err(e) => {
                log::warn!("loading {} {}: {:?}", file_type, id, e);
                None
            }
        };
        (id, data)
    };
    let mut caches = Vec::new();
    for id in proguard_ids(event) {
        caches.push(load(id, PROGUARD_CACHE_FILE_TYPE).await);
    }
    let mut bundles = Vec::new();
    for id in source_bundle_ids(event) {
        if let (_, Some(data)) = load(id, SOURCE_BUNDLE_FILE_TYPE).await {
            bundles.push(data);
        }
    }
    let files = JvmFiles {
        proguard: caches
            .iter()
            .map(|(id, data)| (id.clone(), data.as_deref()))
            .collect(),
        source_bundles: bundles.iter().map(|b| b.as_ref()).collect(),
    };
    let release = event["release"].as_str().map(str::to_string);
    symbolicate_event(event, &files, release_package(release.as_deref()));
}

/// Deobfuscates a JVM event in place, the way `process_jvm_stacktraces`
/// does with Symbolicator's response, then `deobfuscate_exception_value`.
pub fn symbolicate_event(event: &mut Value, files: &JvmFiles<'_>, release_package: Option<&str>) {
    let mut errors = Vec::new();
    let mut mappers = Vec::new();
    for (uuid, data) in &files.proguard {
        match data.map(ProguardCache::parse) {
            Some(Ok(cache)) => mappers.push(cache),
            Some(Err(e)) => {
                log::warn!("unreadable proguard cache {}: {}", uuid, e);
                errors.push(json!({
                    "type": "proguard_missing_lineno",
                    "symbolicator_type": "invalid",
                    "mapping_uuid": uuid,
                }));
            }
            None => errors.push(json!({
                "type": "proguard_missing_mapping",
                "symbolicator_type": "missing",
                "mapping_uuid": uuid,
            })),
        }
    }
    let mut bundles: Vec<SourceBundle> = files
        .source_bundles
        .iter()
        .filter_map(|data| SourceBundle::parse(data))
        .collect();

    let platform = event_platform(event).to_string();
    let infos = stacktraces(event);
    let requests: Vec<Vec<JvmFrame>> = infos
        .iter()
        .map(|info| {
            frames_of(info)
                .iter()
                .enumerate()
                .filter(|(_, f)| handles_frame(f, &platform))
                .map(|(index, f)| JvmFrame::from_value(f, index))
                .collect()
        })
        .collect();

    let exceptions = Exceptions::new(event);
    if requests.iter().all(Vec::is_empty)
        && exceptions.processable.is_empty()
        && exceptions.class_names().is_empty()
    {
        return;
    }

    let mapped_exceptions: Vec<JvmException> = exceptions
        .processable
        .iter()
        .map(|exc| map_exception(&mappers, exc).unwrap_or_else(|| exc.clone()))
        .collect();

    for (info, request) in infos.iter().zip(requests) {
        let exception = info
            .exception
            .as_ref()
            .map(|exc| map_exception(&mappers, exc).unwrap_or_else(|| exc.clone()));
        let mut frames = request;
        // Event order is caller first; remapping walks callee first.
        frames.reverse();
        let mut remapped = map_stacktrace(&mappers, &frames, release_package, exception.as_ref());
        remapped.reverse();
        for frame in &mut remapped {
            apply_source_context(&mut bundles, frame);
        }

        let raw_frames = frames_of(info).to_vec();
        let mut new_frames = Vec::with_capacity(raw_frames.len());
        for (index, raw) in raw_frames.iter().enumerate() {
            let matching: Vec<&JvmFrame> = remapped.iter().filter(|f| f.index == index).collect();
            if !matching.is_empty() {
                for symbolicated in matching {
                    let mut frame = raw.clone();
                    merge_frame(&mut frame, symbolicated);
                    new_frames.push(frame);
                }
            } else if !handles_frame(raw, &platform) {
                new_frames.push(raw.clone());
            }
        }

        let container = pointer_mut(event, &info.container);
        container["stacktrace"]["frames"] = Value::Array(new_frames);
        if info.has_container() {
            container["raw_stacktrace"] = json!({ "frames": raw_frames });
        }
    }

    if !errors.is_empty() {
        if !event["errors"].is_array() {
            event["errors"] = json!([]);
        }
        if let Some(list) = event["errors"].as_array_mut() {
            list.extend(errors);
        }
    }

    let classes: HashMap<String, String> = exceptions
        .class_names()
        .into_iter()
        .filter_map(|class| {
            let mapped = mappers.iter().find_map(|m| m.remap_class(&class))?;
            Some((class, mapped.to_string()))
        })
        .collect();
    exceptions.deobfuscate_and_save(event, &classes, &mapped_exceptions);

    if !files.proguard.is_empty() {
        deobfuscate_exception_value(event);
    }
}

// ---------------------------------------------------------------------------
// Sentry glue: stack traces, frames, exceptions
// ---------------------------------------------------------------------------

fn event_platform(event: &Value) -> &str {
    event["platform"].as_str().unwrap_or("unknown")
}

fn frame_platform<'a>(frame: &'a Value, platform: &'a str) -> &'a str {
    frame["platform"].as_str().unwrap_or(platform)
}

/// Sentry's `_handles_frame`.
fn handles_frame(frame: &Value, platform: &str) -> bool {
    frame.get("function").is_some()
        && frame.get("module").is_some()
        && JAVA_PLATFORMS.contains(&frame_platform(frame, platform))
}

/// One stack trace Sentry's `find_stacktraces_in_data` returns. `container`
/// is the JSON pointer of the object holding `stacktrace`: an exception or a
/// thread, or the event itself for a top-level stack trace.
struct StacktraceInfo {
    container: String,
    frames: Vec<Value>,
    exception: Option<JvmException>,
}

impl StacktraceInfo {
    fn has_container(&self) -> bool {
        !self.container.is_empty()
    }
}

fn frames_of(info: &StacktraceInfo) -> &[Value] {
    &info.frames
}

fn stacktraces(event: &Value) -> Vec<StacktraceInfo> {
    let mut out = Vec::new();
    let mut push = |container: String, exception: Option<JvmException>| {
        let holder = event.pointer(&container).unwrap_or(&Value::Null);
        let Some(frames) = holder["stacktrace"]["frames"].as_array() else {
            return;
        };
        if frames.is_empty() {
            return;
        }
        out.push(StacktraceInfo {
            container,
            frames: frames.clone(),
            exception,
        });
    };
    for (i, exc) in array(&event["exception"]["values"]).iter().enumerate() {
        push(
            format!("/exception/values/{i}"),
            JvmException::from_value(exc),
        );
    }
    push(String::new(), None);
    for i in 0..array(&event["threads"]["values"]).len() {
        push(format!("/threads/values/{i}"), None);
    }
    out
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn pointer_mut<'a>(event: &'a mut Value, pointer: &str) -> &'a mut Value {
    // Every pointer came from `stacktraces`, on this same event.
    event
        .pointer_mut(pointer)
        .expect("stack trace container exists")
}

/// The frame fields Sentry's `_normalize_frame` sends, plus what the
/// remapping fills in.
#[derive(Debug, Clone, Default)]
struct JvmFrame {
    function: String,
    module: String,
    filename: Option<String>,
    abs_path: Option<String>,
    lineno: Option<u32>,
    in_app: Option<bool>,
    pre_context: Vec<String>,
    context_line: Option<String>,
    post_context: Vec<String>,
    method_synthesized: bool,
    index: usize,
}

impl JvmFrame {
    fn from_value(frame: &Value, index: usize) -> Self {
        let text = |key: &str| frame[key].as_str().map(str::to_string);
        Self {
            function: text("function").unwrap_or_default(),
            module: text("module").unwrap_or_default(),
            filename: text("filename"),
            abs_path: text("abs_path"),
            lineno: frame["lineno"].as_u64().map(|l| l as u32),
            in_app: frame["in_app"].as_bool(),
            index,
            ..Default::default()
        }
    }
}

/// Sentry's `_merge_frame`.
fn merge_frame(frame: &mut Value, symbolicated: &JvmFrame) {
    let Some(obj) = frame.as_object_mut() else {
        return;
    };
    if !symbolicated.function.is_empty() {
        obj.insert("function".into(), symbolicated.function.clone().into());
    }
    for (key, value) in [
        ("abs_path", &symbolicated.abs_path),
        ("filename", &symbolicated.filename),
    ] {
        match value.as_deref().filter(|v| !v.is_empty()) {
            Some(v) => {
                obj.insert(key.into(), v.into());
            }
            None => {
                obj.remove(key);
            }
        }
    }
    if let Some(lineno) = symbolicated.lineno {
        obj.insert("lineno".into(), lineno.into());
    }
    if !symbolicated.module.is_empty() {
        obj.insert("module".into(), symbolicated.module.clone().into());
    }
    if let Some(in_app) = symbolicated.in_app {
        obj.insert("in_app".into(), in_app.into());
    }
    if !symbolicated.pre_context.is_empty() {
        obj.insert(
            "pre_context".into(),
            symbolicated.pre_context.clone().into(),
        );
    }
    if let Some(line) = &symbolicated.context_line {
        obj.insert("context_line".into(), line.clone().into());
    }
    if !symbolicated.post_context.is_empty() {
        obj.insert(
            "post_context".into(),
            symbolicated.post_context.clone().into(),
        );
    }
    if symbolicated.method_synthesized {
        obj.insert("method_synthesized".into(), true.into());
        obj.insert("in_app".into(), false.into());
    }
}

#[derive(Debug, Clone)]
struct JvmException {
    ty: String,
    module: String,
}

impl JvmException {
    fn from_value(exc: &Value) -> Option<Self> {
        Some(Self {
            ty: exc["type"].as_str().filter(|t| !t.is_empty())?.to_string(),
            module: exc["module"].as_str().unwrap_or_default().to_string(),
        })
    }

    /// R8 aggressive mode can move a class into the root package, leaving
    /// `module` empty.
    fn full_class_name(&self) -> String {
        if self.module.is_empty() {
            self.ty.clone()
        } else {
            format!("{}.{}", self.module, self.ty)
        }
    }
}

/// Likely Java class references in free text (Sentry's `_JAVA_CLASS_IN_TEXT_RE`).
static JAVA_CLASS_IN_TEXT: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(
        r#"(?:[A-Za-z_$][\w$]*\.){1,}[A-Za-z_$][\w$]*(?:\$[A-Za-z_$][\w$]*)*|['"][A-Za-z_$][\w$]*['"]"#,
    )
    .expect("valid regex")
});

/// A whole `<from> cannot be cast to <to>` value (Sentry's `_CLASS_CAST_MESSAGE_RE`).
static CLASS_CAST_MESSAGE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(
        r"^(?P<from_class>[A-Za-z_$][\w$.]*)(?P<from_array_suffix>(?:\[\])*) cannot be cast to (?P<to_class>[A-Za-z_$][\w$.]*)(?P<to_array_suffix>(?:\[\])*)$",
    )
    .expect("valid regex")
});

/// Sentry's `Exceptions`: the exceptions whose type can be remapped, and the
/// class names their values mention.
struct Exceptions {
    /// Every value with a type, and its index in `exception.values`.
    processable: Vec<JvmException>,
    processable_index: Vec<usize>,
    /// `(index in exception.values, class names in its value)`.
    with_values: Vec<(usize, Vec<String>)>,
}

impl Exceptions {
    fn new(event: &Value) -> Self {
        let mut processable = Vec::new();
        let mut processable_index = Vec::new();
        let mut with_values = Vec::new();
        for (i, exc) in array(&event["exception"]["values"]).iter().enumerate() {
            if let Some(e) = JvmException::from_value(exc) {
                processable.push(e);
                processable_index.push(i);
            }
            let Some(value) = exc["value"].as_str().filter(|v| !v.is_empty()) else {
                continue;
            };
            let classes: Vec<String> = match CLASS_CAST_MESSAGE.captures(value).ok().flatten() {
                Some(caps) => vec![caps["from_class"].to_string(), caps["to_class"].to_string()],
                None => JAVA_CLASS_IN_TEXT
                    .find_iter(value)
                    .filter_map(Result::ok)
                    .map(|m| m.as_str().to_string())
                    .collect(),
            };
            if !classes.is_empty() {
                with_values.push((i, classes));
            }
        }
        Self {
            processable,
            processable_index,
            with_values,
        }
    }

    fn class_names(&self) -> Vec<String> {
        self.with_values
            .iter()
            .flat_map(|(_, names)| names.iter().map(|n| strip_quotes(n).to_string()))
            .collect()
    }

    /// Sentry's `deobfuscate_and_save`.
    fn deobfuscate_and_save(
        &self,
        event: &mut Value,
        classes: &HashMap<String, String>,
        mapped: &[JvmException],
    ) {
        let values = &mut event["exception"]["values"];
        for (&i, exc) in self.processable_index.iter().zip(mapped) {
            let raw = &mut values[i];
            raw["raw_module"] = raw.get("module").cloned().unwrap_or(Value::Null);
            raw["raw_type"] = raw.get("type").cloned().unwrap_or(Value::Null);
            raw["module"] = exc.module.clone().into();
            raw["type"] = exc.ty.clone().into();
        }
        if classes.is_empty() {
            return;
        }
        for (i, names) in &self.with_values {
            let exc = &mut values[*i];
            let Some(original) = exc["value"].as_str().map(str::to_string) else {
                continue;
            };
            if let Some(mapped) = deobfuscate_class_cast_message(&original, classes) {
                exc["raw_value"] = original.into();
                exc["value"] = mapped.into();
                continue;
            }
            let mut tokens: Vec<&String> = Vec::new();
            for name in names {
                if !tokens.contains(&name) {
                    tokens.push(name);
                }
            }
            tokens.sort_by_key(|t| std::cmp::Reverse(strip_quotes(t).len()));
            let mut value = original.clone();
            let mut replaced = false;
            for token in tokens {
                let Some(mapped) = classes
                    .get(token.as_str())
                    .or_else(|| classes.get(strip_quotes(token)))
                else {
                    continue;
                };
                replaced = true;
                value = value.replace(token.as_str(), &wrap_with_same_quotes(token, mapped));
            }
            if replaced {
                exc["raw_value"] = original.into();
                exc["value"] = value.into();
            }
        }
    }
}

fn deobfuscate_class_cast_message(
    value: &str,
    classes: &HashMap<String, String>,
) -> Option<String> {
    let caps = CLASS_CAST_MESSAGE.captures(value).ok().flatten()?;
    let (from, to) = (&caps["from_class"], &caps["to_class"]);
    let (mapped_from, mapped_to) = (classes.get(from), classes.get(to));
    if mapped_from.is_none() && mapped_to.is_none() {
        return None;
    }
    Some(format!(
        "{}{} cannot be cast to {}{}",
        mapped_from.map(String::as_str).unwrap_or(from),
        &caps["from_array_suffix"],
        mapped_to.map(String::as_str).unwrap_or(to),
        &caps["to_array_suffix"],
    ))
}

fn is_quoted(token: &str) -> bool {
    token.len() >= 2
        && ((token.starts_with('\'') && token.ends_with('\''))
            || (token.starts_with('"') && token.ends_with('"')))
}

fn strip_quotes(token: &str) -> &str {
    if is_quoted(token) {
        &token[1..token.len() - 1]
    } else {
        token
    }
}

fn wrap_with_same_quotes(original: &str, replacement: &str) -> String {
    if is_quoted(original) {
        let quote = &original[..1];
        format!("{quote}{replacement}{quote}")
    } else {
        replacement.to_string()
    }
}

/// Sentry's `deobfuscate_exception_value` preprocessor: the last exception's
/// value names its raw top frame's method, so it gets the remapped name.
fn deobfuscate_exception_value(event: &mut Value) {
    let Some(exc) = event["exception"]["values"]
        .as_array_mut()
        .and_then(|v| v.last_mut())
    else {
        return;
    };
    let method = |frame: &Value| -> Option<String> {
        let module = frame["module"].as_str().filter(|s| !s.is_empty())?;
        let function = frame["function"].as_str().filter(|s| !s.is_empty())?;
        Some(format!("{module}.{function}"))
    };
    let last = |key: &str| {
        exc[key]["frames"]
            .as_array()
            .and_then(|f| f.last())
            .and_then(method)
    };
    let (Some(deobfuscated), Some(raw)) = (last("stacktrace"), last("raw_stacktrace")) else {
        return;
    };
    if let Some(value) = exc["value"].as_str().filter(|v| !v.is_empty()) {
        let value = value.replace(&raw, &deobfuscated);
        exc["value"] = value.into();
    }
}

// ---------------------------------------------------------------------------
// Symbolicator port: remapping
// ---------------------------------------------------------------------------

enum FullRemapResult {
    /// Outline frame: skip it, keep rewrite eligibility for the next one.
    OutlineFrame,
    /// Rewrite rules cleared every frame: skip it and stop rewriting.
    RewriteCleared,
    Frames(Vec<JvmFrame>),
    NoFrames,
}

/// Symbolicator's `map_exception`.
fn map_exception(mappers: &[ProguardCache<'_>], exception: &JvmException) -> Option<JvmException> {
    let key = exception.full_class_name();
    let mapped = mappers.iter().find_map(|m| m.remap_class(&key))?;
    let (module, ty) = mapped.rsplit_once('.').unwrap_or(("", mapped));
    Some(JvmException {
        ty: ty.into(),
        module: module.into(),
    })
}

/// Symbolicator's `map_stacktrace`, with frames callee first. Sentry never
/// sends a frame `signature`, so that path is left out.
fn map_stacktrace(
    mappers: &[ProguardCache<'_>],
    stacktrace: &[JvmFrame],
    release_package: Option<&str>,
    exception: Option<&JvmException>,
) -> Vec<JvmFrame> {
    let mut carried_outline_pos = vec![None; mappers.len()];
    let mut remapped_frames = Vec::new();
    let exception_descriptor =
        exception.map(|exc| proguard::class_name_to_descriptor(&exc.full_class_name()));
    let mut next_frame_can_rewrite = exception_descriptor.is_some();

    'frames: for frame in stacktrace {
        // Line number when there is one, else line 0, which ProguardCache
        // answers with a single frame, as retrace does.
        let proguard_frame = proguard::StackFrame::new(
            &frame.module,
            &frame.function,
            frame.lineno.unwrap_or(0) as usize,
        );

        let mut mapped_result = None;
        for (mapper_idx, mapper) in mappers.iter().enumerate() {
            match map_full_frame(
                mapper,
                &proguard_frame,
                frame,
                exception_descriptor.as_deref(),
                next_frame_can_rewrite,
                &mut carried_outline_pos[mapper_idx],
            ) {
                FullRemapResult::OutlineFrame => continue 'frames,
                FullRemapResult::RewriteCleared => {
                    next_frame_can_rewrite = false;
                    continue 'frames;
                }
                FullRemapResult::Frames(frames) => {
                    mapped_result = Some(frames);
                    break;
                }
                FullRemapResult::NoFrames => {}
            }
            if let Some(frames) = map_class_method(mapper, frame) {
                mapped_result = Some(frames);
                break;
            }
            if let Some(frames) = map_class(mapper, frame) {
                mapped_result = Some(frames);
                break;
            }
        }
        next_frame_can_rewrite = false;

        if let Some(frames) = mapped_result.as_mut() {
            // Compiler-synthesized frames go, but only when the frame had a
            // line number: retrace keeps them otherwise.
            if frame.lineno.is_some() {
                frames.retain(|f| !f.method_synthesized);
                if frames.is_empty() {
                    continue 'frames;
                }
            }
            for mapped in frames.iter_mut() {
                if let Some(package) = release_package {
                    if mapped.module.starts_with(package) && mapped.in_app.is_none() {
                        mapped.in_app = Some(true);
                    }
                }
            }
        }
        remapped_frames.extend(mapped_result.unwrap_or_else(|| vec![frame.clone()]));
    }
    remapped_frames
}

fn map_full_frame(
    mapper: &ProguardCache<'_>,
    proguard_frame: &proguard::StackFrame<'_>,
    original: &JvmFrame,
    exception_descriptor: Option<&str>,
    apply_rewrite: bool,
    carried_outline_pos: &mut Option<usize>,
) -> FullRemapResult {
    let Some(iter) = mapper.remap_frame_with_context(
        proguard_frame,
        exception_descriptor,
        apply_rewrite,
        carried_outline_pos,
    ) else {
        return FullRemapResult::OutlineFrame;
    };
    let had_mappings = iter.had_mappings();
    let frames: Vec<_> = iter.collect();
    if frames.is_empty() {
        return if had_mappings {
            FullRemapResult::RewriteCleared
        } else {
            FullRemapResult::NoFrames
        };
    }
    FullRemapResult::Frames(
        frames
            .iter()
            .map(|f| JvmFrame {
                module: f.class().to_owned(),
                function: f.method().to_owned(),
                lineno: f.line().map(|l| l as u32),
                abs_path: f
                    .file()
                    .map(String::from)
                    .or_else(|| original.abs_path.clone()),
                filename: f
                    .file()
                    .map(String::from)
                    .or_else(|| original.filename.clone()),
                method_synthesized: f.method_synthesized(),
                ..original.clone()
            })
            .collect(),
    )
}

fn map_class_method(mapper: &ProguardCache<'_>, frame: &JvmFrame) -> Option<Vec<JvmFrame>> {
    let (class, method) = mapper.remap_method(&frame.module, &frame.function)?;
    Some(vec![JvmFrame {
        module: class.to_owned(),
        function: method.to_owned(),
        ..frame.clone()
    }])
}

fn map_class(mapper: &ProguardCache<'_>, frame: &JvmFrame) -> Option<Vec<JvmFrame>> {
    let class = mapper.remap_class(&frame.module)?;
    Some(vec![JvmFrame {
        module: class.to_owned(),
        ..frame.clone()
    }])
}

// ---------------------------------------------------------------------------
// Symbolicator port: source context
// ---------------------------------------------------------------------------

/// Symbolicator's `apply_source_context`.
fn apply_source_context(bundles: &mut [SourceBundle<'_>], frame: &mut JvmFrame) {
    let lineno = match frame.lineno {
        None | Some(0) => return,
        Some(n) => n as usize,
    };
    let url = build_source_file_name(frame);
    for bundle in bundles {
        let Some(source) = bundle.source(&url) else {
            continue;
        };
        if let Some((pre, line, post)) = get_context_lines(source, lineno) {
            frame.pre_context = pre;
            frame.context_line = Some(line);
            frame.post_context = post;
            break;
        }
    }
}

/// Symbolicator's `build_source_file_name`: `~/<package path>/<file stem>.jvm`.
fn build_source_file_name(frame: &JvmFrame) -> String {
    let mut name = String::from("~/");
    let valid_path = frame
        .abs_path
        .as_deref()
        .filter(|p| !p.contains('$'))
        .and_then(|p| p.rsplit_once('.'))
        .map(|(before, _)| before);
    match valid_path {
        Some(stem) => {
            if let Some((package, _)) = frame.module.rsplit_once('.') {
                name.push_str(&package.replace('.', "/"));
                name.push('/');
            }
            name.push_str(stem);
        }
        None => {
            let class = frame.module.split('$').next().unwrap_or(&frame.module);
            name.push_str(&class.replace('.', "/"));
        }
    }
    // The real extension (.java, .kt) is not known, so bundles use .jvm.
    name.push_str(".jvm");
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_file_name_drops_inner_classes() {
        // Symbolicator `test_build_source_file_name`.
        let frame = JvmFrame {
            function: "run".into(),
            module: "com.foo.bar.ui.activities.base.BaseActivity$$ExternalSyntheticLambda2".into(),
            ..Default::default()
        };
        assert_eq!(
            build_source_file_name(&frame),
            "~/com/foo/bar/ui/activities/base/BaseActivity.jvm"
        );
    }

    #[test]
    fn source_file_name_uses_the_file_stem_and_package() {
        let frame = JvmFrame {
            module: "com.example.GreeterKt".into(),
            abs_path: Some("Greeter.kt".into()),
            ..Default::default()
        };
        assert_eq!(build_source_file_name(&frame), "~/com/example/Greeter.jvm");
    }
}
