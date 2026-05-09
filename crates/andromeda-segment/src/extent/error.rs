use andromeda_error::{AndromedaError, AndromedaErrorKind};

pub(crate) fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
