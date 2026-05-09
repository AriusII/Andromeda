//! Compatibility facade for index-owned B-Tree KeyV1 format validation.
//!
//! The index crate owns the validation gate. Storage keeps only the historical
//! constructor shape that accepts `FormatVersion` and returns it to callers.

use andromeda_core::AndromedaResult;
pub use andromeda_storage_index::{
    BTreeFormatIdentityError, BTreeKeyFormatIdentity, BTreeOperationType,
};

use crate::format_version::FormatVersion;

#[derive(Debug, Clone)]
pub struct KeyV1FormatValidator {
    storage_version: FormatVersion,
    inner: andromeda_storage_index::KeyV1FormatValidator,
}

impl KeyV1FormatValidator {
    pub fn new(storage_version: FormatVersion, key_format: BTreeKeyFormatIdentity) -> Self {
        Self {
            storage_version,
            inner: andromeda_storage_index::KeyV1FormatValidator::new(
                storage_version.major,
                storage_version.minor,
                key_format,
            ),
        }
    }

    pub fn validate_operation(&self, operation: BTreeOperationType) -> AndromedaResult<()> {
        self.inner.validate_operation(operation)
    }

    pub fn is_format_compatible(&self) -> bool {
        self.inner.is_format_compatible()
    }

    pub fn format_identity(&self) -> BTreeKeyFormatIdentity {
        self.inner.format_identity()
    }

    pub fn storage_version(&self) -> FormatVersion {
        self.storage_version
    }
}
