use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::Serialize;

/// The `message` every 5xx body carries, in place of the error's `Display`.
///
/// Fixed on purpose. `AppError::Database` renders `sqlx::Error`, whose
/// Postgres arm names the constraint, table and column of the failed query,
/// and `AppError::Internal` interpolates whatever string its call site had to
/// hand. Neither is safe on a wire that a caller holding nothing but a public
/// DSN can read.
pub const INTERNAL_ERROR_MESSAGE: &str = "Internal server error";

/// Response header echoing [`ErrorDetail::incident_id`].
///
/// Duplicates the body on purpose. `middleware::Logger` is the only place that
/// knows the method and path of the request that failed, and a response header
/// is the only part of the response it can read, so this is what lets one
/// `grep <id>` return both the access-log line naming the route and the
/// `log::error!` line carrying the detail.
pub const INCIDENT_ID_HEADER: &str = "X-Rustrak-Incident";

/// The `Retry-After` (and `X-Sentry-Rate-Limits`) of every 503: long enough
/// for a busy database or a draining spool to recover, short enough that
/// little is lost meanwhile. A Sentry SDK never resends an envelope that got
/// an HTTP response: the event of a 503 is dropped (sentry-javascript
/// `transports/base.ts`, sentry-python `transport.py`), and the headers
/// only make it stop sending, and dropping the next ones, for this long.
pub const RETRY_AFTER_SECS: u64 = 10;

/// The `log::error!` line that carries what [`INTERNAL_ERROR_MESSAGE`] took
/// out of the response body.
fn incident_log_line(incident_id: &str, error: &AppError) -> String {
    format!("incident {incident_id}: {error}")
}

/// JSON error response structure
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
    /// Machine-readable annotations naming the inputs that were rejected.
    ///
    /// Omitted from the JSON entirely when empty, so every consumer written
    /// before this existed keeps seeing exactly the body it saw before.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldError>,
    /// Correlates this response with the `log::error!` line that carries the
    /// detail [`INTERNAL_ERROR_MESSAGE`] replaced.
    ///
    /// Present on every 5xx and on nothing else: a 4xx says what went wrong in
    /// `message`, logs nothing, and so has no line for an id to point at.
    ///
    /// `nullable = false`: `skip_serializing_if` means the key is *absent* on
    /// a 4xx, never `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub incident_id: Option<String>,
}

/// One rejected input, named as data rather than buried in the prose of
/// [`ErrorDetail::message`].
///
/// `field` is a **dot path into the request body** (`slug`,
/// `credentials.webhook_url`), not a JSON Pointer and not a UI label: a
/// consumer feeds it straight to a form library's `setError`.
///
/// The rejected *value* is deliberately never echoed back. One careless
/// application to a password input would put a credential in the log store.
///
/// The three fields are private on purpose. [`Self::new`] and [`Self::custom`]
/// are the only ways to build one, which is what makes "`message` is populated
/// only alongside [`FieldErrorCode::Custom`]" an invariant the compiler holds
/// rather than a convention a struct literal could quietly break.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FieldError {
    field: String,
    code: FieldErrorCode,
    /// Only for what [`FieldErrorCode`] cannot express, and then only
    /// alongside [`FieldErrorCode::Custom`]. Consumers pick their copy from
    /// `(field, code)` so it can be translated, and never parse this.
    ///
    /// `nullable = false`: `skip_serializing_if` means the key is *absent*,
    /// never `null`, so a generated client must not model a null it can never
    /// receive.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    message: Option<String>,
}

impl FieldError {
    /// A field annotation carrying no bespoke message, which is the shape
    /// every site should be using.
    pub fn new(field: impl Into<String>, code: FieldErrorCode) -> Self {
        Self {
            field: field.into(),
            code,
            message: None,
        }
    }

    /// A [`FieldErrorCode::Custom`] annotation with prose the code set cannot
    /// express. Reach for it only after failing to find an honest code.
    pub fn custom(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            code: FieldErrorCode::Custom,
            message: Some(message.into()),
        }
    }

    /// The dot path into the request body this annotation blames.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// The closed-vocabulary reason.
    pub fn code(&self) -> FieldErrorCode {
        self.code
    }

    /// The bespoke prose, present only for [`FieldErrorCode::Custom`].
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

