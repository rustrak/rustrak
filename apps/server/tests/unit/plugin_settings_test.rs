//! A plugin's settings on the host: the schema from its manifest, the values
//! an admin writes, and what the plugin and the browser each get to read.

use rustrak::plugins::settings::{self, Scope, SettingSpec};
use serde_json::{json, Map, Value};

fn specs() -> Vec<SettingSpec> {
    serde_json::from_value(json!([
        { "key": "greeting", "type": "text", "default": "Hello" },
        { "key": "apiKey", "type": "secret", "required": true },
        { "key": "retentionDays", "type": "number", "scope": "project",
          "integer": true, "default": 30, "min": 1, "max": 365 },
        { "key": "enabled", "type": "boolean", "scope": "project" },
        { "key": "mode", "type": "select", "options": ["simple", "full"], "default": "simple" },
        { "key": "endpoint", "type": "url" },
    ]))
    .unwrap()
}

fn map(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn a_well_formed_schema_is_accepted() {
    assert_eq!(settings::validate(&specs()), Ok(()));
}

#[test]
fn a_schema_with_a_mistake_is_refused() {
    for (bad, why) in [
        (
            json!([{ "key": "a", "type": "text" }, { "key": "a", "type": "text" }]),
            "a: declared twice",
        ),
        (
            json!([{ "key": "Api-Key", "type": "text" }]),
            "Api-Key: a key is camelCase",
        ),
        (
            json!([{ "key": "a", "type": "select", "options": [] }]),
            "a: a select needs options",
        ),
        (
            json!([{ "key": "a", "type": "select", "options": ["x"], "default": "y" }]),
            "a: the default is not valid",
        ),
        (
            json!([{ "key": "a", "type": "secret", "default": "x" }]),
            "a: a secret has no default",
        ),
        (
            json!([{ "key": "a", "type": "secret", "scope": "project" }]),
            "a: a secret belongs to the instance",
        ),
        (
            json!([{ "key": "a", "type": "number", "min": 3, "max": 1 }]),
            "a: min is above max",
        ),
    ] {
        let specs: Vec<SettingSpec> = serde_json::from_value(bad).unwrap();
        assert_eq!(settings::validate(&specs), Err(why.to_string()));
    }
}

#[test]
fn the_plugin_reads_the_project_then_the_instance_then_the_default() {
    let specs = specs();
    let instance = map(json!({ "retentionDays": 90, "apiKey": "k" }));
    let project = map(json!({ "retentionDays": 7 }));

    let everywhere = settings::resolve(&specs, &instance, None);
    assert_eq!(everywhere["greeting"], "Hello");
    assert_eq!(everywhere["retentionDays"], 90);
    assert_eq!(everywhere["apiKey"], "k");
    assert_eq!(everywhere["endpoint"], Value::Null);

    let here = settings::resolve(&specs, &instance, Some(&project));
    assert_eq!(here["retentionDays"], 7);
    assert_eq!(here["mode"], "simple");
}

#[test]
fn the_browser_never_gets_a_secret() {
    let specs = specs();
    let resolved = settings::resolve(&specs, &map(json!({ "apiKey": "k" })), None);
    let public = settings::public(&specs, resolved);
    assert!(!public.contains_key("apiKey"));
    assert_eq!(public["greeting"], "Hello");
}

#[test]
fn a_value_the_schema_no_longer_declares_never_reaches_the_browser() {
    // A secret an older version declared, still stored after an upgrade.
    let stored = map(json!({ "greeting": "Hola", "apiKey": "k", "oldToken": "leak" }));
    let public = settings::public(&specs(), stored);
    assert_eq!(public, map(json!({ "greeting": "Hola" })));
}

#[test]
fn a_change_is_checked_against_the_schema() {
    let specs = specs();
    let mut values = Map::new();
    settings::apply(
        &specs,
        Scope::Instance,
        &mut values,
        map(json!({ "greeting": "Hola", "apiKey": "secret", "retentionDays": 60, "mode": "full" })),
    )
    .unwrap();
    assert_eq!(values["greeting"], "Hola");
    assert_eq!(values["retentionDays"], 60);

    for (patch, field, why) in [
        (json!({ "nope": 1 }), "nope", "not a setting of this plugin"),
        (json!({ "greeting": 3 }), "greeting", "must be text"),
        (
            json!({ "retentionDays": 1.5 }),
            "retentionDays",
            "must be a whole number",
        ),
        (
            json!({ "retentionDays": 0 }),
            "retentionDays",
            "must be at least 1",
        ),
        (
            json!({ "retentionDays": 400 }),
            "retentionDays",
            "must be at most 365",
        ),
        (
            json!({ "mode": "turbo" }),
            "mode",
            "must be one of simple, full",
        ),
        (
            json!({ "enabled": "yes" }),
            "enabled",
            "must be true or false",
        ),
        (
            json!({ "endpoint": "ftp//x" }),
            "endpoint",
            "must be an http(s) address",
        ),
    ] {
        let err =
            settings::apply(&specs, Scope::Instance, &mut values.clone(), map(patch)).unwrap_err();
        assert_eq!(err, vec![(field.to_string(), why.to_string())]);
    }
}

#[test]
fn a_project_only_sets_project_settings() {
    let specs = specs();
    let mut values = Map::new();
    let err = settings::apply(
        &specs,
        Scope::Project,
        &mut values,
        map(json!({ "greeting": "x" })),
    )
    .unwrap_err();
    assert_eq!(
        err,
        vec![(
            "greeting".to_string(),
            "is set for the whole instance".to_string()
        )]
    );
    settings::apply(
        &specs,
        Scope::Project,
        &mut values,
        map(json!({ "enabled": true })),
    )
    .unwrap();
    assert_eq!(values["enabled"], true);
}

#[test]
fn null_clears_a_value_and_the_default_comes_back() {
    let specs = specs();
    let mut values = map(json!({ "greeting": "Hola" }));
    settings::apply(
        &specs,
        Scope::Instance,
        &mut values,
        map(json!({ "greeting": null })),
    )
    .unwrap();
    assert!(!values.contains_key("greeting"));
    assert_eq!(
        settings::resolve(&specs, &values, None)["greeting"],
        "Hello"
    );
}

#[test]
fn missing_required_settings_are_named() {
    let specs = specs();
    assert_eq!(
        settings::missing(&specs, &settings::resolve(&specs, &Map::new(), None)),
        vec!["apiKey".to_string()]
    );
    let set = map(json!({ "apiKey": "k" }));
    assert!(settings::missing(&specs, &settings::resolve(&specs, &set, None)).is_empty());
}

#[test]
fn a_required_setting_a_project_sets_is_not_missing_there() {
    let specs: Vec<SettingSpec> = serde_json::from_value(json!([
        { "key": "siteId", "type": "text", "scope": "project", "required": true },
    ]))
    .unwrap();
    let project = map(json!({ "siteId": "s1" }));
    let resolved = settings::resolve(&specs, &Map::new(), Some(&project));
    assert!(settings::missing(&specs, &resolved).is_empty());
    let elsewhere = settings::resolve(&specs, &Map::new(), None);
    assert_eq!(settings::missing(&specs, &elsewhere), ["siteId"]);
}
