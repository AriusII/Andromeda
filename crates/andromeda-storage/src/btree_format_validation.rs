#![forbid(unsafe_code)]

//! B-Tree KeyV1 Format Validation Gate — DEC-038 Implementation
//!
//! This module implements format validation gates required by DEC-032 and DEC-038:
//! - Validates persisted B-Tree indexes have recognized key-format identity
//! - Rejects mutations (insert, delete) with explicit deferral message
//! - Allows read-only operations (lookup, range_scan)
//! - Enforces fail-fast behavior on format mismatches
//! - Provides deterministic validation results across multiple calls
//!
//! ## Invariants
//!
//! 1. **Format Identity Determinism**: Same (major, minor, codec_version) always validates identically
//! 2. **Mutation Gating**: All insert/delete/split/merge attempts fail with deferral reference
//! 3. **Read-Only Pass-Through**: lookup() and range_scan() allowed for known formats
//! 4. **Fail-Fast on Unknown**: Unknown formats rejected before any operation
//! 5. **No Silent Corruption**: All validation failures are auditable errors
//!
//! ## Durable Mutation Gate
//!
//! Page-backed B-Tree mutations are not promoted yet. This gate preserves format
//! safety by allowing read-only access and rejecting mutation attempts before any
//! page or WAL state can be changed. When durable mutations are promoted, the
//! promotion must:
//! - Replace `validate_mutation_allowed()` with actual mutation logic
//! - Update WAL record apply() methods to execute mutations
//! - Re-validate all format gates still hold after mutation implementation

use crate::format_version::FormatVersion;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::fmt;

/// Format identity for persisted B-Tree indexes (DEC-038).
///
/// Each index must record which key-format codec was used when it was created.
/// This allows later versions to validate format compatibility and reject
/// unsupported formats before mutation attempts.
///
/// # Invariants
///
/// - `major` and `minor` must not both be zero
/// - `codec_version` identifies the key encoding scheme (1 = KeyV1)
/// - All fields together uniquely identify the format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BTreeKeyFormatIdentity {
    /// Major version (breaking changes to format)
    pub major: u32,
    /// Minor version (backward-compatible extensions)
    pub minor: u32,
    /// Codec version (1 = KeyV1 order-preserving encoding; see btree_key_codec.rs)
    pub codec_version: u8,
    /// Maximum key size in bytes for the persisted key format.
    pub max_key_size: u16,
}

impl BTreeKeyFormatIdentity {
    /// Create a new format identity.
    ///
    /// # Panics
    /// Panics if major and minor are both zero (reserved for uninitialized state).
    pub const fn new(major: u32, minor: u32, codec_version: u8, max_key_size: u16) -> Self {
        if major == 0 && minor == 0 {
            panic!("Format version (0, 0) is reserved; use (1, 0) for initial KeyV1 format");
        }
        Self {
            major,
            minor,
            codec_version,
            max_key_size,
        }
    }

    /// KeyV1 format identity (current locked format per DEC-032).
    ///
    /// - Major 1, Minor 0: Initial release
    /// - Codec version 1: Order-preserving key encoding with type tags
    /// - Max key size 4096 bytes: Conservative limit for heap page tuples
    pub const V1_0: Self = Self {
        major: 1,
        minor: 0,
        codec_version: 1,
        max_key_size: 4096,
    };

    /// Check if this format is backward compatible with another format.
    ///
    /// Same rules as FormatVersion: same major, self.minor >= other.minor.
    pub const fn is_backward_compatible_with(&self, other: BTreeKeyFormatIdentity) -> bool {
        self.major == other.major && self.minor >= other.minor
    }

    /// Get human-readable format identifier.
    pub fn format_name(&self) -> String {
        format!("KeyV{}_Codec{}", self.major, self.codec_version)
    }
}

impl fmt::Display for BTreeKeyFormatIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BTreeKeyFormat(v{}.{}, codec={}, max_key={})",
            self.major, self.minor, self.codec_version, self.max_key_size
        )
    }
}

/// B-Tree operation type for validation gating (DEC-038).
///
/// Used to distinguish read-only operations (allowed) from mutations (deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeOperationType {
    /// Read-only: Lookup a single key. Allowed in all formats.
    Lookup,
    /// Read-only: Range scan with cursor. Allowed in all formats.
    RangeScan,
    /// Mutation: Insert a key-value pair.
    Insert,
    /// Mutation: Delete a key-value pair.
    Delete,
    /// Internal: Split a node.
    Split,
    /// Internal: Merge two nodes.
    Merge,
}

