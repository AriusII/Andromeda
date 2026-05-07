use andromeda_core::{AndromedaError, AndromedaErrorKind};

use crate::Lsn;

/// Helper to convert io::Error to AndromedaError for codec operations.
pub(super) fn io_error(e: std::io::Error) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Storage,
        format!("catalog WAL codec I/O error: {}", e),
    )
}

pub(super) fn catalog_wal_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message.into())
}

pub(super) fn lsn_order_error(previous: Lsn, current: Lsn) -> AndromedaError {
    catalog_wal_error(format!(
        "catalog WAL replay requires strictly increasing LSN order: previous {}, current {}",
        previous.get(),
        current.get()
    ))
}
