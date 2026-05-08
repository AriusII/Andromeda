use std::{error::Error, fmt};

pub type BackupResult<T> = Result<T, BackupValidationError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupValidationError {
    message: String,
}

impl BackupValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for BackupValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for BackupValidationError {}

pub(crate) fn backup_error(message: impl Into<String>) -> BackupValidationError {
    BackupValidationError::new(message)
}
