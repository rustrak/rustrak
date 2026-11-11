use std::borrow::Cow;

use crate::protocol::clamp;
use crate::services::normalize_message_for_grouping;
use std::collections::{HashMap, HashSet};

use dynfmt::{Argument, Format, FormatArgs, PythonFormat, SimpleCurlyFormat};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Separator used in grouping keys (diamond character)
const GROUPING_SEPARATOR: &str = " ⋄ ";

/// How deep the exception tree may be walked before the chain is left
/// untouched. Sentry's walk is unbounded, but Python's default recursion limit
/// stops it at the same depth and the caller's blanket `except` turns that into
/// the unfiltered chain, so the fallback matches at the same threshold.
const MAX_EXCEPTION_TREE_DEPTH: usize = 1000;

/// Calculates the grouping key for an event
pub fn calculate_grouping_key(event_data: &Value) -> String {
    let components = grouping_components(event_data);
    let transaction = get_transaction(event_data);

    // Check for custom fingerprint. An empty fingerprint (sent as `[]` by
    // sentry-ruby, or left empty after Relay drops null/array/object parts)
    // means "no custom fingerprint" and falls back to default grouping, as
    // in Sentry (`event.data.get("fingerprint") or ["{{ default }}"]`).
    if let Some(fingerprint) = event_data.get("fingerprint").and_then(|f| f.as_array()) {
        let parts: Vec<String> = fingerprint
            .iter()
            .filter_map(|part| {
                let coerced = coerce_fingerprint_element(part)?;
                if coerced == "{{ default }}" {
                    Some(default_grouping_key(&components, &transaction))
                } else {
                    Some(coerced)
                }
            })
            .collect();
        if !parts.is_empty() {
            return parts.join(GROUPING_SEPARATOR);
        }
    }

    // Default grouping
    default_grouping_key(&components, &transaction)
}

/// One `"Type: value"` per exception the chain contributes, mirroring Sentry's
/// `chained_exception` component, which holds every surviving exception rather
/// than a single chosen one.
fn grouping_components(event_data: &Value) -> Vec<String> {
    if let Some(values) = exception_values(event_data) {
        let (chain, _) = filter_exception_groups(values);
        let contributing: Vec<&&Value> = chain.iter().filter(|e| !is_synthetic(e)).collect();
        if !contributing.is_empty() {
            return contributing
                .iter()
                .map(|exception| {
                    let (exc_type, exc_value) = type_and_value_of(exception);
                    let exc_type = if exc_type.is_empty() {
                        "Error"
                    } else {
                        exc_type
                    };
                    let exc_value = normalize_message_for_grouping(exc_value);
                    get_title(&clamp(exc_type, 128), &clamp(&exc_value, 1024))
                })
                .collect();
        }
    }

    let (calculated_type, calculated_value) =
        raw_type_and_value(event_data, MessagePreference::Grouping);
    let calculated_value = normalize_message_for_grouping(&calculated_value);
    vec![get_title(
        &clamp(&calculated_type, 128),
        &clamp(&calculated_value, 1024),
    )]
}

/// Coerces a single fingerprint array element to a string, mirroring Relay's
/// `LenientString` (relay-event-schema/src/protocol/types.rs:722-747).
///
/// Returns `None` for elements Relay drops (null, arrays, objects) so the
/// caller can skip them rather than emit an empty component.
fn coerce_fingerprint_element(part: &Value) -> Option<String> {
    match part {
        Value::String(s) => Some(s.clone()),
        // True/False (capitalized) for legacy python compatibility.
        Value::Bool(true) => Some("True".to_string()),
        Value::Bool(false) => Some("False".to_string()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i.to_string())
            } else if let Some(u) = n.as_u64() {
                Some(u.to_string())
            } else {
                // Float: truncate toward zero, like Relay's `num.trunc()`.
                n.as_f64().map(|f| f.trunc().to_string())
            }
        }
        // null, arrays, and objects are dropped.
        _ => None,
    }
}

/// Default grouping key: every contributing component, then the transaction.
fn default_grouping_key(components: &[String], transaction: &str) -> String {
    let mut parts: Vec<&str> = components.iter().map(String::as_str).collect();
    parts.push(transaction);
    parts.join(GROUPING_SEPARATOR)
}

