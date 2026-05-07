use super::{BTreeKeyFormatIdentity, BTreeOperationType, validation::validate_identity_parts};
use crate::format_version::FormatVersion;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// KeyV1 format validation gate.
///
/// This gate enforces format validation before B-Tree operations and rejects
/// mutations with explicit deferral messages. It prevents silent data loss or
/// corruption while durable mutation support is not promoted.
#[derive(Debug, Clone)]
pub struct KeyV1FormatValidator {
    storage_version: FormatVersion,
    key_format: BTreeKeyFormatIdentity,
}

impl KeyV1FormatValidator {
    /// Create a new validator for a B-Tree index.
    pub fn new(storage_version: FormatVersion, key_format: BTreeKeyFormatIdentity) -> Self {
        Self {
            storage_version,
            key_format,
        }
    }

    /// Validate that an operation is allowed on this index.
    pub fn validate_operation(&self, operation: BTreeOperationType) -> AndromedaResult<()> {
        self.validate_format_supported()?;

        if operation.is_read_only() {
            return Ok(());
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "B-Tree mutation '{}' is not promoted for durable page-backed indexes. \
                 See DEC-038 for rationale. Index format: {}. \
                 If you believe this is an error, escalate to Release Governance.",
                operation.name(),
                self.key_format
            ),
        ))
    }

    fn validate_format_supported(&self) -> AndromedaResult<()> {
        validate_identity_parts(
            self.key_format.major,
            self.key_format.minor,
            self.key_format.max_key_size,
        )?;

        if self.key_format.major != 1 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "B-Tree format major version {} not supported. \
                     This database only supports v1.x formats (KeyV1). \
                     Format details: {}. \
                     Recovery mode: ForensicStart (read-only) only. \
                     For compatibility details, see DEC-038.",
                    self.key_format.major, self.key_format
                ),
            ));
        }

        if self.storage_version.major != self.key_format.major {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "B-Tree storage format version {}.{} is incompatible with index key format {}.{}. \
                     Recovery mode: ForensicStart (read-only) only. \
                     For compatibility details, see DEC-038.",
                    self.storage_version.major,
                    self.storage_version.minor,
                    self.key_format.major,
                    self.key_format.minor
                ),
            ));
        }

        if self.key_format.codec_version != 1 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "B-Tree codec version {} not supported. \
                     Expected codec version 1 (KeyV1 order-preserving encoding). \
                     Format details: {}. \
                     Possible causes: Future codec version, or corrupted format metadata. \
                     See DEC-038 and btree_key_codec.rs for details.",
                    self.key_format.codec_version, self.key_format
                ),
            ));
        }

        Ok(())
    }

    /// Check if this format is compatible with the current storage version.
    pub fn is_format_compatible(&self) -> bool {
        self.storage_version.major == 1 && self.key_format.major == 1
    }

    /// Get the format identity for this index.
    pub fn format_identity(&self) -> BTreeKeyFormatIdentity {
        self.key_format
    }

    /// Get storage version for this validator.
    pub fn storage_version(&self) -> FormatVersion {
        self.storage_version
    }
}
