//! `rustrak.toml` applied to a database: what a restart creates, reconciles,
//! refuses and leaves alone.

use rustrak::bootstrap::{self, file::BootstrapFile};
use rustrak::models::{CreateProject, Product, UpdateProject};
use rustrak::services::ProjectService;
use uuid::Uuid;

use crate::common::TestDb;

const KEY: &str = "0123456789abcdef0123456789abcdef";
const OTHER_KEY: &str = "fedcba9876543210fedcba9876543210";

fn file(toml: &str) -> BootstrapFile {
    BootstrapFile::parse(toml).unwrap_or_else(|e| panic!("fixture should parse: {e}"))
}

fn project(id: i32, name: &str, slug: &str, key: &str) -> String {
    format!(
        "[[bootstrap.projects]]\nid = {id}\nname = \"{name}\"\nslug = \"{slug}\"\nsentry_key = \"{key}\"\n"
    )
}

async fn apply(db: &TestDb, toml: &str) {
    bootstrap::apply(&db.pool, &file(toml))
        .await
        .unwrap_or_else(|e| panic!("should apply: {e}"));
}

async fn apply_err(db: &TestDb, toml: &str) -> String {
    match bootstrap::apply(&db.pool, &file(toml)).await {
        Ok(()) => panic!("should be refused"),
        Err(e) => e.to_string(),
    }
}

async fn api_project(db: &TestDb, name: &str) -> i32 {
    ProjectService::create(
        &db.pool,
        CreateProject {
            name: name.into(),
            slug: None,
            platform: None,
        },
    )
    .await
    .unwrap()
    .id
}

#[tokio::test]
async fn creates_a_project_whose_dsn_is_known_before_the_server_starts() {
    let db = TestDb::new().await;
    apply(&db, &project(7, "My App", "my-app", KEY)).await;

    let p = ProjectService::get_by_id(&db.pool, 7).await.unwrap();
    assert_eq!(p.name, "My App");
    assert_eq!(p.slug, "my-app");
    assert_eq!(
        p.dsn("https://errors.example.com"),
        format!("https://{KEY}@errors.example.com/7")
    );
}

#[tokio::test]
async fn applying_the_same_file_again_changes_nothing() {
    let db = TestDb::new().await;
    let toml = project(1, "App", "app", KEY);
    apply(&db, &toml).await;
    let before = ProjectService::get_by_id(&db.pool, 1).await.unwrap();

    apply(&db, &toml).await;

    let after = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    assert_eq!(ProjectService::list(&db.pool).await.unwrap().len(), 1);
    assert_eq!(
        after.updated_at, before.updated_at,
        "a no-op restart wrote the row"
    );
}

#[tokio::test]
async fn a_renamed_project_is_renamed_on_restart_and_keeps_its_dsn() {
    let db = TestDb::new().await;
    apply(&db, &project(1, "App", "app", KEY)).await;

    apply(&db, &project(1, "Storefront", "storefront", KEY)).await;

    let p = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    assert_eq!(
        (p.name.as_str(), p.slug.as_str()),
        ("Storefront", "storefront")
    );
    assert_eq!(p.sentry_key, Uuid::parse_str(KEY).unwrap());
}

#[tokio::test]
async fn refuses_a_key_that_differs_from_the_stored_one_and_changes_nothing() {
    let db = TestDb::new().await;
    apply(&db, &project(1, "App", "app", KEY)).await;

    let err = apply_err(&db, &project(1, "Renamed", "renamed", OTHER_KEY)).await;

    assert!(
        err.contains("project 1") && err.contains("sentry_key"),
        "{err}"
    );
    let p = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    assert_eq!(p.name, "App", "the rename went through before the refusal");
    assert_eq!(p.sentry_key, Uuid::parse_str(KEY).unwrap());
}

#[tokio::test]
async fn refuses_an_id_already_held_by_a_project_with_another_key() {
    let db = TestDb::new().await;
    let id = api_project(&db, "Made in the dashboard").await;

    let err = apply_err(&db, &project(id, "App", "app", KEY)).await;

    assert!(err.contains(&format!("project {id}")), "{err}");
    let p = ProjectService::get_by_id(&db.pool, id).await.unwrap();
    assert_eq!(p.name, "Made in the dashboard");
}

