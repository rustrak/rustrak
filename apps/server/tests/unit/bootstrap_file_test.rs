//! `rustrak.toml`: what the declarative bootstrap file accepts and refuses.
//! Everything here is decided before the server touches the database.

use rustrak::bootstrap::file::BootstrapFile;
use rustrak::models::Product;
use uuid::Uuid;

const KEY: &str = "0123456789abcdef0123456789abcdef";
const TOKEN: &str = "0123456789abcdef0123456789abcdef01234567";

fn parse(toml: &str) -> BootstrapFile {
    BootstrapFile::parse(toml).unwrap_or_else(|e| panic!("should parse: {e}"))
}

fn parse_err(toml: &str) -> String {
    match BootstrapFile::parse(toml) {
        Ok(f) => panic!("should be refused, parsed as {f:?}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn reads_a_project_with_its_fixed_id_and_key() {
    let file = parse(&format!(
        r#"
        [[bootstrap.projects]]
        id = 7
        name = "My App"
        slug = "my-app"
        sentry_key = "{KEY}"
        platform = "python"
        disabled_products = ["logs", "crons"]
        "#
    ));

    assert_eq!(file.projects.len(), 1);
    let p = &file.projects[0];
    assert_eq!(p.id, 7);
    assert_eq!(p.name, "My App");
    assert_eq!(p.slug.as_deref(), Some("my-app"));
    assert_eq!(p.sentry_key, Some(Uuid::parse_str(KEY).unwrap()));
    assert_eq!(p.platform.as_deref(), Some("python"));
    assert_eq!(
        p.disabled_products,
        Some(vec![Product::Logs, Product::Crons])
    );
}

#[test]
fn refuses_a_misspelled_field_instead_of_ignoring_it() {
    let err = parse_err(
        r#"
        [[bootstrap.projects]]
        id = 1
        name = "App"
        sentry_kye = "0123456789abcdef0123456789abcdef"
        "#,
    );
    assert!(err.contains("sentry_kye"), "{err}");
}

#[test]
fn refuses_a_section_it_does_not_know() {
    let err = parse_err("[server]\nport = 8080\n");
    assert!(err.contains("server"), "{err}");
}

fn project(id: i32, name: &str, slug: &str, key: &str) -> String {
    format!(
        "[[bootstrap.projects]]\nid = {id}\nname = \"{name}\"\nslug = \"{slug}\"\nsentry_key = \"{key}\"\n"
    )
}

const OTHER_KEY: &str = "fedcba9876543210fedcba9876543210";

#[test]
fn refuses_a_project_id_that_is_not_positive() {
    for id in [0, -3] {
        let err = parse_err(&project(id, "App", "app", KEY));
        assert!(err.contains("id"), "{err}");
    }
}

#[test]
fn refuses_two_projects_with_the_same_id() {
    let err = parse_err(&format!(
        "{}{}",
        project(1, "A", "a", KEY),
        project(1, "B", "b", OTHER_KEY)
    ));
    assert!(err.contains("id 1"), "{err}");
}

#[test]
fn refuses_two_projects_with_the_same_sentry_key() {
    let err = parse_err(&format!(
        "{}{}",
        project(1, "A", "a", KEY),
        project(2, "B", "b", KEY)
    ));
    assert!(err.contains("sentry_key"), "{err}");
}

#[test]
fn refuses_two_projects_with_the_same_slug() {
    let err = parse_err(&format!(
        "{}{}",
        project(1, "A", "app", KEY),
        project(2, "B", "app", OTHER_KEY)
    ));
    assert!(err.contains("slug \"app\""), "{err}");
}

#[test]
fn refuses_two_projects_with_the_same_name() {
    let err = parse_err(&format!(
        "{}{}",
        project(1, "App", "a", KEY),
        project(2, "App", "b", OTHER_KEY)
    ));
    assert!(err.contains("name \"App\""), "{err}");
}

#[test]
fn refuses_a_blank_project_name() {
    let err = parse_err(&project(1, "  ", "a", KEY));
    assert!(err.contains("name"), "{err}");
}

#[test]
fn accepts_a_sentry_key_with_or_without_hyphens_and_refuses_anything_else() {
    let dashed = "01234567-89ab-cdef-0123-456789abcdef";
    let file = parse(&project(1, "A", "a", dashed));
    assert_eq!(
        file.projects[0].sentry_key,
        Some(Uuid::parse_str(KEY).unwrap())
    );

    let err = parse_err(&project(1, "A", "a", "not-a-key"));
    assert!(err.contains("sentry_key"), "{err}");
}

/// A secret written to a file the way Docker and Kubernetes mount one,
/// trailing newline included.
fn secret_file(value: &str) -> tempfile::NamedTempFile {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), format!("{value}\n")).unwrap();
    f
}

