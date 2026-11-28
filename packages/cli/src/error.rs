//! One error type for the whole binary, whose `exit_code` is what the process
//! returns. A command reports a failure; `main` decides the status.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// A field the server rejected, as `{"field": "message"}` flattened.
#[derive(Debug, Clone)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum Error {
    /// The request never got an answer: DNS, TLS, a timeout, a refused socket.
    #[error("could not reach the server: {0}")]
    Http(#[from] reqwest::Error),

    /// The server answered with an error document.
    #[error("{message}")]
    Api {
        status: u16,
        kind: String,
        message: String,
        fields: Vec<FieldError>,
    },

    /// No token for this profile, or the one stored is rejected.
    #[error("{0}")]
    Unauthenticated(String),

    /// The config file is missing, malformed or holds no usable profile.
    #[error("{0}")]
    Config(String),

    /// A value the user passed that cannot be used as given.
    #[error("{0}")]
    Invalid(String),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// A response body that did not deserialize.
    #[error("unexpected response from {path}: {source}")]
    Decode {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

impl Error {
    /// The process status. Usage errors are clap's 2; these are ours.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Unauthenticated(_) => 3,
            Error::Api { status, .. } if *status == 401 => 3,
            Error::Api { status, .. } if *status == 403 => 4,
            Error::Api { status, .. } if *status == 404 => 5,
            _ => 1,
        }
    }

    /// The `fields` the server named, for a caller that wants to print them
    /// under the message.
    pub fn fields(&self) -> &[FieldError] {
        match self {
            Error::Api { fields, .. } => fields,
            _ => &[],
        }
    }
}