#[tokio::test]
async fn a_project_without_a_declared_key_keeps_the_random_one_it_was_given() {
    let db = TestDb::new().await;
    let toml = "[[bootstrap.projects]]\nid = 3\nname = \"App\"\n";
    apply(&db, toml).await;
    let first = ProjectService::get_by_id(&db.pool, 3).await.unwrap();

    apply(&db, toml).await;

    let second = ProjectService::get_by_id(&db.pool, 3).await.unwrap();
    assert_eq!(first.sentry_key, second.sentry_key);
    assert_eq!(second.slug, "app", "the slug is derived from the name");
}

#[tokio::test]
async fn refuses_a_slug_held_by_another_project() {
    let db = TestDb::new().await;
    api_project(&db, "Shop").await; // slug "shop"

    let err = apply_err(&db, &project(9, "App", "shop", KEY)).await;

    assert!(err.contains("project 9") && err.contains("shop"), "{err}");
    assert!(ProjectService::get_by_id(&db.pool, 9).await.is_err());
}

#[tokio::test]
async fn refuses_a_key_held_by_another_project() {
    let db = TestDb::new().await;
    apply(&db, &project(1, "A", "a", KEY)).await;

    // Project 1 left the file, so nothing checks the two against each other
    // before the database does.
    let err = apply_err(&db, &project(2, "B", "b", KEY)).await;

    assert!(
        err.contains("project 2") && err.contains("sentry_key"),
        "{err}"
    );
    assert!(ProjectService::get_by_id(&db.pool, 2).await.is_err());
}

#[tokio::test]
async fn a_project_created_later_from_the_dashboard_gets_a_free_id() {
    let db = TestDb::new().await;
    apply(&db, &project(5, "App", "app", KEY)).await;

    // On PostgreSQL an explicit id does not move the id sequence: without
    // moving it, this insert collides with project 5 sooner or later.
    for n in 0..6 {
        let id = api_project(&db, &format!("Later {n}")).await;
        assert_ne!(id, 5);
    }
}

#[tokio::test]
async fn a_project_removed_from_the_file_is_kept() {
    let db = TestDb::new().await;
    apply(&db, &project(1, "App", "app", KEY)).await;

    apply(&db, "").await;

    assert!(ProjectService::get_by_id(&db.pool, 1).await.is_ok());
}

#[tokio::test]
async fn platform_and_disabled_products_are_applied_and_reconciled() {
    let db = TestDb::new().await;
    let with = |platform: &str, off: &str| {
        format!(
            "{}platform = \"{platform}\"\ndisabled_products = [{off}]\n",
            project(1, "App", "app", KEY)
        )
    };
    apply(&db, &with("python", "\"logs\"")).await;
    let p = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    assert_eq!(p.platform.as_deref(), Some("python"));
    assert_eq!(p.disabled_products.0, vec![Product::Logs]);

    apply(&db, &with("go", "\"crons\", \"feedback\"")).await;
    let p = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    assert_eq!(p.platform.as_deref(), Some("go"));
    assert_eq!(
        p.disabled_products.0,
        vec![Product::Crons, Product::Feedback]
    );

    apply(&db, &with("go", "")).await;
    let p = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    assert!(
        p.disabled_products.0.is_empty(),
        "an empty list turns them all back on"
    );
}

