pub(super) fn restore_error(message: impl Into<String>) -> crate::RestoreValidationError {
    crate::RestoreValidationError::new(message)
}

pub(super) fn map_backup_validation<T>(
    result: andromeda_backup::BackupResult<T>,
) -> crate::RestoreResult<T> {
    result.map_err(|error| restore_error(error.message()))
}
