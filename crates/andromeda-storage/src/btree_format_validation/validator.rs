use super::{BTreeKeyFormatIdentity, BTreeOperationType};
use crate::format_version::FormatVersion;
use andromeda_core::AndromedaResult;
use andromeda_storage_index::KeyV1FormatGate;

/// KeyV1 format validation gate.
///
/// This gate enforces format validation before B-Tree operations and rejects
/// mutations with explicit deferral messages. It prevents silent data loss or
/// corruption while durable mutation support is not promoted.
#[derive(Debug, Clone)]
pub struct KeyV1FormatValidator {
    storage_version: FormatVersion,
    gate: KeyV1FormatGate,
}

impl KeyV1FormatValidator {
    /// Create a new validator for a B-Tree index.
    pub fn new(storage_version: FormatVersion, key_format: BTreeKeyFormatIdentity) -> Self {
        Self {
            storage_version,
            gate: KeyV1FormatGate::new(storage_version.major, storage_version.minor, key_format),
        }
    }

    /// Validate that an operation is allowed on this index.
    pub fn validate_operation(&self, operation: BTreeOperationType) -> AndromedaResult<()> {
        self.gate.validate_operation(operation)
    }

    /// Check if this format is compatible with the current storage version.
    pub fn is_format_compatible(&self) -> bool {
        self.gate.is_format_compatible()
    }

    /// Get the format identity for this index.
    pub fn format_identity(&self) -> BTreeKeyFormatIdentity {
        self.gate.format_identity()
    }

    /// Get storage version for this validator.
    pub fn storage_version(&self) -> FormatVersion {
        self.storage_version
    }
}
