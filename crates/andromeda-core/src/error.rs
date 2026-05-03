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