/// The closed vocabulary a [`FieldError`] may use.
///
/// Deliberately small and resource-agnostic: a large or per-endpoint
/// vocabulary becomes a public API that cannot be versioned. Adding a variant
/// is a wire change and requires the matching addition to the TypeScript union
/// in `packages/client/src/errors.ts`, which
/// `packages/client/tests/unit/app-error-contract.test.ts` enforces by parsing
/// this enum out of this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum FieldErrorCode {
    /// Absent or empty, and mandatory.
    Required,
    /// Malformed, or not acceptable for this particular request.
    ///
    /// The two readings are deliberately one code, and the **status** is what
    /// tells them apart. `(role, invalid)` on a **400** means the value is not
    /// a member of the enum at all ("`superuser` is not a role"); the same
    /// `(role, invalid)` on a **409** means the value is well-formed but not
    /// acceptable *right now* ("this is the last admin"). A consumer that
    /// wants different copy for the two must branch on `kind`/status as well
    /// as on `(field, code)`.
    Invalid,
    /// Taken. The thing only the server can know, and always a 409.
    AlreadyExists,
    /// Below the minimum length.
    TooShort,
    /// Above the maximum length.
    TooLong,
    /// Well-formed but wrong, such as a current password that does not match.
    Incorrect,
    /// Read [`FieldError::message`] instead.
    Custom,
}

/// An [`AppError`] together with the fields it blames.
///
/// Holds the error it decorates rather than restating its status and wire
/// `type`, so an annotated `Conflict` is a 409 named `Conflict` by
/// construction and cannot drift from a plain one.
///
/// Both fields are private and there is no public constructor:
/// [`AppError::with_fields`] is the only way to build one, which is what turns
/// "`inner` is never itself an [`AppError::WithFields`]" from a comment into
/// an invariant. It has to be enforced rather than documented, because
/// [`AppError::kind`], [`AppError::error_type`] and `status_code` all recurse
/// through `inner` while [`AppError::field_errors`] reads one level: a nested
/// value would emit the right status and message and silently drop the inner
/// annotations.
#[derive(Debug, thiserror::Error)]
#[error("{inner}")]
pub struct FieldedError {
    inner: Box<AppError>,
    fields: Vec<FieldError>,
}

impl FieldedError {
    /// The error being decorated. Never an [`AppError::WithFields`].
    pub fn inner(&self) -> &AppError {
        &self.inner
    }

    /// The inputs this error blames.
    pub fn fields(&self) -> &[FieldError] {
        &self.fields
    }
}

/// Application errors
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Payload too large: {0}")]
    PayloadTooLarge(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal server error: {0}")]
    Internal(String),

    /// A condition that clears up by itself (a full disk, a spool backlog):
    /// 503 with `Retry-After`, as Relay answers when its buffer is full, so
    /// an SDK pauses for a while. The event itself is dropped by the SDK.
    /// The message is ours and safe to show.
    #[error("Service unavailable: {0}")]
    Unavailable(String),

    /// Any of the nine above, annotated with the fields it blames.
    ///
    /// A decorator, not a tenth kind of error: it has no status, no wire
    /// `type` and no message of its own, and is only ever built through
    /// [`AppError::with_field`] / [`AppError::with_fields`]. That is what
    /// keeps the nine the whole vocabulary, and what let the ~105 call sites
    /// that have no field to name stay untouched.
    #[error(transparent)]
    WithFields(FieldedError),
}

/// An extractor's rejection, so a handler can take `AppResult<web::Json<T>>`
/// and read it after the credentials: actix polls a handler's extractors
/// together, and a bad body must not answer an anonymous caller before 401.
/// Keeps the extractor's 404 (a path that does not parse) and 413.
impl From<actix_web::Error> for AppError {
    fn from(error: actix_web::Error) -> Self {
        let message = error.to_string();
        match error.as_response_error().status_code() {
            StatusCode::NOT_FOUND => AppError::NotFound(message),
            StatusCode::PAYLOAD_TOO_LARGE => AppError::PayloadTooLarge(message),
            _ => AppError::Validation(message),
        }
    }
}

