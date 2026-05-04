use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(super) fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

pub(super) fn resource_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Resource, message)
}
