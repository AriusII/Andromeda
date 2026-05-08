use andromeda_backup::BackupResult;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

pub(super) fn backup_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

pub(crate) fn backup_validation_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

pub(crate) fn restore_validation_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

pub(crate) fn map_backup_validation<T>(result: BackupResult<T>) -> AndromedaResult<T> {
    result.map_err(|error| backup_validation_error(error.message()))
}
