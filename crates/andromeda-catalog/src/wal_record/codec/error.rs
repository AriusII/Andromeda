//! Error helpers for the catalog WAL payload codec.

use andromeda_core::{AndromedaError, AndromedaErrorKind};

pub(super) fn catalog_error(message: &str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Catalog, message)
}