impl AppError {
    /// Annotates this error with one rejected field, keeping its status, its
    /// wire `type` and its message exactly as they were.
    ///
    /// ```ignore
    /// AppError::Conflict(format!("Project with slug '{slug}' already exists"))
    ///     .with_field("slug", FieldErrorCode::AlreadyExists)
    /// ```
    #[must_use]
    pub fn with_field(self, field: impl Into<String>, code: FieldErrorCode) -> Self {
        self.with_fields(vec![FieldError::new(field, code)])
    }

    /// Annotates this error with several rejected fields.
    ///
    /// Appends when the error is already annotated, which is what keeps
    /// [`FieldedError::inner`] from ever being a [`AppError::WithFields`]
    /// however many times this is called.
    #[must_use]
    pub fn with_fields(self, fields: Vec<FieldError>) -> Self {
        match self {
            AppError::WithFields(mut fielded) => {
                fielded.fields.extend(fields);
                AppError::WithFields(fielded)
            }
            inner => AppError::WithFields(FieldedError {
                inner: Box::new(inner),
                fields,
            }),
        }
    }

    /// This error with any field annotation stripped.
    ///
    /// What a `match` on the variant wants: `err.kind()` is
    /// `AppError::Conflict(_)` whether or not the error names a field, while
    /// matching `err` itself would silently miss every annotated one.
    pub fn kind(&self) -> &AppError {
        match self {
            AppError::WithFields(fielded) => fielded.inner.kind(),
            other => other,
        }
    }

    /// The database's own code for this error: a SQLSTATE on Postgres, an
    /// extended result code on SQLite. `None` for anything else.
    pub fn db_code(&self) -> Option<String> {
        match self.kind() {
            AppError::Database(sqlx::Error::Database(db)) => db.code().map(|c| c.into_owned()),
            _ => None,
        }
    }

    /// A filesystem failure: a full disk is [`AppError::Unavailable`], anything
    /// else [`AppError::Internal`]. `what` names the step that failed.
    pub fn from_io(what: &str, error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::StorageFull {
            log::error!("{what}: {error}");
            return AppError::Unavailable("the disk is full".to_string());
        }
        AppError::Internal(format!("{what}: {error}"))
    }

    /// Whether the same work may succeed later, so it is retried rather than
    /// given up on. An allowlist of the opposite: only a failure known to
    /// come back on the same input is not transient (the query or the data
    /// is wrong: see [`is_deterministic_code`], a decode error, a missing
    /// row, a request that does not validate). Everything else, a database
    /// that is down, read-only, aborted or out of anything, an I/O error, an
    /// internal failure nobody classified, is retried: guessing wrong that
    /// way costs a retry, the other way an event.
    ///
    /// The recovery worker counts only non-transient failures toward
    /// dead-lettering, and a database error answers 503 + `Retry-After`
    /// when it is transient and not a query bug. `Internal` stays a 500 on
    /// the wire: it is where code bugs land too.
    pub fn is_transient(&self) -> bool {
        use sqlx::Error as E;
        match self.kind() {
            AppError::Unavailable(_) | AppError::Internal(_) => true,
            AppError::Database(E::Database(db)) => {
                !db.code().as_deref().is_some_and(is_deterministic_code)
            }
            AppError::Database(e) => !matches!(
                e,
                E::RowNotFound
                    | E::TypeNotFound { .. }
                    | E::ColumnIndexOutOfBounds { .. }
                    | E::ColumnNotFound(_)
                    | E::ColumnDecode { .. }
                    | E::Encode(_)
                    | E::Decode(_)
                    | E::InvalidArgument(_)
            ),
            // The request itself is wrong.
            _ => false,
        }
    }

    /// A database error from a wrong statement or a missing grant: retried
    /// (a fix or a grant makes it pass) but a 500, since it is a bug.
    fn is_query_bug(&self) -> bool {
        matches!(self.kind(), AppError::Database(sqlx::Error::Database(db))
            if db.code().as_deref().is_some_and(is_query_bug_code))
    }

