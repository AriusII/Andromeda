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
