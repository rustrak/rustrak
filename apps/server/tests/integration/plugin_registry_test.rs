//! The plugin registry: a signed index of published plugins. What it lists
//! installs as verified; a package uploaded by hand (someone's own plugin)
//! installs as not verified.

use actix_web::http::StatusCode;
use actix_web::test;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::common::{self, plugins::hello_rtp, TestDb};

/// A registry in a folder: hello's package, an index naming it, the index's
/// minisign signature with a key made for this test, and that key.
struct Registry {
    dir: tempfile::TempDir,
    key: String,
}

impl Registry {
    fn new(sha256: Option<String>, sign: bool) -> Self {
        Self::listing("hello", sha256, sign)
    }

    /// The same, hello's package listed under `id`.
    fn listing(id: &str, sha256: Option<String>, sign: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let rtp = hello_rtp();
        std::fs::write(dir.path().join("hello-0.1.0.rtp"), &rtp).unwrap();
        let logo = b"<svg xmlns='http://www.w3.org/2000/svg'/>";
        std::fs::write(dir.path().join("hello.svg"), logo).unwrap();
        let index = json!({
            "version": 1,
            "plugins": [{
                "id": id,
                "name": "Hello",
                "description": "The smallest Rustrak plugin.",
                "icon": "star",
                "author": "Rustrak",
                "logo": { "url": "hello.svg", "sha256": hex::encode(Sha256::digest(logo)) },
                "versions": [
                    { "version": "0.1.0", "rustrak": ">=0.33.0", "url": "hello-0.1.0.rtp",
                      "sha256": sha256.unwrap_or_else(|| hex::encode(Sha256::digest(&rtp))) },
                    { "version": "9.0.0", "rustrak": ">=99.0.0", "url": "hello-9.0.0.rtp",
                      "sha256": "00" },
                ],
            }],
        })
        .to_string();
        std::fs::write(dir.path().join("index.json"), &index).unwrap();
        let keys = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let signed = if sign {
            index.clone()
        } else {
            format!("{index} ")
        };
        let signature = minisign::sign(
            Some(&keys.pk),
            &keys.sk,
            std::io::Cursor::new(signed.into_bytes()),
            Some("rustrak plugin index"),
            None,
        )
        .unwrap();
        std::fs::write(dir.path().join("index.json.minisig"), signature.to_string()).unwrap();
        Registry {
            key: keys.pk.to_base64(),
            dir,
        }
    }

    fn config(&self) -> rustrak::config::Config {
        let mut config = common::config();
        config.plugin_registry = Some(format!("file://{}/index.json", self.dir.path().display()));
        config.plugin_registry_keys = vec![self.key.clone()];
        config
    }
}

#[actix_web::test]
async fn a_published_plugin_installs_from_the_registry_as_verified() {
    let registry = Registry::new(None, true);
    let db = TestDb::new().await;
    let state = common::state(&db.pool, registry.config()).await;
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;

    let req = test::TestRequest::get()
        .uri("/api/settings/plugins/registry")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    let listing: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(listing["available"], true, "{listing}");
    let hello = &listing["plugins"][0];
    assert_eq!(hello["id"], "hello");
    // The newest version this Rustrak can run, not the newest published.
    assert_eq!(hello["version"], "0.1.0");
    assert_eq!(hello["installed_version"], Value::Null);

    // Its logo, before it is installed: through Rustrak, checked against
    // the signed index, inert like an installed one's.
    let logo = hello["logo"].as_str().expect("a logo").to_string();
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&logo)
            .insert_header(crate::common::bearer(&owner))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/svg+xml");
    assert!(resp
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("sandbox"));

    let req = test::TestRequest::post()
        .uri("/api/settings/plugins/registry/hello")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let installed: Value = test::read_body_json(resp).await;
    assert_eq!(installed["verified"], true);

    let req = test::TestRequest::get()
        .uri("/api/settings/plugins/registry")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    let listing: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(listing["plugins"][0]["installed_version"], "0.1.0");

    // Still verified after a restart.
    let restarted =
        rustrak::plugins::host::PluginHost::new(state.config.plugins_dir.clone()).unwrap();
    restarted.boot(&db.pool).await.unwrap();
    assert!(restarted.list(&db.pool).await.unwrap()[0].verified);
}

#[actix_web::test]
async fn an_uploaded_package_installs_as_not_verified() {
    let db = TestDb::new().await;
    let app = common::app(&db.pool).await;
    let owner = common::create_test_token(&db.pool).await;
    let req = test::TestRequest::post()
        .uri("/api/settings/plugins")
        .insert_header(crate::common::bearer(&owner))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(hello_rtp())
        .to_request();
    let installed: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(installed["verified"], false);
}

#[actix_web::test]
async fn an_index_whose_signature_does_not_match_is_not_trusted() {
    let registry = Registry::new(None, false);
    let db = TestDb::new().await;
    let state = common::state(&db.pool, registry.config()).await;
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;

    let req = test::TestRequest::get()
        .uri("/api/settings/plugins/registry")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    let listing: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(listing["available"], false);
    assert!(
        listing["error"].as_str().unwrap().contains("signature"),
        "{listing}"
    );
    assert_eq!(listing["plugins"], json!([]));

    let req = test::TestRequest::post()
        .uri("/api/settings/plugins/registry/hello")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_GATEWAY
    );
    assert!(state.plugins.get("hello").is_none());
}

#[actix_web::test]
async fn a_package_that_is_not_the_one_the_index_signed_is_refused() {
    let registry = Registry::new(Some("ab".repeat(32)), true);
    let db = TestDb::new().await;
    let state = common::state(&db.pool, registry.config()).await;
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;

    let req = test::TestRequest::post()
        .uri("/api/settings/plugins/registry/hello")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_GATEWAY
    );
    assert!(state.plugins.get("hello").is_none());
}

#[actix_web::test]
async fn a_package_of_another_plugin_is_refused_before_it_touches_that_plugin() {
    let registry = Registry::listing("other", None, true);
    let db = TestDb::new().await;
    let state = common::state(&db.pool, registry.config()).await;
    state
        .plugins
        .install(
            &db.pool,
            hello_rtp(),
            rustrak::plugins::host::Source::Upload,
        )
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;

    let req = test::TestRequest::post()
        .uri("/api/settings/plugins/registry/other")
        .insert_header(crate::common::bearer(&owner))
        .to_request();
    assert_eq!(
        test::call_service(&app, req).await.status(),
        StatusCode::BAD_GATEWAY
    );
    // Hello, which the package really is, stays as it was installed.
    let listed = state.plugins.list(&db.pool).await.unwrap();
    assert_eq!(listed.len(), 1, "hello is still installed, alone");
    assert!(!listed[0].verified);
}
