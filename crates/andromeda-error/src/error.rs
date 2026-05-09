//! Core error categories and result alias.

use std::{error::Error as StdError, fmt, sync::Arc};

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
    pub const fn code(self) -> &'static str {
        match self {
            Self::Catalog => "AE0001",
            Self::Contract => "AE0002",
            Self::Execution => "AE0003",
            Self::Internal => "AE0004",
            Self::Protocol => "AE0005",
            Self::Resource => "AE0006",
            Self::Security => "AE0007",
            Self::Srpl => "AE0008",
            Self::Storage => "AE0009",
            Self::Timeout => "AE0010",
            Self::Transaction => "AE0011",
            Self::Transport => "AE0012",
        }
    }

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

#[derive(Debug, Clone)]
pub struct AndromedaError {
    kind: AndromedaErrorKind,
    message: String,
    source: Option<Arc<dyn StdError + Send + Sync + 'static>>,
}

impl AndromedaError {
    pub fn new(kind: AndromedaErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        kind: AndromedaErrorKind,
        message: impl Into<String>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            source: Some(Arc::new(source)),
        }
    }

    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    pub fn kind(&self) -> AndromedaErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn StdError + 'static))
    }

    pub fn render_diagnostic(&self) -> String {
        let mut diagnostic = format!("{} {}: {}", self.code(), self.kind, self.message);
        let mut next_source = self.source();

        while let Some(source) = next_source {
            diagnostic.push_str("; source: ");
            diagnostic.push_str(&source.to_string());
            next_source = source.source();
        }

        diagnostic
    }
}

impl fmt::Display for AndromedaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl PartialEq for AndromedaError {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.message == other.message
    }
}

impl Eq for AndromedaError {}

impl StdError for AndromedaError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kind_has_stable_label() {
        let labels = [
            (AndromedaErrorKind::Catalog, "catalog"),
            (AndromedaErrorKind::Contract, "contract"),
            (AndromedaErrorKind::Execution, "execution"),
            (AndromedaErrorKind::Internal, "internal"),
            (AndromedaErrorKind::Protocol, "protocol"),
            (AndromedaErrorKind::Resource, "resource"),
            (AndromedaErrorKind::Security, "security"),
            (AndromedaErrorKind::Srpl, "srpl"),
            (AndromedaErrorKind::Storage, "storage"),
            (AndromedaErrorKind::Timeout, "timeout"),
            (AndromedaErrorKind::Transaction, "transaction"),
            (AndromedaErrorKind::Transport, "transport"),
        ];

        for (kind, label) in labels {
            assert_eq!(kind.as_str(), label);
            assert_eq!(kind.to_string(), label);
        }
    }

    #[test]
    fn error_kind_has_stable_code() {
        let codes = [
            (AndromedaErrorKind::Catalog, "AE0001"),
            (AndromedaErrorKind::Contract, "AE0002"),
            (AndromedaErrorKind::Execution, "AE0003"),
            (AndromedaErrorKind::Internal, "AE0004"),
            (AndromedaErrorKind::Protocol, "AE0005"),
            (AndromedaErrorKind::Resource, "AE0006"),
            (AndromedaErrorKind::Security, "AE0007"),
            (AndromedaErrorKind::Srpl, "AE0008"),
            (AndromedaErrorKind::Storage, "AE0009"),
            (AndromedaErrorKind::Timeout, "AE0010"),
            (AndromedaErrorKind::Transaction, "AE0011"),
            (AndromedaErrorKind::Transport, "AE0012"),
        ];

        for (kind, code) in codes {
            assert_eq!(kind.code(), code);
            assert_eq!(AndromedaError::new(kind, "typed failure").code(), code);
        }
    }

    #[test]
    fn error_display_preserves_kind_and_message() {
        let error = AndromedaError::new(AndromedaErrorKind::Contract, "hash mismatch");

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert_eq!(error.code(), "AE0002");
        assert_eq!(error.message(), "hash mismatch");
        assert_eq!(error.to_string(), "contract: hash mismatch");
    }

    #[test]
    fn diagnostic_rendering_is_code_kind_and_message() {
        let error = AndromedaError::new(AndromedaErrorKind::Contract, "hash mismatch");

        assert_eq!(error.render_diagnostic(), "AE0002 contract: hash mismatch");
    }
}
