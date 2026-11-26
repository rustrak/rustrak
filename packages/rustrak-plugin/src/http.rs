use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::__private::RouteDef;
use crate::__private::bindings::{self, Request};
use crate::ctx::Ctx;
use crate::db::DbError;

/// What a handler can fail with; each maps to an HTTP status.
#[derive(Debug)]
pub enum Error {
    /// 400, with a message for the caller.
    BadRequest(String),
    /// 403
    Forbidden,
    /// 404
    NotFound,
    /// 500. The message is logged by the host, not shown to the caller.
    Internal(String),
    /// 503: the plugin cannot answer yet, a required setting is not set.
    Unavailable(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<DbError> for Error {
    fn from(e: DbError) -> Self {
        Error::Internal(e.0)
    }
}

#[doc(hidden)]
pub struct Response {
    status: u16,
    content_type: Option<&'static str>,
    body: Vec<u8>,
}

/// A handler's return value.
pub trait IntoResponse {
    #[doc(hidden)]
    fn into_response(self) -> Response;
}

/// 204, no body.
pub struct NoContent;

impl IntoResponse for NoContent {
    fn into_response(self) -> Response {
        Response {
            status: 204,
            content_type: None,
            body: vec![],
        }
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        Response {
            status: 200,
            content_type: Some("text/plain; charset=utf-8"),
            body: self.into_bytes(),
        }
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        self.to_string().into_response()
    }
}

/// A JSON body: what a handler takes as `Json<T>` and returns as `Json(value)`.
pub struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        match serde_json::to_vec(&self.0) {
            Ok(body) => Response {
                status: 200,
                content_type: Some("application/json"),
                body,
            },
            Err(e) => Error::Internal(e.to_string()).into_response(),
        }
    }
}

/// 201 with a JSON body: something was created.
pub struct Created<T>(pub T);

impl<T: Serialize> IntoResponse for Created<T> {
    fn into_response(self) -> Response {
        // A body that failed to serialize stays the 500 it is.
        let response = Json(self.0).into_response();
        if response.status == 200 {
            response.with_status(201)
        } else {
            response
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, detail) = match self {
            Error::BadRequest(m) => (400, m),
            Error::Forbidden => (403, "Forbidden".into()),
            Error::NotFound => (404, "Not found".into()),
            Error::Unavailable(m) => (503, m),
            Error::Internal(m) => {
                crate::__private::bindings::rustrak::plugin::log::write(
                    crate::__private::bindings::rustrak::plugin::log::Level::Error,
                    &m,
                );
                (500, "Internal error".into())
            }
        };
        Json(serde_json::json!({ "detail": detail }))
            .into_response()
            .with_status(status)
    }
}

impl<T: IntoResponse> IntoResponse for Result<T> {
    fn into_response(self) -> Response {
        self.map_or_else(IntoResponse::into_response, IntoResponse::into_response)
    }
}

impl Response {
    fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
}

/// Something a handler takes as an argument, read from the request.
pub trait FromCtx: Sized {
    #[doc(hidden)]
    fn from_ctx(ctx: &Ctx) -> Result<Self>;
}

/// The raw request body.
pub struct Body(pub Vec<u8>);

impl Body {
    /// The body as text, invalid UTF-8 replaced.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }
}

impl FromCtx for Body {
    fn from_ctx(ctx: &Ctx) -> Result<Self> {
        Ok(Body(ctx.request.body.clone()))
    }
}

impl<T: DeserializeOwned> FromCtx for Json<T> {
    fn from_ctx(ctx: &Ctx) -> Result<Self> {
        let body = ctx.request.body.as_slice();
        serde_json::from_slice(body)
            .map(Json)
            .map_err(|e| Error::BadRequest(format!("invalid JSON body: {e}")))
    }
}

/// The query string, deserialized into `T`.
pub struct Query<T>(pub T);

impl<T: DeserializeOwned> FromCtx for Query<T> {
    fn from_ctx(ctx: &Ctx) -> Result<Self> {
        let query = ctx.request.query.as_str();
        serde_urlencoded::from_str(query)
            .map(Query)
            .map_err(|e| Error::BadRequest(format!("invalid query: {e}")))
    }
}

pub(crate) fn dispatch(req: Request) -> bindings::Response {
    let route = inventory::iter::<RouteDef>
        .into_iter()
        .find(|r| r.method == req.method && r.path == req.path);
    let response = match route {
        Some(route) => (route.handler)(&Ctx { request: req }),
        None => Error::NotFound.into_response(),
    };
    bindings::Response {
        status: response.status,
        headers: response
            .content_type
            .map(|ct| vec![("content-type".to_string(), ct.to_string())])
            .unwrap_or_default(),
        body: response.body,
    }
}
