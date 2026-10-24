//! Native (Apple) symbolication: what Sentry hands to Symbolicator for a
//! `cocoa`/`native` event, done in process.
//!
//! The event-level glue (which frames go in, how results merge back,
//! `raw_stacktrace`, image statuses and errors) follows Sentry's
//! `sentry/lang/native/processing.py` (2458718b2c). The symbolication itself
//! is a port of Symbolicator's `symbolicator-native` (`symbolicate.rs`,
//! `native.rs`, `module_lookup.rs`, 799f5d9696), on the same `symbolic`
//! crates.

mod function_name;

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde_json::{json, Map, Value};
use symbolic_common::{InstructionInfo, Name};
use symbolic_debuginfo::{Archive, FileFormat};
use symbolic_demangle::{Demangle, DemangleOptions};
use symbolic_symcache::{SymCache, SymCacheConverter};
use uuid::Uuid;

use crate::services::source_bundle::{get_context_lines, SourceBundle};

pub use function_name::{replace_enclosed_string, split_func_tokens, trim_function_name};

/// Sentry's `NATIVE_PLATFORMS` (`src/sentry/lang/native/utils.py`).
pub const NATIVE_PLATFORMS: &[&str] = &["objc", "cocoa", "swift", "native", "c", "csharp"];

/// Sentry's `NATIVE_IMAGE_TYPES`.
const NATIVE_IMAGE_TYPES: &[&str] = &[
    "apple",
    "symbolic",
    "elf",
    "macho",
    "pe",
    "pe_dotnet",
    "wasm",
];

/// `source_file_metadata.file_type` of an uploaded Mach-O debug file, as uploaded.
pub const MACHO_FILE_TYPE: &str = "macho";
/// `source_file_metadata.file_type` of the SymCache built from it at upload,
/// which is what events are symbolicated with.
pub const SYMCACHE_FILE_TYPE: &str = "symcache";

/// Symbolicator's demangling options.
const DEMANGLE_OPTIONS: DemangleOptions = DemangleOptions::complete().return_type(false);

