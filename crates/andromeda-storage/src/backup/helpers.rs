use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(super) fn backup_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