#[test]
fn reads_a_token_given_inline() {
    let file = parse(&format!(
        "[[bootstrap.tokens]]\nname = \"ci\"\ntoken = \"{TOKEN}\"\ndescription = \"CI deploys\"\n"
    ));
    assert_eq!(file.tokens.len(), 1);
    assert_eq!(file.tokens[0].name, "ci");
    assert_eq!(file.tokens[0].token, TOKEN);
    assert_eq!(file.tokens[0].description.as_deref(), Some("CI deploys"));
}

#[test]
fn reads_a_token_from_a_secret_file_without_its_trailing_newline() {
    let secret = secret_file(TOKEN);
    let file = parse(&format!(
        "[[bootstrap.tokens]]\nname = \"ci\"\ntoken_file = {:?}\n",
        secret.path()
    ));
    assert_eq!(file.tokens[0].token, TOKEN);
}

#[test]
fn refuses_a_token_with_both_or_neither_value_and_file() {
    let secret = secret_file(TOKEN);
    let both = parse_err(&format!(
        "[[bootstrap.tokens]]\nname = \"ci\"\ntoken = \"{TOKEN}\"\ntoken_file = {:?}\n",
        secret.path()
    ));
    assert!(both.contains("ci") && both.contains("token_file"), "{both}");

    let neither = parse_err("[[bootstrap.tokens]]\nname = \"ci\"\n");
    assert!(
        neither.contains("ci") && neither.contains("token"),
        "{neither}"
    );
}

#[test]
fn refuses_a_token_file_that_cannot_be_read() {
    let err = parse_err(
        "[[bootstrap.tokens]]\nname = \"ci\"\ntoken_file = \"/nonexistent/rustrak-token\"\n",
    );
    assert!(err.contains("/nonexistent/rustrak-token"), "{err}");
}

#[test]
fn refuses_a_malformed_token_without_echoing_it() {
    for bad in [
        "0123456789ABCDEF0123456789ABCDEF01234567", // uppercase
        "0123456789abcdef",                         // too short
        "zz23456789abcdef0123456789abcdef01234567", // not hex
    ] {
        let err = parse_err(&format!(
            "[[bootstrap.tokens]]\nname = \"ci\"\ntoken = \"{bad}\"\n"
        ));
        assert!(err.contains("40 lowercase hex"), "{err}");
        assert!(!err.contains(bad), "the error leaks the secret: {err}");
    }
}

#[test]
fn refuses_two_tokens_with_the_same_name_or_the_same_value() {
    const OTHER_TOKEN: &str = "fedcba9876543210fedcba9876543210fedcba98";
    let same_name = parse_err(&format!(
        "[[bootstrap.tokens]]\nname = \"ci\"\ntoken = \"{TOKEN}\"\n\
         [[bootstrap.tokens]]\nname = \"ci\"\ntoken = \"{OTHER_TOKEN}\"\n"
    ));
    assert!(same_name.contains("name \"ci\""), "{same_name}");

    let same_value = parse_err(&format!(
        "[[bootstrap.tokens]]\nname = \"ci\"\ntoken = \"{TOKEN}\"\n\
         [[bootstrap.tokens]]\nname = \"deploy\"\ntoken = \"{TOKEN}\"\n"
    ));
    assert!(same_value.contains("deploy"), "{same_value}");
    assert!(
        !same_value.contains(TOKEN),
        "the error leaks the secret: {same_value}"
    );
}

#[test]
fn refuses_a_blank_token_name() {
    let err = parse_err(&format!(
        "[[bootstrap.tokens]]\nname = \" \"\ntoken = \"{TOKEN}\"\n"
    ));
    assert!(err.contains("name"), "{err}");
}

