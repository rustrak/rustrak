//! Serving the compiled dashboard.
//!
//! The dashboard is a single-page application: `apps/dashboard` compiles to a
//! directory of static files and this module hands them out. There is no Node
//! process in production and no second container -- the same Actix instance
//! that answers `/api/projects` answers `/`.
//!
//! That is not only a packaging convenience. It puts the browser and the API
//! on one origin, which is what keeps the session cookie first-party and
//! removes CORS from the dashboard's path entirely.
//!
//! # It is optional, and stays optional
//!
//! Rustrak's premise is that the server alone is a complete product: point any
//! Sentry SDK at it and never deploy a UI. So nothing here is required. If the
//! configured directory has no `index.html`, [`Dashboard::detect`] returns
//! `None`, nothing is mounted, and the server behaves exactly as it did before
//! this module existed.
//!
//! # The two mounts
//!
//! `/assets` is served on its own because everything Vite puts there carries a
//! content hash in its file name. A URL under it can never point at different
//! bytes, so it is the one directory that can be marked `immutable` -- and the
//! one where a miss must stay a miss rather than fall through to the shell.
//!
//! Everything else falls back to `index.html`, because in a client-routed
//! application `/projects/42` is a real page the server has never heard of.
//!
//! # The one rule the fallback must not break
//!
//! A catch-all that answers *everything* with HTML is worse than no fallback.
//! An API path that reaches this module is a path no handler claimed, and it
//! has to stay an API answer: a JSON 404, in the same shape as every other
//! error the server produces. Otherwise a mistyped endpoint answers `200
//! text/html` and the client reports a validation failure against itself.

use actix_files::Files;
use actix_web::body::BoxBody;
use actix_web::dev::{fn_service, ServiceRequest, ServiceResponse};
use actix_web::http::header::{
    CacheControl, CacheDirective, ETag, EntityTag, IfNoneMatch, CONTENT_TYPE,
};
use actix_web::middleware::DefaultHeaders;
use actix_web::{web, Error, HttpMessage, HttpRequest, HttpResponse, ResponseError};
use bytes::Bytes;
use std::path::{Path, PathBuf};

use crate::config::DashboardConfig;
use crate::error::AppError;

/// The path prefixes the API owns.
///
/// A request under one of these never becomes the dashboard shell, whether or
/// not a handler claimed it. The dashboard's Vite proxy keeps the same list in
/// `apps/dashboard/vite.config.ts`, so that development routes requests the
/// same way production does; the two must not drift.
pub const API_PREFIXES: [&str; 6] = ["/api", "/auth", "/health", "/docs", "/api-docs", "/metrics"];

/// How long a hashed asset may be cached: one year, the maximum HTTP defines
/// as meaningful.
const ONE_YEAR: u32 = 31_536_000;

/// A compiled dashboard on disk.
///
/// Constructed only through [`Dashboard::detect`], so holding one is proof
/// that a build was found rather than a promise that one might be.
#[derive(Debug, Clone)]
pub struct Dashboard {
    root: PathBuf,
    /// `index.html`, read once at startup.
    ///
    /// Every client route answers with these exact bytes, so reading them per
    /// request would be a blocking filesystem call on the hot path for a file
    /// that cannot change while the process lives. `Bytes` clones by
    /// refcount, so handing it out costs nothing.
    shell: Bytes,
    /// The shell's identity, for revalidation. See [`Dashboard::shell_response`].
    etag: EntityTag,
}

impl Dashboard {
    /// Look for a build in `root`.
    ///
    /// The test is `index.html`, not the directory: an empty `static/` is what
    /// a half-finished `COPY` in a Dockerfile leaves behind, and mounting it
    /// would answer every route with a 404 that looks like a routing bug
    /// rather than a missing build.
    pub fn detect(root: impl AsRef<Path>) -> Option<Self> {
        let root = root.as_ref();

        let index = root.join("index.html");

        if !index.is_file() {
            return None;
        }

        // Resolved once, at startup: leaving it relative would make the mount
        // depend on the process's working directory at the time of the
        // request rather than at the time it started.
        let root = root.canonicalize().ok()?;
        let shell = Bytes::from(std::fs::read(&index).ok()?);
        let etag = EntityTag::new_weak(hex::encode(
            &<sha2::Sha256 as sha2::Digest>::digest(&shell)[..8],
        ));

        Some(Self { root, shell, etag })
    }