    /// The `error.type` literal this error serialises as. An annotated error
    /// delegates, so it can never report a type its inner error would not.
    fn error_type(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "NotFound",
            AppError::Validation(_) => "ValidationError",
            AppError::Conflict(_) => "Conflict",
            AppError::Unauthorized(_) => "Unauthorized",
            AppError::Forbidden(_) => "Forbidden",
            AppError::PayloadTooLarge(_) => "PayloadTooLarge",
            AppError::Database(_) => "DatabaseError",
            AppError::Internal(_) => "InternalError",
            AppError::Unavailable(_) => "ServiceUnavailable",
            AppError::WithFields(fielded) => fielded.inner.error_type(),
        }
    }

    /// The fields this error blames, empty when it blames none.
    fn field_errors(&self) -> &[FieldError] {
        match self {
            AppError::WithFields(fielded) => &fielded.fields,
            _ => &[],
        }
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            // A bug in the SQL or a missing grant is retried by the spool but
            // is no outage: it stays a 500 on the wire.
            AppError::Database(_) if self.is_transient() && !self.is_query_bug() => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::WithFields(fielded) => fielded.inner.status_code(),
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        // `Unavailable` says what it is in its own words and was logged where
        // it was raised; it has no hidden detail for an incident to point at.
        let incident_id = (status.is_server_error()
            && !matches!(self.kind(), AppError::Unavailable(_)))
        .then(|| uuid::Uuid::new_v4().to_string());

        let message = match &incident_id {
            Some(id) => {
                log::error!("{}", incident_log_line(id, self));
                if let Some(code) = self.db_code() {
                    crate::telemetry::Counters::global().db_error(&code);
                }
                INTERNAL_ERROR_MESSAGE.to_string()
            }
            None => self.to_string(),
        };

        let mut builder = HttpResponse::build(status);
        if let Some(id) = &incident_id {
            builder.insert_header((INCIDENT_ID_HEADER, id.as_str()));
        }
        if status == StatusCode::SERVICE_UNAVAILABLE {
            if matches!(self.kind(), AppError::Database(_)) {
                crate::telemetry::Counters::global().db_unavailable();
            }
            builder.insert_header((
                actix_web::http::header::RETRY_AFTER,
                RETRY_AFTER_SECS.to_string(),
            ));
            // Every category, for the same time. sentry-python reads
            // `Retry-After` only on a 429 but this header on any status.
            builder.insert_header((
                "X-Sentry-Rate-Limits",
                format!("{RETRY_AFTER_SECS}::organization:service_unavailable"),
            ));
        }

        builder.json(ErrorResponse {
            error: ErrorDetail {
                error_type: self.error_type().to_string(),
                message,
                fields: self.field_errors().to_vec(),
                incident_id,
            },
        })
    }
}

/// A database code caused by one event's data, so it comes back on the same
/// payload and nowhere else: on Postgres (five-character SQLSTATEs) data
/// exception (22) and integrity constraint violation (23); on SQLite
/// (extended result codes, under 10000) TOOBIG, CONSTRAINT, MISMATCH and
/// RANGE. Schema, code and permission errors (SQLite ERROR, Postgres 42) are
/// not here: they fail every event alike, see [`is_query_bug_code`].
fn is_deterministic_code(code: &str) -> bool {
    if code.len() == 5 {
        return code.starts_with("22") || code.starts_with("23");
    }
    code.parse::<u32>()
        .is_ok_and(|extended| matches!(extended & 0xff, 18 | 19 | 20 | 25))
}

/// SQLite ERROR (no such table, syntax) or Postgres class 42 (syntax,
/// undefined object, insufficient privilege): the statement or the grants
/// are wrong, not the data.
fn is_query_bug_code(code: &str) -> bool {
    if code.len() == 5 {
        return code.starts_with("42");
    }
    code.parse::<u32>()
        .is_ok_and(|extended| extended & 0xff == 1)
}

