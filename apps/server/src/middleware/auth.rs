use actix_session::Session;
use actix_web::{
    body::{EitherBody, MessageBody},
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, FromRequest, HttpResponse,
};
use std::future::{ready, Ready};
use std::pin::Pin;
use std::rc::Rc;

use crate::auth;
use crate::routes::dashboard::API_PREFIXES;

/// Middleware to require authentication for routes.
///
/// It guards what routing does not: a request that matches no handler. The API
/// prefixes are exempt because every route under them authenticates for itself
/// through an extractor -- `SentryAuth` on ingest, `AuthenticatedUser` on the
/// management API -- and doing it twice would mean two places to keep in step.
#[derive(Debug, Clone, Copy)]
pub struct RequireAuth {
    /// Whether the compiled dashboard is mounted at the root.
    ///
    /// When it is, the paths outside the API prefixes stop being unmatched:
    /// they are the single-page application's own routes, and they have to be
    /// public. That is not a relaxation -- the bundle is the code that draws
    /// the login form, it holds nothing a stranger could not download from the
    /// release, and every API call it goes on to make is authenticated as
    /// before. Off, the middleware behaves exactly as it did before the
    /// dashboard existed.
    serve_dashboard: bool,
}

impl RequireAuth {
    /// `serve_dashboard` says whether [`crate::routes::dashboard`] is mounted.
    pub fn new(serve_dashboard: bool) -> Self {
        Self { serve_dashboard }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RequireAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = RequireAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequireAuthMiddleware {
            service: Rc::new(service),
            serve_dashboard: self.serve_dashboard,
        }))
    }
}

pub struct RequireAuthMiddleware<S> {
    service: Rc<S>,
    serve_dashboard: bool,
}

impl<S, B> Service<ServiceRequest> for RequireAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let path = req.path().to_string();

        // Exempt routes from authentication middleware
        // - /auth/* - authentication routes (login, register, etc.)
        // - /api/* - API routes (authentication handled by extractors:
        //            SentryAuth for ingest, AuthenticatedUser for management)
        // - /health - health check routes
        // - /metrics - opt-in Prometheus scrape, restricted at the network edge
        let is_api = path.starts_with("/auth")
            || path.starts_with("/api/")
            || path.starts_with("/health")
            || path == "/metrics"
            || {
                #[cfg(feature = "openapi")]
                {
                    path.starts_with("/docs") || path.starts_with("/api-docs/")
                }
                #[cfg(not(feature = "openapi"))]
                {
                    false
                }
            };

        // With the dashboard mounted, everything the API does not own is one
        // of its client routes and is served as static files. `API_PREFIXES`
        // is the same list `routes::dashboard` refuses to answer with the
        // shell, so a path is either an API answer or a public one and never
        // both.
        let is_dashboard = self.serve_dashboard
            && !API_PREFIXES
                .iter()
                .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")));

        let is_exempt = is_api || is_dashboard;

        if is_exempt {
            let service = Rc::clone(&self.service);
            return Box::pin(
                async move { service.call(req).await.map(|res| res.map_into_left_body()) },
            );
        }

        // Check session for authenticated user
        let http_req = req.request();
        let session = Session::extract(http_req).into_inner();

        let service = Rc::clone(&self.service);

        Box::pin(async move {
            match session {
                Ok(session) => {
                    if auth::get_user_id_from_session(&session).is_some() {
                        // User is authenticated
                        service.call(req).await.map(|res| res.map_into_left_body())
                    } else {
                        // Not authenticated
                        let (http_req, _) = req.into_parts();
                        let response = HttpResponse::Unauthorized()
                            .json(serde_json::json!({
                                "error": "Not authenticated"
                            }))
                            .map_into_boxed_body();
                        Ok(ServiceResponse::new(http_req, response).map_into_right_body())
                    }
                }
                Err(_) => {
                    // Session error
                    let (http_req, _) = req.into_parts();
                    let response = HttpResponse::Unauthorized()
                        .json(serde_json::json!({
                            "error": "Session error"
                        }))
                        .map_into_boxed_body();
                    Ok(ServiceResponse::new(http_req, response).map_into_right_body())
                }
            }
        })
    }
}
