//! Error types and result type alias.
//!
//! This module defines the error handling system for Andromeda.
//!
//! ## Error Kinds
//!
//! Errors are categorized by `AndromedaErrorKind`:
//! - **Catalog**: Object definition or versioning errors
//! - **Contract**: Procedure contract mismatch or incompatibility
//! - **Execution**: Runtime execution errors
//! - **Internal**: Unexpected internal state (bugs)
//! - **Protocol**: Wire protocol violations
//! - **Resource**: Resource exhaustion or limit violations
//! - **Security**: Authentication or authorization failures
//! - **Srpl**: SRPL program errors
//! - **Storage**: Database storage or persistence errors
//! - **Transaction**: Transaction management errors
//!
//! ## Usage
//!
//! Create errors with `AndromedaError::new()`:
//!
//! ```ignore
//! use andromeda_core::{AndromedaError, AndromedaErrorKind};
//!
//! let err = AndromedaError::new(
//!     AndromedaErrorKind::Contract,
//!     "procedure contract hash mismatch"
//! );
//! ```
//!
//! Errors implement `std::error::Error` and format with `Display`.

use std::fmt;

pub type AndromedaResult<T> = Result<T, AndromedaError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndromedaErrorKind {
    Catalog,
    Contract,
    Execution,
    Internal,
    Protocol,
    Resource,
    Security,
    Srpl,
    Storage,
    /// A deadline was exceeded at the QUIC stream or lock-wait layer.
    ///
    /// Classified as `Persistent` in `ErrorRetryability`: a timeout indicates
    /// the invocation budget has been consumed and must not be retried by the
    /// standard `RetryPolicy`. Lock-wait retries are handled internally before
    /// a `Timeout` error surfaces to the caller.
    ///
    /// Added in Wave 13, Batch 18. Full invocation deadline enforcement
    /// (admission → WAL commit) is Wave 14+; see `SCOPED_INVOCATION_TIMEOUT.md`.
    Timeout,
    Transaction,
    Transport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndromedaError {
    kind: AndromedaErrorKind,
    message: String,
}

impl AndromedaError {
    pub fn new(kind: AndromedaErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> AndromedaErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AndromedaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for AndromedaError {}
