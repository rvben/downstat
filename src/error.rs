//! Error type, the stable error `kind` set, and the exit-code contract.
//!
//! Errors are reported as a clispec structured envelope on the last line of
//! stderr: `{"error":{"kind":...,"message":...,"exit_code":...,"hint":...}}`.
//!
//! Exit codes (also declared in the schema):
//! - `1` no data (a queried name was found on no registry)
//! - `2` a network or parse failure
//! - `3` usage error (bad arguments / config)

use thiserror::Error;

/// All failure modes of a downstat run.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DownstatError {
    /// Invalid command-line arguments or config (also wraps clap errors).
    #[error("{message}")]
    Usage { message: String },

    /// A registry request failed at the network/transport level.
    #[error("network error: {message}")]
    Http { message: String },

    /// A registry returned a response that could not be parsed.
    #[error("could not parse the {registry} response: {message}")]
    Parse {
        registry: &'static str,
        message: String,
    },

    /// A queried name was found on no registry at all.
    #[error("{name} was not found on any registry")]
    NoData { name: String },
}

impl DownstatError {
    /// Stable snake_case identifier consumers branch on (the schema `errors` set).
    pub fn kind(&self) -> &'static str {
        match self {
            DownstatError::Usage { .. } => "usage",
            DownstatError::Http { .. } => "http",
            DownstatError::Parse { .. } => "parse",
            DownstatError::NoData { .. } => "no_data",
        }
    }

    /// Whether a consumer should retry.
    pub fn retryable(&self) -> bool {
        matches!(self, DownstatError::Http { .. })
    }

    /// Actionable remediation, when there is one.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            DownstatError::Usage { .. } => Some("see `downstat --help` or `downstat schema`"),
            DownstatError::NoData { .. } => {
                Some("check the name, or use --only to target a specific registry")
            }
            _ => None,
        }
    }

    /// The process exit code associated with this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            DownstatError::NoData { .. } => 1,
            DownstatError::Http { .. } | DownstatError::Parse { .. } => 2,
            DownstatError::Usage { .. } => 3,
        }
    }
}
