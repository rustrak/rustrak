//! The Rustrak command-line client.
//!
//! The binary is a thin layer over the REST API the server already serves, so
//! every command is a request a signed-in dashboard user could make by hand.
//! Authentication is the one place it does more than proxy: the API only mints
//! tokens from a *session*, so `rustrak auth login` signs in with an email and
//! password, trades that session cookie for a long-lived token, and stores the
//! token. Every later command is a plain `Authorization: Bearer` call.

pub mod api;
pub mod cli;
pub mod client;
pub mod commands;
pub mod config;
pub mod error;
pub mod output;

pub use error::{Error, Result};