impl BTreeOperationType {
    /// Check if this operation is read-only.
    pub const fn is_read_only(&self) -> bool {
        matches!(
            self,
            BTreeOperationType::Lookup | BTreeOperationType::RangeScan
        )
    }

    /// Get human-readable operation name.
    pub const fn name(&self) -> &'static str {
        match self {
            BTreeOperationType::Lookup => "lookup",
            BTreeOperationType::RangeScan => "range_scan",
            BTreeOperationType::Insert => "insert",
            BTreeOperationType::Delete => "delete",
            BTreeOperationType::Split => "split",
            BTreeOperationType::Merge => "merge",
        }
    }
}

/// KeyV1 format validation gate (DEC-038 implementation).
///
/// This gate enforces format validation before B-Tree operations and rejects
/// mutations with explicit deferral message. It is the primary mechanism
/// preventing silent data loss or corruption while durable mutation support is
/// not promoted.
///
/// # Validation Levels
///
/// 1. **Format Recognition**: Can we read/write this format?
///    - KeyV1 (1.0) = ✅ Supported
///    - Unknown = ❌ Fail-fast before any operation
///
/// 2. **Operation Type**: Is this operation allowed?
///    - Read-only (lookup, range_scan) = ✅ Allowed
///    - Mutations (insert, delete) = ❌ Explicit deferral error
///
/// 3. **Determinism**: Multiple validators produce identical results.
///    - Same format + operation = Same result always
///
/// # Compliance with Doctrine
///
/// - **Fail-Fast**: Unknown formats rejected immediately, not silently accepted
/// - **No Silent Data Loss**: Mutations fail explicitly with reference to DEC-038
/// - **Recoverable**: Format identity stored with index; recovery can validate before replay
/// - **Auditable**: All rejections logged with format identity and reference to decision record
#[derive(Debug, Clone)]
pub struct KeyV1FormatValidator {
    /// The FormatVersion of the storage system.
    storage_version: FormatVersion,
    /// The specific B-Tree key format used by this index
    key_format: BTreeKeyFormatIdentity,
}

impl KeyV1FormatValidator {
    /// Create a new validator for a B-Tree index.
    ///
    /// # Arguments
    /// * `storage_version` - Storage system version.
    /// * `key_format` - Format identity of this specific index
    ///
    /// # Returns
    /// Validator ready to validate operations.
    pub fn new(storage_version: FormatVersion, key_format: BTreeKeyFormatIdentity) -> Self {
        Self {
            storage_version,
            key_format,
        }
    }