    /// The dashboard the configuration describes, if it is to be served.
    ///
    /// `None` for two different reasons, and `main` tells them apart for the
    /// log: the switch is off, or there is no build where the directory says.
    pub fn from_config(config: &DashboardConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        Self::detect(&config.dir)
    }

    /// Where the build was found. Logged at startup so an operator can see
    /// which directory the server actually picked up.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The application shell.
    ///
    /// `no-cache` is not `no-store`: the browser may keep it, it just may not
    /// use it without asking. It has to ask, because the shell names the
    /// hashed bundles and a cached copy keeps pointing at chunks a deploy has
    /// already removed. The ETag is what makes that cheap -- a navigation
    /// after the first costs a 304 and no body.
    fn shell_response(&self, req: &HttpRequest) -> HttpResponse {
        let unchanged = req
            .get_header::<IfNoneMatch>()
            .is_some_and(|if_none_match| match if_none_match {
                IfNoneMatch::Any => true,
                IfNoneMatch::Items(tags) => tags.iter().any(|tag| tag.weak_eq(&self.etag)),
            });

        let mut builder = if unchanged {
            HttpResponse::NotModified()
        } else {
            HttpResponse::Ok()
        };

        builder
            .insert_header((CONTENT_TYPE, "text/html; charset=utf-8"))
            .insert_header(ETag(self.etag.clone()))
            .insert_header(CacheControl(vec![CacheDirective::NoCache]));

        if unchanged {
            builder.finish()
        } else {
            builder.body(self.shell.clone())
        }
    }

    /// The service configuration. Register it **last**: it ends in a catch-all,
    /// and Actix resolves services in registration order, so anything mounted
    /// after it would be unreachable.
    pub fn configure(&self) -> impl FnOnce(&mut web::ServiceConfig) {
        let root = self.root.clone();
        let assets = self.root.join("assets");
        let shell = self.clone();

        move |cfg: &mut web::ServiceConfig| {
            // Hashed bundles. `DefaultHeaders` rather than a header on `Files`
            // because `Files` has no way to set one, and a scope is the only
            // place a middleware can wrap it.
            cfg.service(
                web::scope("/assets")
                    .wrap(DefaultHeaders::new().add(CacheControl(vec![
                        CacheDirective::Public,
                        CacheDirective::MaxAge(ONE_YEAR),
                        CacheDirective::Extension("immutable".to_owned(), None),
                    ])))
                    .service(Files::new("", &assets).prefer_utf8(true)),
            );

            // The root, claimed before the catch-all.
            //
            // `Files` answers a request for a directory that exists out of its
            // own listing renderer, which never reaches `default_handler`, so
            // `/` would 404 while `/projects` rendered. Naming it explicitly
            // keeps every shell response coming from one place, headers
            // included; `index_file` would have served it from a second.
            let root_shell = shell.clone();
            let serve_shell = move |req: HttpRequest| {
                let dashboard = root_shell.clone();
                async move { dashboard.shell_response(&req) }
            };
            cfg.service(
                web::resource("/")
                    .route(web::get().to(serve_shell.clone()))
                    .route(web::head().to(serve_shell)),
            );

            // Everything else.
            cfg.service(
                Files::new("/", &root)
                    .prefer_utf8(true)
                    .default_handler(fn_service(move |req: ServiceRequest| {
                        let dashboard = shell.clone();
                        async move { shell_or_not_found(req, dashboard) }
                    })),
            );
        }
    }
}

/// The fallback: the shell for a client route, a JSON 404 for anything the API
/// owns.
fn shell_or_not_found(
    req: ServiceRequest,
    dashboard: Dashboard,
) -> Result<ServiceResponse<BoxBody>, Error> {
    let (http_req, _payload) = req.into_parts();
    let path = http_req.path();

    let api_path = API_PREFIXES.iter().any(|prefix| {
        // Prefix *segments*, not string prefixes: `/api` and `/api/x` are the
        // API's, `/apidocs-for-humans` is a client route the dashboard may
        // legitimately own one day.
        path == *prefix || path.starts_with(&format!("{prefix}/"))
    });

    // A miss under `/assets` is a missing build artifact, never a client
    // route. Answering it with the shell hands the browser HTML under a `.js`
    // URL, and the syntax error that follows names the bundle rather than the
    // deploy that dropped it.
    let missing_asset = path == "/assets" || path.starts_with("/assets/");

    if api_path || missing_asset {
        let response = AppError::NotFound(path.to_string()).error_response();
        return Ok(ServiceResponse::new(http_req, response));
    }

    let response = dashboard.shell_response(&http_req);

    Ok(ServiceResponse::new(http_req, response))
}