/// Result type alias for handlers
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Reads the JSON body an `AppError` actually puts on the wire.
    async fn body_of(error: &AppError) -> serde_json::Value {
        let response = error.error_response();
        let bytes = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("the error body must be readable");
        serde_json::from_slice(&bytes).expect("the error body must be JSON")
    }

    /// A database error carrying `code`, as a driver would report it.
    #[derive(Debug)]
    struct CodedDbError(&'static str);

    impl std::fmt::Display for CodedDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "code {}", self.0)
        }
    }

    impl std::error::Error for CodedDbError {}

    impl sqlx::error::DatabaseError for CodedDbError {
        fn message(&self) -> &str {
            self.0
        }
        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            Some(self.0.into())
        }
        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }
        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
    }

    pub(crate) fn db_error(code: &'static str) -> AppError {
        AppError::Database(sqlx::Error::Database(Box::new(CodedDbError(code))))
    }

    /// An SDK backs off on a 503 with `Retry-After`; on a 500 it may drop the
    /// event or retry at once. Pool exhaustion, a locked SQLite, a full disk
    /// and a shed request all clear up by themselves.
    #[actix_web::test]
    async fn transient_conditions_are_503_with_retry_after() {
        let transient = [
            AppError::Database(sqlx::Error::PoolTimedOut),
            db_error("5"),     // SQLITE_BUSY
            db_error("517"),   // SQLITE_BUSY_SNAPSHOT
            db_error("6"),     // SQLITE_LOCKED
            db_error("13"),    // SQLITE_FULL
            db_error("53100"), // Postgres disk_full
            db_error("53300"), // Postgres too_many_connections
            // The database is down or read-only for a while: the same
            // condition the recovery worker retries, so the same 503.
            AppError::Database(sqlx::Error::PoolClosed),
            AppError::Database(sqlx::Error::Io(std::io::Error::from(
                std::io::ErrorKind::ConnectionRefused,
            ))),
            db_error("8"),     // SQLITE_READONLY
            db_error("08006"), // Postgres connection_failure
            db_error("57P01"), // Postgres admin_shutdown
            db_error("25006"), // Postgres read_only_sql_transaction
            AppError::Unavailable("spool backlog".to_string()),
        ];
        for error in transient {
            let response = error.error_response();
            assert_eq!(
                response.status(),
                StatusCode::SERVICE_UNAVAILABLE,
                "{error}"
            );
            assert_eq!(
                response.headers().get("Retry-After").unwrap(),
                &RETRY_AFTER_SECS.to_string(),
                "{error}"
            );
            // sentry-python backs off on a 5xx only through this header.
            assert_eq!(
                response.headers().get("X-Sentry-Rate-Limits").unwrap(),
                &format!("{RETRY_AFTER_SECS}::organization:service_unavailable"),
                "{error}"
            );
        }
    }

    /// A 503 from the database is counted for `/metrics`; a shed request is
    /// counted where it is shed, not here.
    #[actix_web::test]
    async fn a_503_from_the_database_is_counted() {
        let counters = crate::telemetry::Counters::global();
        let count = || {
            counters
                .render(Default::default(), None)
                .lines()
                .find_map(|l| l.strip_prefix("rustrak_db_unavailable_total "))
                .unwrap()
                .parse::<u64>()
                .unwrap()
        };
        let before = count();
        let _ = AppError::Database(sqlx::Error::PoolTimedOut).error_response();
        assert!(count() > before);
    }

    /// What the recovery worker may count toward dead-lettering: a failure
    /// that will happen again on the same file. Anything the database or the
    /// disk can recover from by itself never counts, so an outage of any
    /// length never dead-letters the backlog.
    #[test]
    fn only_deterministic_failures_count_toward_dead_lettering() {
        for transient in [
            AppError::Database(sqlx::Error::PoolTimedOut),
            AppError::Database(sqlx::Error::PoolClosed),
            AppError::Database(sqlx::Error::WorkerCrashed),
            AppError::Database(sqlx::Error::Io(std::io::Error::from(
                std::io::ErrorKind::ConnectionRefused,
            ))),
            AppError::Database(sqlx::Error::Protocol("reset".to_string())),
            db_error("5"),     // SQLITE_BUSY
            db_error("517"),   // SQLITE_BUSY_SNAPSHOT
            db_error("6"),     // SQLITE_LOCKED
            db_error("13"),    // SQLITE_FULL
            db_error("10"),    // SQLITE_IOERR
            db_error("3850"),  // SQLITE_IOERR_LOCK
            db_error("14"),    // SQLITE_CANTOPEN
            db_error("53100"), // Postgres disk_full
            db_error("53300"), // Postgres too_many_connections
            db_error("57P01"), // Postgres admin_shutdown
            db_error("57P03"), // Postgres cannot_connect_now
            db_error("08006"), // Postgres connection_failure
            db_error("40001"), // Postgres serialization_failure
            db_error("40P01"), // Postgres deadlock_detected
            AppError::Unavailable("the disk is full".to_string()),
            // Not known to be about the payload, so retried, never counted.
            db_error("8"),     // SQLITE_READONLY
            db_error("1032"),  // SQLITE_READONLY_DBMOVED
            db_error("4"),     // SQLITE_ABORT
            db_error("25006"), // Postgres read_only_sql_transaction
            db_error("55P03"), // Postgres lock_not_available
            db_error("XX000"), // Postgres internal_error
            AppError::Internal("Failed to read event file: Too many open files".to_string()),
            AppError::Internal("event read task failed: cancelled".to_string()),
            // Schema, code or a revoked grant: every event fails the same
            // way, so counting them would dead-letter the whole spool.
            db_error("1"),     // SQLITE_ERROR (no such table, ...)
            db_error("42P01"), // Postgres undefined_table
            db_error("42601"), // Postgres syntax_error
            db_error("42501"), // Postgres insufficient_privilege
        ] {
            assert!(transient.is_transient(), "{transient}");
        }
        for deterministic in [
            db_error("2067"),  // SQLITE_CONSTRAINT_UNIQUE
            db_error("19"),    // SQLITE_CONSTRAINT
            db_error("18"),    // SQLITE_TOOBIG
            db_error("20"),    // SQLITE_MISMATCH
            db_error("25"),    // SQLITE_RANGE
            db_error("23505"), // Postgres unique_violation
            db_error("22001"), // Postgres string_data_right_truncation
            AppError::Database(sqlx::Error::RowNotFound),
            AppError::Validation("Invalid event JSON".to_string()),
        ] {
            assert!(!deterministic.is_transient(), "{deterministic}");
        }
    }

    /// A bug stays a 500 with no `Retry-After`: retrying it later does not help.
    #[actix_web::test]
    async fn genuine_failures_stay_500() {
        for error in [
            db_error("1"),     // SQLITE_ERROR
            db_error("42P01"), // Postgres undefined_table
            db_error("42501"), // Postgres insufficient_privilege
            db_error("2067"),  // SQLITE_CONSTRAINT_UNIQUE
            db_error("23505"), // Postgres unique_violation
            AppError::Database(sqlx::Error::RowNotFound),
            AppError::Internal("bug".to_string()),
        ] {
            let response = error.error_response();
            assert_eq!(
                response.status(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "{error}"
            );
            assert!(response.headers().get("Retry-After").is_none(), "{error}");
        }
    }

    /// Disk full on the spool is the same condition as `SQLITE_FULL`.
    #[test]
    fn a_full_disk_is_unavailable() {
        let full = std::io::Error::from_raw_os_error(libc::ENOSPC);
        assert!(matches!(
            AppError::from_io("store", full),
            AppError::Unavailable(_)
        ));
        let other = std::io::Error::from_raw_os_error(libc::EACCES);
        assert!(matches!(
            AppError::from_io("store", other),
            AppError::Internal(_)
        ));
    }

    /// A 500 must never put `Display` on the wire. `sqlx::Error` renders the
    /// constraint, table and column names of whatever query failed, and
    /// `Internal` interpolates arbitrary internal text, so the message a
    /// caller receives has to be a fixed string chosen here.
    #[actix_web::test]
    async fn a_server_error_redacts_its_display_message() {
        let error = AppError::Internal(
            "duplicate key value violates unique constraint \"users_email_key\"".to_string(),
        );

        let body = body_of(&error).await;

        assert_eq!(body["error"]["message"], INTERNAL_ERROR_MESSAGE);
        assert_eq!(body["error"]["type"], "InternalError");
    }

    /// `ResponseError::error_response` never sees the request, so the incident
    /// line cannot name the route. The access log can: it is the one place
    /// that has the method and path, and headers are the only part of a
    /// response it can read. Echoing the id there is what turns one `grep`
    /// into both the route and the detail.
    #[actix_web::test]
    async fn a_server_error_echoes_its_incident_id_in_a_header() {
        let error = AppError::Database(sqlx::Error::PoolClosed);
        let response = error.error_response();

        let header = response
            .headers()
            .get(INCIDENT_ID_HEADER)
            .expect("a 5xx must echo its incident id as a header")
            .to_str()
            .expect("the header must be ASCII")
            .to_string();

        let body = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("the error body must be readable");
        let body: serde_json::Value =
            serde_json::from_slice(&body).expect("the error body must be JSON");

        assert_eq!(
            body["error"]["incident_id"], header,
            "the header and the body must name the same incident"
        );
    }

    /// Nothing was logged for a 4xx, so there is no incident to point at and
    /// the header would be a promise the log cannot keep.
    #[actix_web::test]
    async fn a_client_error_sets_no_incident_header() {
        let response = AppError::Validation("slug is required".to_string()).error_response();

        assert!(response.headers().get(INCIDENT_ID_HEADER).is_none());
    }

    /// The detail the body no longer carries has to land somewhere, keyed by
    /// the id the caller was given. Asserted on the formatted line rather than
    /// on a captured logger: `log` is global state, and the property that
    /// matters is what the line says.
    #[test]
    fn the_log_line_pairs_the_incident_id_with_the_unredacted_detail() {
        let error = AppError::Database(sqlx::Error::RowNotFound);
        let incident_id = "0b3f1c1e-2a4d-4b8e-9f1a-6c7d8e9f0a1b";

        let line = incident_log_line(incident_id, &error);

        assert!(line.contains(incident_id), "line must quote the id: {line}");
        assert!(
            line.contains(&error.to_string()),
            "line must carry the detail the body dropped: {line}"
        );
    }

    /// Redacting without a correlation id would trade a leak for an outage
    /// nobody can diagnose: the caller has a 500 with no handle, and the
    /// operator has a log line with nothing to match it against. Every 5xx
    /// carries an id, and the log line carries the same one.
    #[actix_web::test]
    async fn a_server_error_carries_an_incident_id() {
        let body = body_of(&AppError::Internal("pool exhausted".to_string())).await;

        let incident_id = body["error"]["incident_id"]
            .as_str()
            .expect("a 5xx body must carry an incident_id");
        assert!(
            uuid::Uuid::parse_str(incident_id).is_ok(),
            "incident_id must be a UUID, got {incident_id:?}"
        );
    }

    /// Two 500s are two incidents. Reusing an id, or deriving it from the
    /// error's contents, would collapse unrelated failures into one line in
    /// whatever the operator greps.
    #[actix_web::test]
    async fn each_server_error_gets_its_own_incident_id() {
        let error = AppError::Internal("pool exhausted".to_string());

        let first = body_of(&error).await;
        let second = body_of(&error).await;

        assert_ne!(
            first["error"]["incident_id"], second["error"]["incident_id"],
            "each response must be its own incident"
        );
    }

    /// A 4xx is not an incident: it is the caller's input being rejected, the
    /// message already says everything, and nothing was logged for an id to
    /// point at. The key is absent, never null, matching how `fields` and
    /// `FieldError::message` already behave.
    #[actix_web::test]
    async fn a_client_error_carries_no_incident_id() {
        let body = body_of(&AppError::NotFound("project".to_string())).await;

        assert!(
            body["error"].get("incident_id").is_none(),
            "a 4xx body must omit incident_id entirely, got {body}"
        );
    }

    /// The split is on the status, not on the variant. A 4xx message describes
    /// the caller's own input, is written by hand at the call site, and is the
    /// only thing that makes a 409 actionable, so redacting it would be a
    /// regression dressed as hardening.
    #[actix_web::test]
    async fn a_client_error_keeps_its_display_message() {
        let error = AppError::Conflict("Project with slug 'api' already exists".to_string());

        let body = body_of(&error).await;

        assert_eq!(
            body["error"]["message"],
            "Conflict: Project with slug 'api' already exists"
        );
    }

    /// A code other than `custom` must serialise without a `message` key at
    /// all, not with a null.
    #[test]
    fn a_non_custom_code_serialises_no_message_key() {
        let json = serde_json::to_value(FieldError::new("name", FieldErrorCode::TooLong))
            .expect("FieldError must serialise");
        assert_eq!(
            json,
            serde_json::json!({ "field": "name", "code": "too_long" })
        );
    }
}
