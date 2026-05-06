//! Core error categories and result alias.

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
    /// End-to-end invocation deadline enforcement is layered above this error
    /// kind; callers should treat a surfaced timeout as the final budget result.
    Timeout,
    Transaction,
    Transport,
}

impl AndromedaErrorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Contract => "contract",
            Self::Execution => "execution",
            Self::Internal => "internal",
            Self::Protocol => "protocol",
            Self::Resource => "resource",
            Self::Security => "security",
            Self::Srpl => "srpl",
            Self::Storage => "storage",
            Self::Timeout => "timeout",
            Self::Transaction => "transaction",
            Self::Transport => "transport",
        }
    }
}

impl fmt::Display for AndromedaErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for AndromedaError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kind_has_stable_label() {
        assert_eq!(AndromedaErrorKind::Security.as_str(), "security");
        assert_eq!(AndromedaErrorKind::Timeout.to_string(), "timeout");
    }

    #[test]
    fn error_display_preserves_kind_and_message() {
        let error = AndromedaError::new(AndromedaErrorKind::Contract, "hash mismatch");

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert_eq!(error.message(), "hash mismatch");
        assert_eq!(error.to_string(), "contract: hash mismatch");
    }
}