#[test]
fn never_prints_a_secret_in_debug_output() {
    let file = parse(&format!(
        "[bootstrap.superuser]\nemail = \"admin@example.com\"\npassword = \"hunter2-secret\"\n\
         [[bootstrap.tokens]]\nname = \"ci\"\ntoken = \"{TOKEN}\"\n"
    ));
    let debug = format!("{file:?}");
    assert!(!debug.contains(TOKEN), "{debug}");
    assert!(!debug.contains("hunter2-secret"), "{debug}");
}

#[test]
fn reads_the_superuser_with_an_inline_or_file_password() {
    let file = parse(
        "[bootstrap.superuser]\nemail = \"admin@example.com\"\npassword = \"s3cret:with:colons\"\n",
    );
    let su = file.superuser.expect("superuser");
    assert_eq!(su.email, "admin@example.com");
    assert_eq!(su.password, "s3cret:with:colons");

    let secret = secret_file("from-a-file");
    let file = parse(&format!(
        "[bootstrap.superuser]\nemail = \"admin@example.com\"\npassword_file = {:?}\n",
        secret.path()
    ));
    assert_eq!(file.superuser.unwrap().password, "from-a-file");
}

#[test]
fn refuses_a_superuser_without_a_password() {
    let err = parse_err("[bootstrap.superuser]\nemail = \"admin@example.com\"\npassword = \"\"\n");
    assert!(err.contains("password"), "{err}");
    let err = parse_err("[bootstrap.superuser]\nemail = \"admin@example.com\"\n");
    assert!(err.contains("password"), "{err}");
}

#[test]
fn a_syntax_error_names_the_line_without_echoing_a_secret_on_it() {
    let err = parse_err(&format!(
        "[[bootstrap.tokens]]\nname = \"ci\"\ntokn = \"{TOKEN}\"\n"
    ));
    assert!(err.contains("tokn"), "{err}");
    assert!(err.contains("line 3"), "{err}");
    assert!(!err.contains(TOKEN), "the error leaks the secret: {err}");
}

#[test]
fn refuses_a_project_that_disables_issues() {
    let err = parse_err(&format!(
        "{}disabled_products = [\"issues\"]\n",
        project(1, "App", "app", KEY)
    ));
    assert!(err.contains("id 1") && err.contains("issues"), "{err}");
}

#[test]
fn refuses_a_platform_it_does_not_know() {
    let err = parse_err(&format!(
        "{}platform = \"cobol\"\n",
        project(1, "App", "app", KEY)
    ));
    assert!(err.contains("id 1") && err.contains("cobol"), "{err}");
}

#[test]
fn refuses_a_declared_slug_that_slugifies_to_nothing() {
    // Created once with a slug derived from the name, then refused by every
    // later start: caught before the first.
    for slug in ["", "  ", "---"] {
        let err = parse_err(&project(1, "App", slug, KEY));
        assert!(
            err.contains("id 1") && err.contains("slug"),
            "{slug:?}: {err}"
        );
    }
}

#[test]
fn refuses_a_project_name_over_255_bytes_and_accepts_255() {
    let fits = "a".repeat(255);
    parse(&project(1, &fits, "a", KEY));

    let err = parse_err(&project(1, &"a".repeat(256), "a", KEY));
    assert!(err.contains("id 1") && err.contains("255"), "{err}");
}

#[test]
fn a_value_of_the_wrong_type_is_not_echoed_in_the_error() {
    let err =
        parse_err("[bootstrap.superuser]\nemail = \"admin@example.com\"\npassword = 987654321\n");
    assert!(err.contains("line 3") && err.contains("password"), "{err}");
    assert!(err.contains("expected a string"), "{err}");
    assert!(
        !err.contains("987654321"),
        "the error leaks the secret: {err}"
    );
}

#[test]
fn refuses_a_token_name_over_255_bytes_and_accepts_255() {
    let fits = "t".repeat(255);
    parse(&format!(
        "[[bootstrap.tokens]]\nname = \"{fits}\"\ntoken = \"{TOKEN}\"\n"
    ));

    let long = "t".repeat(256);
    let err = parse_err(&format!(
        "[[bootstrap.tokens]]\nname = \"{long}\"\ntoken = \"{TOKEN}\"\n"
    ));
    assert!(err.contains("255"), "{err}");
}
