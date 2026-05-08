use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_storage_index::validate_btree_key_format_identity_parts;

pub(super) fn validate_identity_parts(
    major: u32,
    minor: u32,
    max_key_size: u16,
) -> AndromedaResult<()> {
    validate_btree_key_format_identity_parts(major, minor, max_key_size)
        .map_err(|err| AndromedaError::new(AndromedaErrorKind::Storage, err.message()))
}
