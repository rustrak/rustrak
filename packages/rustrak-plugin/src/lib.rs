//! Write Rustrak plugins in plain Rust.
//!
//! ```ignore
//! use rustrak_plugin::prelude::*;
//! use crate::Perm; // from rustrak.config.ts, by `rustrak_plugin::config!()`
//!
//! #[post("/hit", public)]
//! fn hit(ctx: &Ctx, body: Body) -> Result<NoContent> {
//!     ctx.db().execute("INSERT INTO hits (path) VALUES (?1)", params![body.text()])?;
//!     Ok(NoContent)
//! }
//!
//! #[get("/count", requires = Perm::Stats.read())]
//! fn count(ctx: &Ctx) -> Result<String> {
//!     Ok(ctx.db().scalar::<i64>("SELECT count(*) FROM hits", params![])?.to_string())
//! }
//! ```
//!
//! Routes live in `src/server/routes/` and register themselves. The plugin's name,
//! version, settings and permissions live in its `package.json` and
//! `rustrak.config.ts`; `@rustrak/plugin` builds and
//! packs it, deriving the rest of the manifest from this code.

mod ctx;
mod db;
mod http;

pub use ctx::Ctx;
pub use db::{Db, DbError, ToValue, Value};
pub use http::{Body, Created, Error, IntoResponse, Json, NoContent, Query, Result};
pub use rustrak_plugin_macros::{delete, get, patch, post, put};

/// The plugin's `Settings` and `Perm`, generated from `rustrak.config.ts` by
/// `rustrak-plugin build` into `.rustrak/plugin.rs`. One line in `lib.rs`.
#[macro_export]
macro_rules! config {
    () => {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/.rustrak/plugin.rs"));
    };
}

#[doc(hidden)]
pub use serde;

/// Builds the parameter list of a query: `params![a, b]`.
#[macro_export]
macro_rules! params {
    ($($v:expr),* $(,)?) => {
        &[$(&$v as &dyn $crate::ToValue),*]
    };
}

pub mod prelude {
    pub use crate::{
        Body, Created, Ctx, Error, IntoResponse, Json, NoContent, Query, Result, delete, get,
        params, patch, post, put,
    };
}

/// What the macros expand to. Not part of the API.
#[doc(hidden)]
pub mod __private {
    pub use inventory;
    pub use serde_json;

    // Generated from plugin.wit: its shapes are the WIT's, not ours to lint.
    #[allow(clippy::too_many_arguments)]
    pub mod bindings {
        wit_bindgen::generate!({
            path: "../../apps/server/wit",
            world: "plugin",
            pub_export_macro: true,
            export_macro_name: "export_plugin",
            default_bindings_module: "rustrak_plugin::__private::bindings",
        });
    }

    pub use bindings::export_plugin;

    // The export lives here, in the SDK, so an author's crate has no
    // boilerplate: depending on rustrak-plugin is enough.
    export_plugin!(Entry with_types_in bindings);

    pub use crate::http::{FromCtx, Response};

    /// A route, as `dispatch` finds it. What Rustrak needs to know about it
    /// (auth, scope, `requires`) is in the `rustrak:routes` custom section
    /// the macro writes beside it, read without running the code.
    pub struct RouteDef {
        pub method: &'static str,
        pub path: &'static str,
        pub handler: fn(&crate::Ctx) -> Response,
    }

    /// The length of `parts` joined.
    pub const fn joined_len(parts: &[&str]) -> usize {
        let (mut n, mut i) = (0, 0);
        while i < parts.len() {
            n += parts[i].len();
            i += 1;
        }
        n
    }

    /// `parts` joined, at compile time: one line of `rustrak:routes`.
    pub const fn joined<const N: usize>(parts: &[&str]) -> [u8; N] {
        let mut out = [0u8; N];
        let (mut k, mut i) = (0, 0);
        while i < parts.len() {
            let bytes = parts[i].as_bytes();
            let mut j = 0;
            while j < bytes.len() {
                out[k] = bytes[j];
                k += 1;
                j += 1;
            }
            i += 1;
        }
        out
    }

    inventory::collect!(RouteDef);

    /// The guest side of the world, exported below.
    pub struct Entry;

    impl bindings::Guest for Entry {
        fn handle_http(req: bindings::Request) -> bindings::Response {
            crate::http::dispatch(req)
        }
    }
}
