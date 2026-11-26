//! A plugin's settings: the schema it declares in `rustrak.config.ts` (the
//! manifest's `[[settings]]`), the values an admin writes through the form
//! Rustrak draws for every plugin, and what each side reads. The plugin gets
//! everything, secrets included; the browser never gets a secret.

use serde::{Deserialize, Serialize};

pub use crate::models::rbac::Scope;
use serde_json::{Map, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum SettingType {
    Text,
    Url,
    Secret,
    Number,
    Boolean,
    Select,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct SettingSpec {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: SettingType,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// A string, a number or a boolean, as the type says.
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integer: Option<bool>,
}

fn camel(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && chars.all(|c| c.is_ascii_alphanumeric())
}

/// Why `value` is not a valid value of `spec`, if it is not.
fn invalid(spec: &SettingSpec, value: &Value) -> Option<String> {
    match spec.kind {
        SettingType::Text | SettingType::Secret => {
            (!value.is_string()).then(|| "must be text".into())
        }
        SettingType::Url => match value.as_str() {
            Some(s) if s.starts_with("http://") || s.starts_with("https://") => None,
            _ => Some("must be an http(s) address".into()),
        },
        SettingType::Boolean => (!value.is_boolean()).then(|| "must be true or false".into()),
        SettingType::Select => {
            let options = spec.options.as_deref().unwrap_or_default();
            match value.as_str() {
                Some(s) if options.iter().any(|o| o == s) => None,
                _ => Some(format!("must be one of {}", options.join(", "))),
            }
        }
        SettingType::Number => {
            let Some(n) = value.as_f64() else {
                return Some("must be a number".into());
            };
            if spec.integer == Some(true) && n.fract() != 0.0 {
                return Some("must be a whole number".into());
            }
            if let Some(min) = spec.min.filter(|min| n < *min) {
                return Some(format!("must be at least {min}"));
            }
            spec.max
                .filter(|max| n > *max)
                .map(|max| format!("must be at most {max}"))
        }
    }
}

/// Checks a manifest's schema: the first problem, as `key: why`.
pub fn validate(specs: &[SettingSpec]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for s in specs {
        let fail = |why: &str| Err(format!("{}: {why}", s.key));
        if !seen.insert(s.key.as_str()) {
            return fail("declared twice");
        }
        if !camel(&s.key) {
            return fail("a key is camelCase");
        }
        if s.kind == SettingType::Secret {
            if s.default.is_some() {
                return fail("a secret has no default");
            }
            if s.scope == Scope::Project {
                return fail("a secret belongs to the instance");
            }
        }
        if s.kind == SettingType::Select && s.options.as_ref().is_none_or(Vec::is_empty) {
            return fail("a select needs options");
        }
        if matches!((s.min, s.max), (Some(min), Some(max)) if min > max) {
            return fail("min is above max");
        }
        if s.default.as_ref().is_some_and(|d| invalid(s, d).is_some()) {
            return fail("the default is not valid");
        }
    }
    Ok(())
}

/// Writes `patch` into the values set at `level`: a value sets, `null`
/// clears. Every problem comes back as `(key, why)`, and then nothing is
/// written.
pub fn apply(
    specs: &[SettingSpec],
    level: Scope,
    values: &mut Map<String, Value>,
    patch: Map<String, Value>,
) -> Result<(), Vec<(String, String)>> {
    let mut errors = Vec::new();
    for (key, value) in &patch {
        let why = match specs.iter().find(|s| &s.key == key) {
            None => Some("not a setting of this plugin".to_string()),
            Some(s) if level == Scope::Project && s.scope == Scope::Instance => {
                Some("is set for the whole instance".to_string())
            }
            Some(_) if value.is_null() => None,
            Some(s) => invalid(s, value),
        };
        if let Some(why) = why {
            errors.push((key.clone(), why));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    for (key, value) in patch {
        if value.is_null() {
            values.remove(&key);
        } else {
            values.insert(key, value);
        }
    }
    Ok(())
}

/// What the plugin reads: for each setting, the project's value, then the
/// instance's, then the default, then `null`.
pub fn resolve(
    specs: &[SettingSpec],
    instance: &Map<String, Value>,
    project: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    specs
        .iter()
        .map(|s| {
            let own = project
                .filter(|_| s.scope == Scope::Project)
                .and_then(|p| p.get(&s.key));
            let value = own
                .or_else(|| instance.get(&s.key))
                .or(s.default.as_ref())
                .cloned()
                .unwrap_or(Value::Null);
            (s.key.clone(), value)
        })
        .collect()
}

/// `values` without its secrets, nor anything the schema no longer declares
/// (a secret of an older version, say): what may reach a browser.
pub fn public(specs: &[SettingSpec], mut values: Map<String, Value>) -> Map<String, Value> {
    values.retain(|key, _| {
        specs
            .iter()
            .any(|s| &s.key == key && s.kind != SettingType::Secret)
    });
    values
}

/// The required settings nothing sets yet, among `resolved` (what
/// [`resolve`] gives): the plugin cannot run without them.
pub fn missing(specs: &[SettingSpec], resolved: &Map<String, Value>) -> Vec<String> {
    specs
        .iter()
        .filter(|s| s.required && resolved.get(&s.key).is_none_or(Value::is_null))
        .map(|s| s.key.clone())
        .collect()
}
