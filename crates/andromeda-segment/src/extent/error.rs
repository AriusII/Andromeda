use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(crate) fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
