use actix_cors::Cors;
use actix_web::http::header::{self, HeaderName};

/// The server's CORS policy.
///
/// Permissive on purpose: Sentry SDKs send from any origin, and in error
/// tracking the app means to send its data here, so there is nothing for
/// CORS to protect.
pub fn cors() -> Cors {
    Cors::default()
        .allow_any_origin()
        .allowed_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"])
        .allowed_headers(vec![
            header::AUTHORIZATION,
            header::ACCEPT,
            header::CONTENT_TYPE,
            header::CONTENT_ENCODING,
            // Headers used by Sentry SDKs
            HeaderName::from_static("x-sentry-auth"),
            HeaderName::from_static("sentry-trace"),
            HeaderName::from_static("baggage"),
        ])
        // A browser SDK can only read what is exposed, and these are what it
        // backs off on: the same three Relay exposes.
        .expose_headers(vec![
            HeaderName::from_static("x-sentry-error"),
            HeaderName::from_static("x-sentry-rate-limits"),
            header::RETRY_AFTER,
        ])
        .max_age(3600)
}
