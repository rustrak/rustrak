//! `rustrak-plugin.toml`: what a plugin's manifest accepts and refuses.

use rustrak::plugins::inspect::Code;
use rustrak::plugins::manifest::{Auth, Manifest, Scope};
use semver::Version;

const VALID: &str = r#"
[plugin]
id = "analytics"
name = "Analytics"
version = "0.1.0"
rustrak = ">=0.33.0, <0.40.0"
world = "0.1.0"

[[permissions]]
resource = "dashboards"
scope = "project"
actions = ["read", "update"]
"#;

fn parse(toml: &str) -> Manifest {
    Manifest::parse(toml).unwrap_or_else(|e| panic!("should parse: {e}"))
}

/// The code's routes, as the SDK's macros write them.
const ROUTES: &str = r#"[
    {"method":"POST","path":"/collect","auth":"public","scope":"project","requires":null},
    {"method":"GET","path":"/stats","auth":"user","scope":"project","requires":"dashboards:read"}
]"#;

/// `VALID` reading the code whose routes are `routes` (JSON).
fn with_code(routes: &str) -> Result<Manifest, String> {
    let mut m = parse(VALID);
    let code = Code {
        routes: serde_json::from_str(routes).unwrap(),
        sql: true,
    };
    m.read_code(code).map(|()| m).map_err(|e| e.to_string())
}

fn parse_err(toml: &str) -> String {
    match Manifest::parse(toml) {
        Ok(m) => panic!("should be refused, parsed as {m:?}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn reads_a_complete_manifest() {
    let m = with_code(ROUTES).unwrap();
    assert_eq!(m.plugin.id, "analytics");
    assert_eq!(m.plugin.version, Version::new(0, 1, 0));
    assert!(m.capabilities.sql, "the code's");
    assert_eq!(m.capabilities.storage_mb, 512, "default quota");
    assert_eq!(m.routes.len(), 2);
    assert_eq!(m.routes[0].auth, Auth::Public);
    assert_eq!(m.routes[1].scope, Scope::Project);
    assert_eq!(m.routes[1].requires.as_deref(), Some("dashboards:read"));
}

#[test]
fn refuses_an_id_that_is_not_a_slug() {
    for id in [
        "Analytics",
        "1abc",
        "a",
        "has_underscore",
        "x".repeat(33).as_str(),
    ] {
        let toml = VALID.replace(r#"id = "analytics""#, &format!(r#"id = "{id}""#));
        assert!(parse_err(&toml).contains("id"), "{id} should be refused");
    }
}

#[test]
fn refuses_unknown_fields() {
    let toml = format!("{VALID}\n[capabilities]\nnetwork = true\n");
    assert!(parse_err(&toml).contains("network"));
}

/// What the code declares is the code's: a manifest naming it is refused.
#[test]
fn refuses_routes_or_a_database_written_in_the_manifest() {
    let routes = format!(
        "{VALID}\n[[routes]]\nmethod = \"GET\"\npath = \"/x\"\nauth = \"public\"\nscope = \"project\"\n"
    );
    assert!(parse_err(&routes).contains("routes"));
    let sql = format!("{VALID}\n[capabilities]\nsql = true\n");
    assert!(parse_err(&sql).contains("sql"));
}

#[test]
fn refuses_a_route_path_without_a_leading_slash() {
    let err = with_code(&ROUTES.replace(r#""/collect""#, r#""collect""#)).unwrap_err();
    assert!(err.contains("collect"), "{err}");
}

#[test]
fn refuses_two_routes_with_the_same_method_and_path() {
    let routes = ROUTES
        .replace(r#""/stats""#, r#""/collect""#)
        .replace(r#""GET""#, r#""POST""#);
    assert!(with_code(&routes).unwrap_err().contains("POST /collect"));
}

#[test]
fn refuses_a_requirement_the_plugin_does_not_declare() {
    let err = with_code(&ROUTES.replace("dashboards:read", "dashboards:delete")).unwrap_err();
    assert!(err.contains("dashboards:delete"), "{err}");
}

#[test]
fn refuses_a_requirement_on_a_public_route() {
    // Nobody is signed in on a public route, so a requirement there would
    // never be checked: refused rather than silently ignored.
    let routes = ROUTES.replace(
        r#""auth":"public","scope":"project","requires":null"#,
        r#""auth":"public","scope":"project","requires":"dashboards:read""#,
    );
    let err = with_code(&routes).unwrap_err();
    assert!(err.contains("POST /collect"), "{err}");
    assert!(err.contains("public"), "{err}");
}

#[test]
fn checks_the_host_version_against_the_declared_range() {
    let m = parse(VALID);
    assert!(m.supports_host(&Version::new(0, 33, 0)).is_ok());
    let err = m.supports_host(&Version::new(0, 40, 0)).unwrap_err();
    assert!(err.to_string().contains("0.40.0"));
}

#[test]
fn refuses_a_world_the_host_does_not_implement() {
    let toml = VALID.replace(r#"world = "0.1.0""#, r#"world = "9.0.0""#);
    assert!(parse_err(&toml).contains("9.0.0"));
}

const SETTINGS: &str = r#"
[[settings]]
key = "greeting"
type = "text"
scope = "instance"
required = false
default = "Hello"

[[settings]]
key = "retentionDays"
type = "number"
scope = "project"
required = false
default = 30
min = 1
integer = true
"#;

#[test]
fn reads_the_settings_schema() {
    let m = parse(&format!("{VALID}{SETTINGS}"));
    let keys: Vec<_> = m.settings.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["greeting", "retentionDays"]);
    assert_eq!(m.settings[1].default, Some(serde_json::json!(30)));
}

#[test]
fn refuses_a_settings_schema_with_a_mistake() {
    let toml = format!("{VALID}{}", SETTINGS.replace("default = 30", "default = 0"));
    assert!(parse_err(&toml).contains("settings: retentionDays: the default is not valid"));
}

#[test]
fn refuses_a_page_outside_a_project_or_the_instance() {
    let toml = format!("{VALID}\n[ui]\npages = [{{ path = \"\", scope = \"settings\" }}]\n");
    assert!(parse_err(&toml).contains("unknown variant"));
}

/// A page that needs one of the plugin's permissions: shown, and listed in
/// the sidebar, only to whoever holds it.
#[test]
fn a_page_requires_one_of_the_plugins_permissions() {
    let ok = format!(
        "{VALID}\n[ui]\npages = [{{ path = \"boards\", scope = \"project\", requires = \"dashboards:read\" }}]\n"
    );
    let manifest = parse(&ok);
    assert_eq!(
        manifest.ui.unwrap().pages[0].requires.as_deref(),
        Some("dashboards:read")
    );
    let unknown = format!(
        "{VALID}\n[ui]\npages = [{{ path = \"boards\", scope = \"project\", requires = \"visitors:read\" }}]\n"
    );
    assert!(parse_err(&unknown).contains("visitors:read"));
}
