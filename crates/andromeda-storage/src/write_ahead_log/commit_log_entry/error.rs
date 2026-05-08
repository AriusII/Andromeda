use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(crate) fn storage_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}

pub(crate) fn transaction_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Transaction, msg)
}