#[tokio::test]
async fn fields_the_file_leaves_out_keep_what_the_dashboard_set() {
    let db = TestDb::new().await;
    let toml = project(1, "App", "app", KEY);
    apply(&db, &toml).await;
    ProjectService::update(
        &db.pool,
        1,
        UpdateProject {
            platform: Some("ruby".into()),
            disabled_products: Some(vec![Product::Logs]),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    apply(&db, &toml).await;

    let p = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    assert_eq!(p.platform.as_deref(), Some("ruby"));
    assert_eq!(p.disabled_products.0, vec![Product::Logs]);
}

#[tokio::test]
async fn a_project_being_deleted_is_skipped_until_its_deletion_finishes() {
    let db = TestDb::new().await;
    let toml = project(1, "App", "app", KEY);
    apply(&db, &toml).await;
    ProjectService::delete(&db.pool, 1).await.unwrap();

    // Its rows are still being removed: neither revived nor a reason not to start.
    apply(&db, &toml).await;

    assert!(ProjectService::get_by_id(&db.pool, 1).await.is_err());
}

// ── Tokens ──────────────────────────────────────────────────────────────────

use rustrak::models::CreateAuthToken;
use rustrak::services::AuthTokenService;

const TOKEN: &str = "0123456789abcdef0123456789abcdef01234567";
const NEW_TOKEN: &str = "fedcba9876543210fedcba9876543210fedcba98";

fn token(name: &str, value: &str, description: &str) -> String {
    format!(
        "[[bootstrap.tokens]]\nname = \"{name}\"\ntoken = \"{value}\"\ndescription = \"{description}\"\n"
    )
}

#[tokio::test]
async fn a_declared_token_authenticates_as_an_instance_token() {
    let db = TestDb::new().await;
    apply(&db, &token("ci", TOKEN, "CI deploys")).await;

    let t = AuthTokenService::get_by_token(&db.pool, TOKEN)
        .await
        .unwrap()
        .expect("the declared token authenticates");
    assert_eq!(t.description.as_deref(), Some("CI deploys"));
    assert_eq!(
        t.user_id, None,
        "an instance token, like RUSTRAK_BOOTSTRAP_TOKEN's"
    );
}

#[tokio::test]
async fn a_restart_keeps_one_token_per_name() {
    let db = TestDb::new().await;
    let toml = token("ci", TOKEN, "CI");
    apply(&db, &toml).await;
    let first = AuthTokenService::get_by_token(&db.pool, TOKEN)
        .await
        .unwrap()
        .unwrap();

    apply(&db, &toml).await;

    let tokens = AuthTokenService::list(&db.pool).await.unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].id, first.id);
}

#[tokio::test]
async fn a_new_value_for_a_name_rotates_the_token() {
    let db = TestDb::new().await;
    apply(&db, &token("ci", TOKEN, "CI")).await;
    let before = AuthTokenService::get_by_token(&db.pool, TOKEN)
        .await
        .unwrap()
        .unwrap();

    apply(&db, &token("ci", NEW_TOKEN, "CI, rotated")).await;

    assert!(
        AuthTokenService::get_by_token(&db.pool, TOKEN)
            .await
            .unwrap()
            .is_none(),
        "the old value still authenticates"
    );
    let after = AuthTokenService::get_by_token(&db.pool, NEW_TOKEN)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.description.as_deref(), Some("CI, rotated"));
}

