//! Plugins: WebAssembly components installed at runtime, each with its own
//! SQLite file, HTTP routes and permissions. The contract is `wit/plugin.wit`;
//! the design is in the plugin spec (`docs/plugins/spec-v0.1.md`).

pub mod host;
pub mod inspect;
pub mod manifest;
pub mod package;
pub mod registry;
pub mod runtime;
pub mod settings;
pub mod store;

/// The versions of the `rustrak:plugin` world this host implements.
pub const WORLDS: &[&str] = &["0.1.0"];

/// This Rustrak's version: what a plugin's `rustrak` range is checked against.
pub fn host_version() -> semver::Version {
    env!("CARGO_PKG_VERSION")
        .parse()
        .expect("the crate version is semver")
}