/// The debug files an event's images need, as loaded from storage.
pub struct NativeFiles<'a> {
    /// Each image's debug ID with its SymCache, `None` when there is none.
    pub symcaches: Vec<(String, Option<&'a [u8]>)>,
    /// Source bundles by the debug ID of the object they belong to.
    pub source_bundles: Vec<(String, &'a [u8])>,
}

/// Whether `data` is a Mach-O file (thin or fat) this module symbolicates with.
pub fn is_macho(data: &[u8]) -> bool {
    Archive::peek(data) == FileFormat::MachO
}

/// One SymCache per object in a debug file (a fat Mach-O holds one per
/// architecture), keyed by the object's debug ID, with whether the object has
/// debug info (DWARF) and not only a symbol table. Refuses a file with no
/// object that has symbols.
pub fn build_symcaches(data: &[u8]) -> Result<Vec<(Uuid, bool, Vec<u8>)>, String> {
    let archive = Archive::parse(data).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for object in archive.objects() {
        let object = object.map_err(|e| e.to_string())?;
        if !object.has_debug_info() && !object.has_symbols() {
            continue;
        }
        let mut converter = SymCacheConverter::new();
        converter
            .process_object(&object)
            .map_err(|e| e.to_string())?;
        let mut cache = Vec::new();
        converter
            .serialize(&mut std::io::Cursor::new(&mut cache))
            .map_err(|e| e.to_string())?;
        out.push((object.debug_id().uuid(), object.has_debug_info(), cache));
    }
    if out.is_empty() {
        return Err("no object with debug information or symbols".into());
    }
    Ok(out)
}

/// Sentry's `is_native_event`: a native platform on the event or any frame.
pub fn is_native_event(event: &Value) -> bool {
    NATIVE_PLATFORMS.contains(&event_platform(event)) || !native_stacktraces(event).is_empty()
}

/// Debug IDs of the event's native images, in order.
pub fn native_image_ids(event: &Value) -> Vec<String> {
    modules(event).into_iter().map(|m| m.debug_id).collect()
}

/// Debug IDs of the native images some frame of `event` points into: the
/// only ones worth loading (an iOS event lists hundreds of system images).
pub fn referenced_image_ids(event: &Value) -> Vec<String> {
    let lookup = ModuleLookup::new(modules(event));
    let platform = event_platform(event);
    let mut ids: Vec<String> = native_stacktraces(event)
        .iter()
        .flat_map(|info| info.frames.iter())
        .filter(|f| handles_frame(f, platform))
        .filter_map(|f| {
            let addr = hex(&f["instruction_addr"])?;
            let mode = addr_mode(f.get("addr_mode"), &lookup);
            lookup.get(addr, mode).map(|m| m.debug_id.clone())
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Symbolicates `event` with the project's stored files when it is native.
/// Never fails: what cannot be loaded is reported in `errors`.
pub async fn process_event(
    provider: &dyn crate::services::sourcemap::SourceMapProvider,
    project_id: i32,
    event: &mut Value,
) {
    if !is_native_event(event) {
        return;
    }
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
    let mut bundles = Vec::new();
    for id in referenced_image_ids(event) {
        caches.push(load(id.clone(), SYMCACHE_FILE_TYPE).await);
        if let (id, Some(data)) = load(id, crate::services::jvm::SOURCE_BUNDLE_FILE_TYPE).await {
            bundles.push((id, data));
        }
    }
    let files = NativeFiles {
        symcaches: caches
            .iter()
            .map(|(id, data)| (id.clone(), data.as_deref()))
            .collect(),
        source_bundles: bundles
            .iter()
            .map(|(id, b)| (id.clone(), b.as_ref()))
            .collect(),
    };
    symbolicate_event(event, &files);
}

/// Symbolicates a native event in place, the way `process_native_stacktraces`
/// does with Symbolicator's response.
pub fn symbolicate_event(event: &mut Value, files: &NativeFiles<'_>) {
    let platform = event_platform(event).to_string();
    let infos = native_stacktraces(event);
    let lookup = ModuleLookup::new(modules(event));

    let caches: HashMap<&str, Result<SymCache<'_>, ()>> = files
        .symcaches
        .iter()
        .filter_map(|(id, data)| {
            let parsed = SymCache::parse((*data)?).map_err(|e| {
                log::warn!("unreadable symcache {}: {}", id, e);
            });
            Some((id.as_str(), parsed))
        })
        .collect();
    let mut bundles: HashMap<&str, SourceBundle<'_>> = files
        .source_bundles
        .iter()
        .filter_map(|(id, data)| Some((id.as_str(), SourceBundle::parse(data)?)))
        .collect();

    let signal = event["exception"]["values"][0]["mechanism"]["meta"]["signal"]["number"]
        .as_u64()
        .map(|s| s as u32);

    // Sentry sends frames callee first, which is the order Symbolicator walks.
    let requests: Vec<Vec<Value>> = infos
        .iter()
        .map(|info| frames_for_symbolication(&info.frames, &platform, info.adjustment.as_deref()))
        .collect();
    if requests.iter().all(Vec::is_empty) {
        return;
    }

    let mut referenced = HashSet::new();
    for frame in requests.iter().flatten() {
        if let Some(addr) = hex(&frame["instruction_addr"]) {
            if let Some(m) = lookup.get(addr, addr_mode(frame.get("addr_mode"), &lookup)) {
                referenced.insert(m.index);
            }
        }
    }

    let completed: Vec<Vec<CompleteFrame>> = infos
        .iter()
        .zip(&requests)
        .map(|(info, frames)| {
            let mut out = symbolicate_stacktrace(frames, &info.registers, signal, &lookup, &caches);
            for frame in out.iter_mut().filter(|f| f.status == "symbolicated") {
                apply_source_context(&mut bundles, frame);
            }
            out
        })
        .collect();

    // Images: `_merge_image` with Symbolicator's `debug_status`.
    let os = os_from_event(event);
    let mut errors = Vec::new();
    for module in &lookup.modules {
        let status = if !referenced.contains(&module.index) {
            "unused"
        } else {
            match caches.get(module.debug_id.as_str()) {
                Some(Ok(_)) => "found",
                Some(Err(())) => "malformed",
                None => "missing",
            }
        };
        let image = &mut event["debug_meta"]["images"][module.index];
        image["debug_status"] = status.into();
        if let Some(error) = image_status_error(status, image, os.as_deref()) {
            errors.push(error);
        }
    }

    for (info, complete) in infos.iter().zip(completed) {
        let mut by_index: HashMap<usize, Vec<&CompleteFrame>> = HashMap::new();
        for frame in &complete {
            by_index
                .entry(frame.original_index)
                .or_default()
                .push(frame);
        }
        let mut new_frames = Vec::new();
        let mut native_idx = 0;
        for raw in info.frames.iter().rev() {
            if !handles_frame(raw, &platform) {
                new_frames.push(raw.clone());
                continue;
            }
            for complete in by_index.get(&native_idx).into_iter().flatten() {
                let mut merged = raw.clone();
                let frame_platform = merged["platform"].as_str().unwrap_or(&platform).to_string();
                merge_frame(&mut merged, complete, &frame_platform);
                new_frames.push(merged);
            }
            native_idx += 1;
        }
        new_frames.reverse();

        let container = event
            .pointer_mut(&info.container)
            .expect("stack trace container exists");
        container["stacktrace"]["frames"] = Value::Array(new_frames);
        if !info.container.is_empty() && native_idx > 0 {
            container["raw_stacktrace"] = json!({
                "frames": info.frames,
                "registers": info.registers_value,
            });
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
}

// ---------------------------------------------------------------------------
// Sentry glue
// ---------------------------------------------------------------------------

fn event_platform(event: &Value) -> &str {
    event["platform"].as_str().unwrap_or("unknown")
}

fn is_native_platform(platform: &str) -> bool {
    NATIVE_PLATFORMS.contains(&platform)
}

/// Sentry's `_handles_frame`.
fn handles_frame(frame: &Value, platform: &str) -> bool {
    frame.is_object()
        && frame["data"]["symbolicator_status"].is_null()
        && is_native_platform(frame["platform"].as_str().unwrap_or(platform))
        && !frame["instruction_addr"].is_null()
}

/// One stack trace of Sentry's `find_stacktraces_in_data` with a native frame.
struct StacktraceInfo {
    /// JSON pointer of the exception or thread holding it, empty for the
    /// event's own `stacktrace`.
    container: String,
    frames: Vec<Value>,
    registers: HashMap<String, u64>,
    registers_value: Value,
    adjustment: Option<String>,
}

fn native_stacktraces(event: &Value) -> Vec<StacktraceInfo> {
    let platform = event_platform(event);
    let mut containers: Vec<String> = (0..array(&event["exception"]["values"]).len())
        .map(|i| format!("/exception/values/{i}"))
        .collect();
    containers.push(String::new());
    containers.extend(
        (0..array(&event["threads"]["values"]).len()).map(|i| format!("/threads/values/{i}")),
    );

    containers
        .into_iter()
        .filter_map(|container| {
            let stacktrace = &event.pointer(&container)?["stacktrace"];
            let frames = stacktrace["frames"].as_array().filter(|f| !f.is_empty())?;
            let native = frames
                .iter()
                .any(|f| is_native_platform(f["platform"].as_str().unwrap_or(platform)));
            native.then(|| StacktraceInfo {
                container,
                frames: frames.clone(),
                registers: stacktrace["registers"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter_map(|(k, v)| Some((k.clone(), hex(v)?)))
                    .collect(),
                registers_value: stacktrace.get("registers").cloned().unwrap_or(Value::Null),
                adjustment: stacktrace["instruction_addr_adjustment"]
                    .as_str()
                    .map(str::to_string),
            })
        })
        .collect()
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

/// Sentry's `get_frames_for_symbolication`: the handled frames, callee first,
/// with the stack trace's `instruction_addr_adjustment` applied.
fn frames_for_symbolication(
    frames: &[Value],
    platform: &str,
    adjustment: Option<&str>,
) -> Vec<Value> {
    let adjustment = adjustment.unwrap_or("auto");
    let mut out: Vec<Value> = frames
        .iter()
        .rev()
        .filter(|f| handles_frame(f, platform))
        .cloned()
        .map(|mut f| {
            if adjustment == "none" {
                f["adjust_instruction_addr"] = false.into();
            }
            f
        })
        .collect();
    if let Some(first) = out.first_mut() {
        match adjustment {
            "all" => first["adjust_instruction_addr"] = true.into(),
            "all_but_first" => first["adjust_instruction_addr"] = false.into(),
            _ => {}
        }
    }
    out
}

/// An address as the protocol sends it: a hex string or a number.
fn hex(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => {
            let s = s.trim();
            match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                Some(h) => u64::from_str_radix(h, 16).ok(),
                None => s.parse().ok(),
            }
        }
        _ => None,
    }
}

fn hex_string(addr: u64) -> String {
    format!("{addr:#x}")
}

/// Sentry's `get_os_from_event`.
fn os_from_event(event: &Value) -> Option<String> {
    let os = &event["contexts"]["os"];
    if os["type"] == "os" {
        if let Some(name) = os["name"].as_str() {
            return Some(name.to_lowercase());
        }
    }
    event["debug_meta"]["sdk_info"]["sdk_name"]
        .as_str()
        .map(str::to_lowercase)
}

// Sentry's `sentry/utils/in_app.py`.
const IOS_APP_PATHS: &[&str] = &[
    "/var/containers/Bundle/Application/",
    "/private/var/containers/Bundle/Application/",
];
const MACOS_APP_PATHS: &[&str] = &[".app/Contents/", "/Users/", "/usr/local/"];
const LINUX_SYS_PATHS: &[&str] = &["/lib/", "/usr/lib/", "linux-gate.so"];
static WINDOWS_SYS_PATH: LazyLock<fancy_regex::Regex> =
    LazyLock::new(|| fancy_regex::Regex::new(r"(?i)^[a-z]:\\windows").expect("valid regex"));
static SUPPORT_FRAMEWORK: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(
        r"/Frameworks/(libswift([a-zA-Z0-9]+)\.dylib$|(KSCrash|SentrySwift|Sentry)\.framework/)",
    )
    .expect("valid regex")
});

fn is_support_framework(package: &str) -> bool {
    SUPPORT_FRAMEWORK.is_match(package).unwrap_or(false)
}

/// Sentry's `is_known_third_party`.
fn is_known_third_party(package: &str, os: Option<&str>) -> bool {
    if is_support_framework(package) {
        return true;
    }
    if IOS_APP_PATHS.iter().any(|p| package.starts_with(p)) {
        return false;
    }
    if package.contains("/Developer/CoreSimulator/Devices/")
        && package.contains("/Containers/Bundle/Application/")
    {
        return false;
    }
    match os {
        Some("macos") => !MACOS_APP_PATHS.iter().any(|p| package.contains(p)),
        Some("linux") => LINUX_SYS_PATHS.iter().any(|p| package.starts_with(p)),
        Some("windows") => WINDOWS_SYS_PATH.is_match(package).unwrap_or(false),
        _ => true,
    }
}

/// Sentry's `is_optional_package`.
fn is_optional_package(package: &str) -> bool {
    package.is_empty()
        || is_support_framework(package)
        || (IOS_APP_PATHS.iter().any(|p| package.starts_with(p))
            && package.contains("/Frameworks/"))
}

/// Sentry's `_handle_image_status`, for the statuses Rustrak produces.
fn image_status_error(status: &str, image: &Value, os: Option<&str>) -> Option<Value> {
    let ty = match status {
        "missing" => {
            let package = image["code_file"].as_str().filter(|p| !p.is_empty())?;
            if is_known_third_party(package, os) || image["type"] == "pe_dotnet" {
                return None;
            }
            if is_optional_package(package) {
                "native_optionally_bundled_dsym"
            } else {
                "native_missing_dsym"
            }
        }
        "malformed" => "native_bad_dsym",
        _ => return None,
    };
    let mut error = Map::new();
    error.insert("message".into(), Value::Null);
    error.insert("type".into(), ty.into());
    for (key, field) in [
        ("image_path", "code_file"),
        ("image_uuid", "debug_id"),
        ("image_arch", "arch"),
    ] {
        if let Some(v) = image.get(field).filter(|v| !v.is_null()) {
            error.insert(key.into(), v.clone());
        }
    }
    Some(Value::Object(error))
}

/// One frame of Symbolicator's response.
#[derive(Debug, Default)]
struct CompleteFrame {
    original_index: usize,
    status: &'static str,
    instruction_addr: Option<u64>,
    /// `rel:N` when the address is relative to an image at 0.
    addr_mode: Option<String>,
    function: Option<String>,
    symbol: Option<String>,
    package: Option<String>,
    abs_path: Option<String>,
    filename: Option<String>,
    lineno: Option<u32>,
    pre_context: Vec<String>,
    context_line: Option<String>,
    post_context: Vec<String>,
    /// The image the frame resolved in, for its source bundle.
    debug_id: Option<String>,
}

/// Sentry's `_merge_frame`.
fn merge_frame(frame: &mut Value, symbolicated: &CompleteFrame, platform: &str) {
    let Some(obj) = frame.as_object_mut() else {
        return;
    };
    if platform != "csharp" {
        if let Some(function) = symbolicated.function.as_deref().filter(|f| !f.is_empty()) {
            let raw = trim(function, 256);
            let trimmed = trim(&trim_function_name(function, platform), 256);
            if trimmed != raw {
                obj.insert("raw_function".into(), raw.into());
            }
            obj.insert("function".into(), trimmed.into());
        }
    }
    if let Some(addr) = symbolicated.instruction_addr {
        obj.insert("instruction_addr".into(), hex_string(addr).into());
    }
    if let Some(symbol) = symbolicated.symbol.as_deref().filter(|s| !s.is_empty()) {
        obj.insert("symbol".into(), symbol.into());
    }
    if let Some(abs_path) = symbolicated.abs_path.as_deref().filter(|p| !p.is_empty()) {
        obj.insert("abs_path".into(), abs_path.into());
        let base = abs_path.rsplit('/').next().unwrap_or(abs_path);
        obj.insert("filename".into(), base.into());
    }
    if let Some(filename) = symbolicated.filename.as_deref().filter(|f| !f.is_empty()) {
        obj.insert("filename".into(), filename.into());
    }
    if let Some(lineno) = symbolicated.lineno.filter(|&l| l != 0) {
        obj.insert("lineno".into(), lineno.into());
    }
    if platform != "csharp" {
        if let Some(package) = symbolicated.package.as_deref().filter(|p| !p.is_empty()) {
            obj.insert("package".into(), package.into());
        }
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
    match &symbolicated.addr_mode {
        Some(mode) => {
            obj.insert("addr_mode".into(), mode.clone().into());
        }
        None => {
            obj.remove("addr_mode");
        }
    }
    let data = obj.entry("data").or_insert_with(|| json!({}));
    if let Some(data) = data.as_object_mut() {
        data.insert("symbolicator_status".into(), symbolicated.status.into());
    }
}

/// Sentry's `trim` for strings: at most `max` characters.
fn trim(value: &str, max: usize) -> String {
    match value.char_indices().nth(max) {
        Some((i, _)) => value[..i].to_string(),
        None => value.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Symbolicator port: module lookup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Module {
    /// Position in `debug_meta.images`.
    index: usize,
    debug_id: String,
    image_addr: u64,
    image_size: u64,
    code_file: Option<String>,
}

/// Sentry's `native_images_from_data`, with debug IDs normalized.
fn modules(event: &Value) -> Vec<Module> {
    array(&event["debug_meta"]["images"])
        .iter()
        .enumerate()
        .filter(|(_, img)| {
            img["type"]
                .as_str()
                .is_some_and(|t| NATIVE_IMAGE_TYPES.contains(&t))
        })
        .filter_map(|(index, img)| {
            let id = ["debug_id", "id", "uuid"]
                .iter()
                .find_map(|k| img[*k].as_str())?;
            Some(Module {
                index,
                debug_id: normalize_debug_id(id),
                image_addr: hex(&img["image_addr"]).unwrap_or(0),
                image_size: hex(&img["image_size"]).unwrap_or(0),
                code_file: img["code_file"].as_str().map(str::to_string),
            })
        })
        .collect()
}

/// A debug ID the way it is stored: a lowercase UUID, without an age.
fn normalize_debug_id(id: &str) -> String {
    let id = id.trim().to_lowercase();
    let uuid_part: String = id.chars().filter(|c| *c != '-').take(32).collect();
    Uuid::parse_str(&uuid_part)
        .map(|u| u.to_string())
        .unwrap_or(id)
}

/// Symbolicator's `ModuleLookup`: images by address.
struct ModuleLookup {
    /// Sorted by `image_addr`, sizes back-filled.
    modules: Vec<Module>,
}

impl ModuleLookup {
    fn new(mut modules: Vec<Module>) -> Self {
        modules.sort_by_key(|m| m.image_addr);
        // A missing size spans up to the next image, as the protocol says.
        for i in 0..modules.len().saturating_sub(1) {
            if modules[i].image_size == 0 {
                modules[i].image_size = modules[i + 1].image_addr - modules[i].image_addr;
            }
        }
        Self { modules }
    }

    /// `get_module_by_addr`.
    fn get(&self, addr: u64, mode: AddrMode) -> Option<&Module> {
        match mode {
            AddrMode::Abs => {
                let idx = match self.modules.binary_search_by_key(&addr, |m| m.image_addr) {
                    Ok(idx) => idx,
                    Err(0) => return None,
                    Err(idx) => idx - 1,
                };
                let module = &self.modules[idx];
                let end = module.image_addr.checked_add(module.image_size)?;
                if end < addr && module.image_size != 0 {
                    return None;
                }
                Some(module)
            }
            AddrMode::Rel(index) => self.modules.iter().find(|m| m.index == index),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AddrMode {
    Abs,
    /// Relative to the image at this position in `debug_meta.images`.
    Rel(usize),
}

/// Sentry's `addr_mode` validation: `abs`, or `rel:<index>` / `rel:<debug id>`.
fn addr_mode(value: Option<&Value>, lookup: &ModuleLookup) -> AddrMode {
    let Some(arg) = value
        .and_then(Value::as_str)
        .and_then(|m| m.strip_prefix("rel:"))
    else {
        return AddrMode::Abs;
    };
    let id = normalize_debug_id(arg);
    lookup
        .modules
        .iter()
        .find(|m| m.debug_id == id)
        .map(|m| m.index)
        .or_else(|| arg.parse().ok())
        .map(AddrMode::Rel)
        .unwrap_or(AddrMode::Abs)
}

// ---------------------------------------------------------------------------
// Symbolicator port: symbolication
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Adjust {
    Yes,
    No,
    Auto,
}

/// Symbolicator's `symbolicate_stacktrace`, frames callee first.
fn symbolicate_stacktrace(
    frames: &[Value],
    registers: &HashMap<String, u64>,
    signal: Option<u32>,
    lookup: &ModuleLookup,
    caches: &HashMap<&str, Result<SymCache<'_>, ()>>,
) -> Vec<CompleteFrame> {
    // Any explicit per-frame adjustment makes the others `Yes`.
    let default = if frames
        .iter()
        .any(|f| f["adjust_instruction_addr"].is_boolean())
    {
        Adjust::Yes
    } else {
        Adjust::Auto
    };
    let mut out: Vec<CompleteFrame> = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        let adjust = match frame["adjust_instruction_addr"].as_bool() {
            Some(true) => Adjust::Yes,
            Some(false) => Adjust::No,
            None => default,
        };
        let addr = hex(&frame["instruction_addr"]).unwrap_or(0);
        let mode = addr_mode(frame.get("addr_mode"), lookup);
        match symbolicate_frame(addr, mode, index, adjust, registers, signal, lookup, caches) {
            Ok(frames) => out.extend(frames),
            Err((status, package)) => {
                let is_outermost = index + 1 == frames.len();
                // An unmapped, null-ish outermost frame ends the unwinding.
                if status == "unknown_image"
                    && is_outermost
                    && mode == AddrMode::Abs
                    && addr < 4096
                    && !out.is_empty()
                {
                    continue;
                }
                // glibc's `_start` has no return address; what follows is garbage.
                if status == "unknown_image"
                    && is_outermost
                    && out
                        .last()
                        .and_then(|f| f.function.as_deref())
                        .is_some_and(|f| matches!(f, "_start" | "__clone" | "__clone3"))
                {
                    continue;
                }
                // Not symbolicated: demangle whichever of function/symbol came.
                let function = frame["function"].as_str();
                let symbol = frame["symbol"].as_str();
                let mangled = match (function, symbol) {
                    (Some(f), None) => Some(f),
                    (None, Some(s)) => Some(s),
                    _ => None,
                };
                let (function, symbol) =
                    match mangled.and_then(|m| Name::from(m).demangle(DEMANGLE_OPTIONS)) {
                        Some(demangled) => (Some(demangled), function.map(str::to_string)),
                        None => (None, None),
                    };
                out.push(CompleteFrame {
                    original_index: index,
                    status,
                    function,
                    symbol: symbol.filter(|_| frame["function"].is_string()),
                    package,
                    addr_mode: frame["addr_mode"]
                        .as_str()
                        .filter(|m| m.starts_with("rel:"))
                        .map(str::to_string),
                    ..Default::default()
                });
            }
        }
    }
    out
}

type FrameResult = Result<Vec<CompleteFrame>, (&'static str, Option<String>)>;

/// Symbolicator's `symbolicate_frame` + `get_relative_caller_addr` +
/// `symbolicate_native_frame`.
#[allow(clippy::too_many_arguments)]
fn symbolicate_frame(
    addr: u64,
    mode: AddrMode,
    index: usize,
    adjust: Adjust,
    registers: &HashMap<String, u64>,
    signal: Option<u32>,
    lookup: &ModuleLookup,
    caches: &HashMap<&str, Result<SymCache<'_>, ()>>,
) -> FrameResult {
    let module = lookup.get(addr, mode).ok_or(("unknown_image", None))?;
    let package = module.code_file.clone();
    let cache = match caches.get(module.debug_id.as_str()) {
        Some(Ok(cache)) => cache,
        Some(Err(())) => return Err(("malformed", package)),
        None => return Err(("missing", package)),
    };

    // Images mounted at 0 have no absolute addresses.
    let absolute = module.image_addr != 0;
    let relative = match mode {
        AddrMode::Abs => addr.checked_sub(module.image_addr),
        AddrMode::Rel(_) => Some(addr),
    }
    .ok_or(("missing_symbol", package.clone()))?;
    let relative = if absolute {
        let crashing = index == 0;
        let ip = crashing
            .then(|| cache.arch().cpu_family().ip_register_name())
            .flatten()
            .and_then(|reg| registers.get(reg).copied());
        let mut info = InstructionInfo::new(cache.arch(), module.image_addr + relative);
        info.is_crashing_frame(crashing)
            .signal(signal)
            .ip_register_value(ip);
        let caller = match adjust {
            Adjust::Yes => info.previous_address(),
            Adjust::No => info.aligned_address(),
            Adjust::Auto => info.caller_address(),
        };
        caller
            .checked_sub(module.image_addr)
            .ok_or(("missing_symbol", package.clone()))?
    } else {
        relative
    };

    let instruction_addr = if absolute {
        module.image_addr + relative
    } else {
        relative
    };
    let addr_mode = (!absolute).then(|| format!("rel:{}", module.index));
    let frames: Vec<CompleteFrame> = cache
        .lookup(relative)
        .map(|location| {
            let function = location.function();
            let abs_path = location
                .file()
                .map(|f| f.full_path())
                .filter(|p| !p.is_empty());
            let filename = abs_path
                .as_deref()
                .map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string());
            let name = function
                .name_for_demangling()
                .demangle(DEMANGLE_OPTIONS)
                .unwrap_or_else(|| function.name().to_string());
            CompleteFrame {
                original_index: index,
                status: "symbolicated",
                instruction_addr: Some(instruction_addr),
                addr_mode: addr_mode.clone(),
                function: Some(name),
                symbol: Some(function.name().to_string()),
                package: package.clone(),
                abs_path,
                filename,
                lineno: Some(location.line()),
                debug_id: Some(module.debug_id.clone()),
                ..Default::default()
            }
        })
        .collect();
    if frames.is_empty() {
        return Err(("missing_symbol", package));
    }
    Ok(frames)
}

/// Symbolicator's native `try_set_source_context`: the frame's file from the
/// source bundle uploaded for its image, by absolute path.
fn apply_source_context(bundles: &mut HashMap<&str, SourceBundle<'_>>, frame: &mut CompleteFrame) {
    let (Some(id), Some(path), Some(lineno)) = (&frame.debug_id, &frame.abs_path, frame.lineno)
    else {
        return;
    };
    let Some(source) = bundles.get_mut(id.as_str()).and_then(|b| b.source(path)) else {
        return;
    };
    if let Some((pre, line, post)) = get_context_lines(source, lineno as usize) {
        frame.pre_context = pre;
        frame.context_line = Some(line);
        frame.post_context = post;
    }
}