/// Calculates the SHA256 hash of the grouping key
pub fn hash_grouping_key(grouping_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(grouping_key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extracts the type and value used to title the issue.
pub fn get_type_and_value(event_data: &Value) -> (String, String) {
    type_and_value(event_data, MessagePreference::Title)
}

fn type_and_value(event_data: &Value, preference: MessagePreference) -> (String, String) {
    let (calculated_type, calculated_value) = raw_type_and_value(event_data, preference);
    (clamp(&calculated_type, 128), clamp(&calculated_value, 1024))
}

/// The type and value as the event carries them.
///
/// The 128 and 1024 limits belong to an issue's title (Sentry applies them in
/// `eventtypes/error.py`, never in its grouping strategies), so grouping asks
/// for the text untouched and applies them itself once the event-specific
/// values are out: a value cut mid-pattern no longer matches it, and two
/// messages that differ only there would open two issues.
fn raw_type_and_value(event_data: &Value, preference: MessagePreference) -> (String, String) {
    // Try to extract from exception, unless it is synthetic. Relay ignores the
    // type/value of synthetic exceptions (signal/segfault wrappers) for
    // grouping and falls through to the next component
    // (relay-event-schema/src/protocol/mechanism.rs:113).
    if let Some(exception) = get_main_exception(event_data).filter(|e| !is_synthetic(e)) {
        let exc_type = exception
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("Error")
            .to_string();

        let exc_value = exception
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        return (exc_type, exc_value);
    }

    // Try to extract from logentry/message
    if let Some(message) = get_log_message(event_data, preference) {
        return ("Log Message".to_string(), message);
    }

    // Fallback
    ("Unknown".to_string(), String::new())
}

/// Gets the exception that should drive the type and value: the one Sentry
/// would mark as `main_exception_id`, falling back to the last of the chain.
fn get_main_exception(event_data: &Value) -> Option<&Value> {
    let values = exception_values(event_data)?;
    match main_exception_id(values) {
        Some(id) => values
            .iter()
            .find(|e| mechanism_id(e, "exception_id") == Some(id))
            .or_else(|| values.last()),
        None => values.last(),
    }
}

/// The exception Sentry would title the issue by, or `None` to keep the
/// default. Mirrors the two steps of its grouping strategy: collapse
/// exception-group wrappers, then let the framework-specific overrides move the
/// choice off a wrapper that carries no information.
fn main_exception_id(values: &[Value]) -> Option<u64> {
    let (collapsed, from_groups) = filter_exception_groups(values);
    override_main_exception_id(&collapsed).or(from_groups)
}

/// Wrappers that exist only to carry a real error, in the order Sentry tries
/// them (`MAIN_EXCEPTION_ID_FUNCS`); the first match wins.
fn override_main_exception_id(exceptions: &[&Value]) -> Option<u64> {
    react_error_with_cause(exceptions)
        .or_else(|| java_rxjava_framework_exception(exceptions))
        .or_else(|| kotlin_diagnostic_wrapper(exceptions))
}

/// React 19 wraps a recovered render error around the error that caused it.
fn react_error_with_cause(exceptions: &[&Value]) -> Option<u64> {
    const REACT_ERRORS_WITH_CAUSE: [&str; 2] = [
        "There was an error during concurrent rendering but React was able to recover by instead synchronously rendering the entire root.",
        "There was an error while hydrating but React was able to recover by instead client rendering from the nearest Suspense boundary.",
    ];
    let (first, last) = (exceptions.first()?, exceptions.last()?);
    let (ty, value) = type_and_value_of(first);
    if ty == "Error"
        && REACT_ERRORS_WITH_CAUSE.contains(&value)
        && last.get("mechanism")?.get("source")?.as_str() == Some("cause")
    {
        return mechanism_id(last, "exception_id");
    }
    None
}

/// RxJava wraps the real error in a framework exception; the answer is its
/// direct child.
fn java_rxjava_framework_exception(exceptions: &[&Value]) -> Option<u64> {
    const RXJAVA_TYPES: [&str; 3] = [
        "OnErrorNotImplementedException",
        "CompositeException",
        "UndeliverableException",
    ];
    if exceptions.len() < 2 {
        return None;
    }
    let wrapper_id = exceptions.iter().find_map(|e| {
        let is_rxjava = e.get("module").and_then(|m| m.as_str())
            == Some("io.reactivex.rxjava3.exceptions")
            && RXJAVA_TYPES.contains(&type_and_value_of(e).0);
        is_rxjava.then(|| mechanism_id(e, "exception_id"))?
    })?;
    exceptions.iter().find_map(|e| {
        (mechanism_id(e, "parent_id") == Some(wrapper_id))
            .then(|| mechanism_id(e, "exception_id"))?
    })
}

/// Kotlin Coroutines and Compose add diagnostic wrappers with no stacktrace and
/// a placeholder message. Walk past them to the first real parent.
fn kotlin_diagnostic_wrapper(exceptions: &[&Value]) -> Option<u64> {
    const WRAPPERS: [(&str, &str); 2] = [
        (
            "kotlinx.coroutines.internal",
            "DiagnosticCoroutineContextException",
        ),
        (
            "androidx.compose.runtime.tooling",
            "DiagnosticComposeException",
        ),
    ];
    let is_wrapper = |e: &Value| {
        let module = e.get("module").and_then(|m| m.as_str()).unwrap_or("");
        WRAPPERS.contains(&(module, type_and_value_of(e).0))
    };
    if exceptions.len() < 2 || !exceptions.iter().any(|e| is_wrapper(e)) {
        return None;
    }
    let by_id: HashMap<u64, &&Value> = exceptions
        .iter()
        .filter_map(|e| mechanism_id(e, "exception_id").map(|id| (id, e)))
        .collect();

    for exception in exceptions.iter().filter(|e| is_wrapper(e)) {
        // Bounded by the number of exceptions, so a malformed cycle terminates.
        let mut current = *exception;
        for _ in 0..by_id.len() {
            let Some(parent_id) = mechanism_id(current, "parent_id") else {
                break;
            };
            let Some(parent) = by_id.get(&parent_id) else {
                break;
            };
            if !is_wrapper(parent) {
                return Some(parent_id);
            }
            current = parent;
        }
    }
    None
}

pub(crate) fn exception_values(event_data: &Value) -> Option<&Vec<Value>> {
    let exception = event_data.get("exception")?;
    if exception.is_array() {
        exception.as_array()
    } else {
        exception.get("values")?.as_array()
    }
}

fn mechanism_id(exception: &Value, field: &str) -> Option<u64> {
    exception.get("mechanism")?.get(field)?.as_u64()
}

fn is_exception_group(exception: &Value) -> bool {
    exception
        .get("mechanism")
        .and_then(|m| m.get("is_exception_group"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Collapses exception-group wrappers that only add a level to the chain,
/// returning the surviving chain and the exception the title should use when
/// the wrapper turned out to carry nothing. A port of Sentry's
/// `filter_exceptions_for_exception_groups`; any malformed tree returns the
/// chain untouched.
fn filter_exception_groups(values: &[Value]) -> (Vec<&Value>, Option<u64>) {
    let as_is = || (values.iter().collect::<Vec<_>>(), None);
    if values.len() <= 1 {
        return as_is();
    }

    // Reconstruct the tree. A missing, duplicated or self-parenting id means
    // the chain cannot be trusted, so nothing is filtered.
    let mut children: HashMap<u64, Vec<&Value>> = HashMap::new();
    let mut by_id: HashMap<u64, &Value> = HashMap::new();
    for exception in values.iter().rev() {
        let Some(id) = mechanism_id(exception, "exception_id") else {
            return as_is();
        };
        let parent = mechanism_id(exception, "parent_id");
        if parent == Some(id) || by_id.contains_key(&id) {
            return as_is();
        }
        by_id.insert(id, exception);
        if let Some(parent) = parent {
            children.entry(parent).or_default().push(exception);
        }
    }

    let Some(root) = by_id.get(&0) else {
        return as_is();
    };

    let mut top_level = Vec::new();
    if !collect_top_level(root, &children, &mut top_level, &mut HashSet::new(), 0) {
        return as_is();
    }
    if top_level.is_empty() {
        return as_is();
    }
    // Sorted by type so sibling de-duplication is deterministic.
    top_level.sort_by(|a, b| type_and_value_of(b).cmp(&type_and_value_of(a)));

    let mut distinct: Vec<&Value> = Vec::new();
    for exception in top_level {
        if distinct
            .last()
            .is_none_or(|prev| type_and_value_of(prev) != type_and_value_of(exception))
        {
            distinct.push(exception);
        }
    }

    if distinct.len() == 1 {
        // The wrapper adds nothing: keep the inner error and its first path.
        let mut path = Vec::new();
        if !collect_first_path(distinct[0], &children, &mut path, &mut HashSet::new(), 0) {
            return as_is();
        }
        let main = mechanism_id(distinct[0], "exception_id");
        return (path, main);
    }

    distinct.push(root);
    (distinct, None)
}

/// The pair Rustrak groups by, standing in for Sentry's grouping component
/// when de-duplicating sibling exceptions.
fn type_and_value_of(exception: &Value) -> (&str, &str) {
    (
        exception.get("type").and_then(|t| t.as_str()).unwrap_or(""),
        exception
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
}

/// Direct descendants of exception groups that are not groups themselves.
///
/// Returns `false` when the walk meets an exception it has already visited or
/// descends past `MAX_EXCEPTION_TREE_DEPTH`. Sentry recurses here unguarded and
/// lets the caller catch the `RecursionError` a cycle or a deep chain raises; a
/// blown Rust stack has no such second chance, so both end the walk and the
/// whole filter falls back to the untouched chain.
fn collect_top_level<'a>(
    exception: &'a Value,
    children: &HashMap<u64, Vec<&'a Value>>,
    out: &mut Vec<&'a Value>,
    visited: &mut HashSet<u64>,
    depth: usize,
) -> bool {
    if !is_exception_group(exception) {
        out.push(exception);
        return true;
    }
    if depth >= MAX_EXCEPTION_TREE_DEPTH {
        return false;
    }
    let Some(id) = mechanism_id(exception, "exception_id") else {
        return true;
    };
    if !visited.insert(id) {
        return false;
    }
    children
        .get(&id)
        .into_iter()
        .flatten()
        .all(|child| collect_top_level(child, children, out, visited, depth + 1))
}

/// Walks from an exception to a leaf, following the first child each time.
/// Returns `false` on a cycle or past `MAX_EXCEPTION_TREE_DEPTH`, for the same
/// reasons `collect_top_level` does.
fn collect_first_path<'a>(
    exception: &'a Value,
    children: &HashMap<u64, Vec<&'a Value>>,
    out: &mut Vec<&'a Value>,
    visited: &mut HashSet<u64>,
    depth: usize,
) -> bool {
    out.push(exception);
    if depth >= MAX_EXCEPTION_TREE_DEPTH {
        return false;
    }
    let Some(id) = mechanism_id(exception, "exception_id") else {
        return true;
    };
    if !visited.insert(id) {
        return false;
    }
    match children.get(&id).and_then(|c| c.first()) {
        Some(first) => collect_first_path(first, children, out, visited, depth + 1),
        None => true,
    }
}

/// Whether an exception is marked synthetic via `mechanism.synthetic`.
fn is_synthetic(exception: &Value) -> bool {
    exception
        .get("mechanism")
        .and_then(|m| m.get("synthetic"))
        .and_then(|s| s.as_bool())
        .unwrap_or(false)
}

/// Grouping takes the template so every rendering lands in one issue; the title
/// takes the rendered text.
#[derive(Clone, Copy)]
enum MessagePreference {
    Grouping,
    Title,
}

/// Renders `message` with `params`, the way an SDK would have rendered
/// `formatted` itself. Returns `None` when the template takes no parameters or
/// the parameters do not fit it, leaving the template as the best text we have.
fn format_message(template: &str, params: &Value) -> Option<String> {
    let args = ParamArgs(params);
    if template.contains('%') {
        PythonFormat
            .format(template, args)
            .ok()
            .map(Cow::into_owned)
    } else if template.contains('{') {
        SimpleCurlyFormat
            .format(template, args)
            .ok()
            .map(Cow::into_owned)
    } else {
        None
    }
}

struct ParamArgs<'a>(&'a Value);

impl FormatArgs for ParamArgs<'_> {
    fn get_index(&self, index: usize) -> Result<Option<Argument<'_>>, ()> {
        match self.0 {
            Value::Array(array) => Ok(array.get(index).map(|v| v as Argument<'_>)),
            _ => Err(()),
        }
    }

    fn get_key(&self, key: &str) -> Result<Option<Argument<'_>>, ()> {
        match self.0 {
            Value::Object(object) => Ok(object.get(key).map(|v| v as Argument<'_>)),
            _ => Err(()),
        }
    }
}

/// A top-level `message` is the legacy spelling of `logentry` and carries the
/// same object shape, so both are read the same way.
fn get_log_message(event_data: &Value, preference: MessagePreference) -> Option<String> {
    let first_line = |msg: &str| msg.lines().next().unwrap_or("").to_string();

    for key in ["logentry", "message"] {
        let Some(entry) = event_data.get(key) else {
            continue;
        };

        if let Some(msg) = entry.as_str() {
            return Some(first_line(msg));
        }

        // Sentry reads these with `or`, where an empty string is falsy and the
        // next option is tried, so an empty field must not shadow a usable one.
        let field = |name: &str| {
            entry
                .get(name)
                .and_then(|m| m.as_str())
                .filter(|m| !m.is_empty())
        };
        let picked = match preference {
            MessagePreference::Grouping => field("message").or_else(|| field("formatted")),
            MessagePreference::Title => {
                if let Some(formatted) = field("formatted") {
                    Some(formatted)
                } else {
                    let rendered = field("message")
                        .zip(entry.get("params"))
                        .and_then(|(template, params)| format_message(template, params));
                    match rendered {
                        Some(rendered) => return Some(first_line(&rendered)),
                        None => field("message"),
                    }
                }
            }
        };

        if let Some(msg) = picked {
            return Some(first_line(msg));
        }
    }

    None
}

/// Gets the transaction from the event
fn get_transaction(event_data: &Value) -> String {
    event_data
        .get("transaction")
        .and_then(|t| t.as_str())
        .map(|s| clamp(s, 200))
        .unwrap_or_else(|| "<no transaction>".to_string())
}

/// Generates the error title
pub fn get_title(calculated_type: &str, calculated_value: &str) -> String {
    if calculated_value.is_empty() {
        calculated_type.to_string()
    } else {
        match calculated_value
            .lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty())
        {
            Some(first_line) => format!("{}: {}", calculated_type, first_line),
            None => calculated_type.to_string(),
        }
    }
}

/// Extracts denormalized fields from the event
pub fn get_denormalized_fields(event_data: &Value) -> DenormalizedFields {
    let (calculated_type, calculated_value) = get_type_and_value(event_data);

    // Try to get the last frame from the stacktrace
    let (filename, module, function) = get_last_frame_info(event_data);

    let transaction = get_transaction(event_data);
    let culprit = get_culprit(&function, &filename, &transaction);
    // Limits match Relay's exactly (relay-event-schema/src/protocol/event.rs
    // @97f9c4b): `logger` max_chars = 64, `release` max_chars = 200.
    let logger = event_data
        .get("logger")
        .and_then(|l| l.as_str())
        .map(|s| clamp(s, 64))
        .unwrap_or_default();
    let release = event_data
        .get("release")
        .and_then(|r| r.as_str())
        .map(|s| clamp(s, 200))
        .unwrap_or_default();

    DenormalizedFields {
        calculated_type,
        calculated_value,
        transaction,
        last_frame_filename: filename,
        last_frame_module: module,
        last_frame_function: function,
        culprit,
        logger,
        release,
    }
}

/// Derives the issue culprit — a short string identifying the error source.
/// Prefers the relevant frame's function, then filename, then the transaction.
fn get_culprit(function: &str, filename: &str, transaction: &str) -> String {
    let chosen = if !function.is_empty() {
        function
    } else if !filename.is_empty() {
        filename
    } else if transaction != "<no transaction>" {
        transaction
    } else {
        ""
    };
    clamp(chosen, 255)
}

/// Denormalized fields extracted from event data
#[derive(Debug, Clone)]
pub struct DenormalizedFields {
    pub calculated_type: String,
    pub calculated_value: String,
    pub transaction: String,
    pub last_frame_filename: String,
    pub last_frame_module: String,
    pub last_frame_function: String,
    pub culprit: String,
    pub logger: String,
    pub release: String,
}

/// Extracts information from the last stacktrace frame
fn get_last_frame_info(event_data: &Value) -> (String, String, String) {
    let exception = match get_main_exception(event_data) {
        Some(e) => e,
        None => return (String::new(), String::new(), String::new()),
    };

    let frames = exception
        .get("stacktrace")
        .and_then(|st| st.get("frames"))
        .and_then(|f| f.as_array());

    let frames = match frames {
        Some(f) if !f.is_empty() => f,
        _ => return (String::new(), String::new(), String::new()),
    };

    // Find the last "in_app" frame or the last frame
    let frame = frames
        .iter()
        .rev()
        .find(|f| f.get("in_app").and_then(|v| v.as_bool()).unwrap_or(false))
        .or_else(|| frames.last());

    match frame {
        Some(f) => {
            let filename = f
                .get("filename")
                .and_then(|v| v.as_str())
                .map(|s| clamp(s, 255))
                .unwrap_or_default();

            let module = f
                .get("module")
                .and_then(|v| v.as_str())
                .map(|s| clamp(s, 255))
                .unwrap_or_default();

            let function = f
                .get("function")
                .and_then(|v| v.as_str())
                .map(|s| clamp(s, 255))
                .unwrap_or_default();

            (filename, module, function)
        }
        None => (String::new(), String::new(), String::new()),
    }
}