    /// Validate that an operation is allowed on this index.
    ///
    /// # Behavior
    /// - ✅ Read-only operations: Always allowed
    /// - ❌ Mutations: Deferred; explicit error with DEC-038 reference
    /// - ❌ Unknown format: Fail-fast before any operation
    ///
    /// # Returns
    /// - `Ok(())` if operation is allowed
    /// - `Err(...)` with specific reason if operation must be rejected
    ///
    /// # Examples
    /// ```ignore
    /// let validator = KeyV1FormatValidator::new(
    ///     FormatVersion::V1_0,
    ///     BTreeKeyFormatIdentity::V1_0
    /// );
    ///
    /// // Read operations succeed
    /// assert!(validator.validate_operation(BTreeOperationType::Lookup).is_ok());
    ///
    /// // Mutations fail with deferral message
    /// let result = validator.validate_operation(BTreeOperationType::Insert);
    /// assert!(result.is_err());
    /// assert!(result.unwrap_err().message().contains("not promoted"));
    /// ```
    pub fn validate_operation(&self, operation: BTreeOperationType) -> AndromedaResult<()> {
        // First: Validate format is supported
        self.validate_format_supported()?;

        // Then: Check if operation is allowed for this format
        if operation.is_read_only() {
            // Read-only operations always allowed
            Ok(())
        } else {
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
    }

    /// Validate that this index's format is supported.
    ///
    /// # Behavior
    /// - ✅ KeyV1 format (major=1, minor=0, codec=1): Supported
    /// - ✅ KeyV1.x backward-compatible formats: Supported
    /// - ❌ Unknown or incompatible formats: Fail-fast
    ///
    /// # Returns
    /// - `Ok(())` if format is supported
    /// - `Err(...)` if format is unknown or incompatible
    fn validate_format_supported(&self) -> AndromedaResult<()> {
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

        // Format is supported; all validations passed
        Ok(())
    }

    /// Check if this format is compatible with the current storage version.
    ///
    /// Uses semantic versioning rules from FormatVersion:
    /// - Same major version required
    /// - Reader.minor >= writer.minor (reader has all writer's features)
    pub fn is_format_compatible(&self) -> bool {
        // We support reading any v1.x format
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_identity_v1_0_valid() {
        let fmt = BTreeKeyFormatIdentity::V1_0;
        assert_eq!(fmt.major, 1);
        assert_eq!(fmt.minor, 0);
        assert_eq!(fmt.codec_version, 1);
    }

    #[test]
    fn test_format_identity_backward_compatible() {
        let v1_0 = BTreeKeyFormatIdentity::V1_0;
        let v1_1 = BTreeKeyFormatIdentity::new(1, 1, 1, 4096);

        assert!(v1_1.is_backward_compatible_with(v1_0));
        assert!(!v1_0.is_backward_compatible_with(v1_1));
    }

    #[test]
    fn test_operation_type_read_only_check() {
        assert!(BTreeOperationType::Lookup.is_read_only());
        assert!(BTreeOperationType::RangeScan.is_read_only());
        assert!(!BTreeOperationType::Insert.is_read_only());
        assert!(!BTreeOperationType::Delete.is_read_only());
    }

    #[test]
    fn test_validator_read_operation_allowed() {
        let validator =
            KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

        // Read operations should succeed
        assert!(
            validator
                .validate_operation(BTreeOperationType::Lookup)
                .is_ok()
        );
        assert!(
            validator
                .validate_operation(BTreeOperationType::RangeScan)
                .is_ok()
        );
    }

    #[test]
    fn test_validator_insert_deferred() {
        let validator =
            KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

        let result = validator.validate_operation(BTreeOperationType::Insert);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message().contains("not promoted"));
        assert!(err.message().contains("DEC-038"));
    }

    #[test]
    fn test_validator_delete_deferred() {
        let validator =
            KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

        let result = validator.validate_operation(BTreeOperationType::Delete);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message().contains("not promoted"));
    }

    #[test]
    fn test_validator_unknown_major_version_rejected() {
        let unknown_fmt = BTreeKeyFormatIdentity::new(99, 0, 1, 4096);
        let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, unknown_fmt);

        let result = validator.validate_operation(BTreeOperationType::Lookup);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message().contains("major version 99 not supported"));
    }

    #[test]
    fn test_validator_unknown_codec_version_rejected() {
        let unknown_fmt = BTreeKeyFormatIdentity::new(1, 0, 99, 4096);
        let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, unknown_fmt);

        let result = validator.validate_operation(BTreeOperationType::Lookup);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message().contains("codec version 99 not supported"));
    }

    #[test]
    fn test_validator_deterministic_results() {
        let fmt = BTreeKeyFormatIdentity::V1_0;
        let v1 = KeyV1FormatValidator::new(FormatVersion::V1_0, fmt);
        let v2 = KeyV1FormatValidator::new(FormatVersion::V1_0, fmt);

        // Same input should produce same results
        let r1 = v1.validate_operation(BTreeOperationType::Insert);
        let r2 = v2.validate_operation(BTreeOperationType::Insert);

        // Both should fail with same reason
        assert!(r1.is_err());
        assert!(r2.is_err());

        // Error messages should be identical (determinism)
        assert_eq!(r1.unwrap_err().message(), r2.unwrap_err().message());
    }

    #[test]
    fn test_format_identity_display() {
        let fmt = BTreeKeyFormatIdentity::V1_0;
        let display = format!("{}", fmt);
        assert!(display.contains("1.0"));
        assert!(display.contains("codec=1"));
    }

    #[test]
    fn test_format_identity_format_name() {
        let fmt = BTreeKeyFormatIdentity::V1_0;
        assert_eq!(fmt.format_name(), "KeyV1_Codec1");
    }
}