#[tokio::test]
async fn a_token_removed_from_the_file_is_revoked_and_others_are_not() {
    let db = TestDb::new().await;
    let from_dashboard = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .unwrap();
    apply(
        &db,
        &format!(
            "{}{}",
            token("ci", TOKEN, "CI"),
            token("deploy", NEW_TOKEN, "Deploy")
        ),
    )
    .await;

    apply(&db, &token("deploy", NEW_TOKEN, "Deploy")).await;

    assert!(AuthTokenService::get_by_token(&db.pool, TOKEN)
        .await
        .unwrap()
        .is_none());
    assert!(AuthTokenService::get_by_token(&db.pool, NEW_TOKEN)
        .await
        .unwrap()
        .is_some());
    assert!(
        AuthTokenService::get_by_token(&db.pool, &from_dashboard.token)
            .await
            .unwrap()
            .is_some(),
        "a token the file never declared was revoked"
    );

    apply(&db, "").await;
    assert!(AuthTokenService::get_by_token(&db.pool, NEW_TOKEN)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn refuses_a_value_that_is_already_another_tokens_without_echoing_it() {
    let db = TestDb::new().await;
    let existing = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .unwrap();

    let err = apply_err(&db, &token("ci", &existing.token, "CI")).await;

    assert!(err.contains("\"ci\""), "{err}");
    assert!(
        !err.contains(&existing.token),
        "the error leaks the secret: {err}"
    );
    assert_eq!(AuthTokenService::list(&db.pool).await.unwrap().len(), 1);
}

// ── Superuser ───────────────────────────────────────────────────────────────

use rustrak::models::{CreateUserRequest, UserRole};
use rustrak::services::UsersService;

const SUPERUSER: &str =
    "[bootstrap.superuser]\nemail = \"admin@example.com\"\npassword = \"pass:with:colons\"\n";

#[tokio::test]
async fn creates_the_declared_superuser_on_an_empty_instance() {
    let db = TestDb::new().await;
    apply(&db, SUPERUSER).await;

    let user = UsersService::get_by_email(&db.pool, "admin@example.com")
        .await
        .unwrap()
        .expect("superuser created");
    assert!(user.is_admin());
    assert!(user.verify_password("pass:with:colons").unwrap());

    apply(&db, SUPERUSER).await;
    assert_eq!(UsersService::user_count(&db.pool).await.unwrap(), 1);
}

#[tokio::test]
async fn the_declared_superuser_is_not_created_once_anyone_exists() {
    let db = TestDb::new().await;
    UsersService::create_user(
        &db.pool,
        &CreateUserRequest {
            email: "first@example.com".into(),
            password: "password123".into(),
        },
        UserRole::Admin,
    )
    .await
    .unwrap();

    apply(&db, SUPERUSER).await;

    assert!(UsersService::get_by_email(&db.pool, "admin@example.com")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn refuses_a_superuser_email_the_users_table_would_not_accept() {
    let db = TestDb::new().await;
    let err = apply_err(
        &db,
        "[bootstrap.superuser]\nemail = \"not an email\"\npassword = \"secret-pass\"\n",
    )
    .await;
    assert!(err.contains("superuser"), "{err}");
    assert!(
        !err.contains("secret-pass"),
        "the error leaks the password: {err}"
    );
}

#[tokio::test]
async fn renaming_a_token_keeps_its_value_working() {
    let db = TestDb::new().await;
    apply(&db, &token("ci", TOKEN, "CI")).await;

    // Same value under a new name: the old row must go before the new one
    // takes the value, or every start fails on it.
    apply(&db, &token("deploy", TOKEN, "CI")).await;
    apply(&db, &token("deploy", TOKEN, "CI")).await;

    assert!(AuthTokenService::get_by_token(&db.pool, TOKEN)
        .await
        .unwrap()
        .is_some());
    assert_eq!(AuthTokenService::list(&db.pool).await.unwrap().len(), 1);
}

/// A start that stopped between creating a declared project and moving the id
/// sequence past it (a crash, a failed query) is repaired by the next start.
#[cfg(feature = "postgres")]
#[tokio::test]
async fn a_restart_repairs_an_id_sequence_left_behind_a_declared_project() {
    let db = TestDb::new().await;
    let toml = project(5, "App", "app", KEY);
    apply(&db, &toml).await;
    sqlx::query("SELECT setval(pg_get_serial_sequence('projects', 'id'), 1, false)")
        .execute(&db.pool)
        .await
        .unwrap();

    apply(&db, &toml).await;

    for n in 0..6 {
        assert_ne!(api_project(&db, &format!("Later {n}")).await, 5);
    }
}

#[tokio::test]
async fn two_declared_tokens_can_swap_values() {
    let db = TestDb::new().await;
    apply(
        &db,
        &format!(
            "{}{}",
            token("ci", TOKEN, "CI"),
            token("deploy", NEW_TOKEN, "Deploy")
        ),
    )
    .await;

    // Each new value is still the other row's until both are written.
    apply(
        &db,
        &format!(
            "{}{}",
            token("ci", NEW_TOKEN, "CI"),
            token("deploy", TOKEN, "Deploy")
        ),
    )
    .await;

    let ci = AuthTokenService::get_by_token(&db.pool, NEW_TOKEN)
        .await
        .unwrap()
        .unwrap();
    let deploy = AuthTokenService::get_by_token(&db.pool, TOKEN)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ci.description.as_deref(), Some("CI"));
    assert_eq!(deploy.description.as_deref(), Some("Deploy"));
    assert_eq!(AuthTokenService::list(&db.pool).await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_refused_token_change_leaves_every_declared_token_as_it_was() {
    let db = TestDb::new().await;
    let from_dashboard = AuthTokenService::create(&db.pool, CreateAuthToken::default())
        .await
        .unwrap();
    apply(&db, &token("ci", TOKEN, "CI")).await;

    // "ci" rotates, but "deploy" takes the dashboard token's value: refused,
    // and the rotation must not have happened either.
    let err = apply_err(
        &db,
        &format!(
            "{}{}",
            token("ci", NEW_TOKEN, "CI"),
            token("deploy", &from_dashboard.token, "Deploy")
        ),
    )
    .await;

    assert!(err.contains("\"deploy\""), "{err}");
    assert!(AuthTokenService::get_by_token(&db.pool, TOKEN)
        .await
        .unwrap()
        .is_some());
    assert!(AuthTokenService::get_by_token(&db.pool, NEW_TOKEN)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn two_declared_projects_can_swap_names_and_slugs() {
    let db = TestDb::new().await;
    apply(
        &db,
        &format!(
            "{}{}",
            project(1, "A", "a", KEY),
            project(2, "B", "b", OTHER_KEY)
        ),
    )
    .await;

    apply(
        &db,
        &format!(
            "{}{}",
            project(1, "B", "b", KEY),
            project(2, "A", "a", OTHER_KEY)
        ),
    )
    .await;

    let one = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    let two = ProjectService::get_by_id(&db.pool, 2).await.unwrap();
    assert_eq!((one.name.as_str(), one.slug.as_str()), ("B", "b"));
    assert_eq!((two.name.as_str(), two.slug.as_str()), ("A", "a"));
}

#[tokio::test]
async fn a_new_project_can_take_the_name_an_existing_one_gives_up() {
    let db = TestDb::new().await;
    apply(&db, &project(1, "App", "app", KEY)).await;

    apply(
        &db,
        &format!(
            "{}{}",
            project(2, "App", "app", OTHER_KEY),
            project(1, "Legacy", "legacy", KEY)
        ),
    )
    .await;

    assert_eq!(
        ProjectService::get_by_id(&db.pool, 1).await.unwrap().slug,
        "legacy"
    );
    assert_eq!(
        ProjectService::get_by_id(&db.pool, 2).await.unwrap().slug,
        "app"
    );
}

#[tokio::test]
async fn a_rename_onto_an_undeclared_projects_slug_changes_no_project() {
    let db = TestDb::new().await;
    api_project(&db, "Shop").await; // slug "shop", not in the file
    apply(
        &db,
        &format!(
            "{}{}",
            project(10, "A", "a", KEY),
            project(11, "B", "b", OTHER_KEY)
        ),
    )
    .await;

    let err = apply_err(
        &db,
        &format!(
            "{}{}",
            project(10, "B", "b", KEY),
            project(11, "A", "shop", OTHER_KEY)
        ),
    )
    .await;

    assert!(err.contains("project 11") && err.contains("shop"), "{err}");
    let one = ProjectService::get_by_id(&db.pool, 10).await.unwrap();
    let two = ProjectService::get_by_id(&db.pool, 11).await.unwrap();
    assert_eq!((one.name.as_str(), one.slug.as_str()), ("A", "a"));
    assert_eq!((two.name.as_str(), two.slug.as_str()), ("B", "b"));
}

#[tokio::test]
async fn a_new_projects_taken_key_is_refused_before_anything_is_written() {
    let db = TestDb::new().await;
    apply(&db, &project(1, "A", "a", KEY)).await;
    apply(&db, &project(5, "Taken", "taken", OTHER_KEY)).await;

    // Project 5 left the file; a new project 2 claims its key while project 1
    // is renamed and a first owner is declared.
    let err = apply_err(
        &db,
        &format!(
            "{SUPERUSER}{}{}",
            project(1, "Renamed", "renamed", KEY),
            project(2, "B", "b", OTHER_KEY)
        ),
    )
    .await;

    assert!(
        err.contains("project 2") && err.contains("sentry_key"),
        "{err}"
    );
    let one = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    assert_eq!((one.name.as_str(), one.slug.as_str()), ("A", "a"));
    assert_eq!(UsersService::user_count(&db.pool).await.unwrap(), 0);
}

#[tokio::test]
async fn a_declared_slug_wins_over_one_derived_from_another_projects_name() {
    let db = TestDb::new().await;
    apply(
        &db,
        &format!(
            "[[bootstrap.projects]]\nid = 1\nname = \"My App\"\n{}",
            project(2, "Other", "my-app", KEY)
        ),
    )
    .await;

    let one = ProjectService::get_by_id(&db.pool, 1).await.unwrap();
    let two = ProjectService::get_by_id(&db.pool, 2).await.unwrap();
    assert_eq!(two.slug, "my-app");
    assert_eq!(one.slug, "my-app-1");
}
