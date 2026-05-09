use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{BTreeKeyFormatIdentity, BTreeOperationType, validate_btree_key_format_identity_parts};

/// Index-owned KeyV1 format validation gate.
#[derive(Debug, Clone, Copy)]
pub struct KeyV1FormatGate {
    storage_version_major: u32,
    storage_version_minor: u32,
    key_format: BTreeKeyFormatIdentity,
}

impl KeyV1FormatGate {
    pub const fn new(
        storage_version_major: u32,
        storage_version_minor: u32,
        key_format: BTreeKeyFormatIdentity,
    ) -> Self {
        Self {
            storage_version_major,
            storage_version_minor,
            key_format,
        }
    }

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

    pub const fn is_format_compatible(&self) -> bool {
        self.storage_version_major == 1 && self.key_format.major == 1
    }

    pub const fn format_identity(&self) -> BTreeKeyFormatIdentity {
        self.key_format
    }

    pub const fn storage_version_parts(&self) -> (u32, u32) {
        (self.storage_version_major, self.storage_version_minor)
    }

    fn validate_format_supported(&self) -> AndromedaResult<()> {
        validate_btree_key_format_identity_parts(
            self.key_format.major,
            self.key_format.minor,
            self.key_format.max_key_size,
        )
        .map_err(|err| AndromedaError::new(AndromedaErrorKind::Storage, err.message()))?;

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

        if self.storage_version_major != self.key_format.major {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "B-Tree storage format version {}.{} is incompatible with index key format {}.{}. \
                     Recovery mode: ForensicStart (read-only) only. \
                     For compatibility details, see DEC-038.",
                    self.storage_version_major,
                    self.storage_version_minor,
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
}

/// KeyV1 validation wrapper that stores storage-version parts beside the gate.
#[derive(Debug, Clone)]
pub struct KeyV1FormatValidator {
    storage_version_major: u32,
    storage_version_minor: u32,
    gate: KeyV1FormatGate,
}

impl KeyV1FormatValidator {
    pub const fn new(
        storage_version_major: u32,
        storage_version_minor: u32,
        key_format: BTreeKeyFormatIdentity,
    ) -> Self {
        Self {
            storage_version_major,
            storage_version_minor,
            gate: KeyV1FormatGate::new(storage_version_major, storage_version_minor, key_format),
        }
    }

    pub fn validate_operation(&self, operation: BTreeOperationType) -> AndromedaResult<()> {
        self.gate.validate_operation(operation)
    }

    pub const fn is_format_compatible(&self) -> bool {
        self.gate.is_format_compatible()
    }

    pub const fn format_identity(&self) -> BTreeKeyFormatIdentity {
        self.gate.format_identity()
    }

    pub const fn storage_version_parts(&self) -> (u32, u32) {
        (self.storage_version_major, self.storage_version_minor)
    }
}

#[cfg(test)]
mod tests {
    use super::KeyV1FormatGate;
    use crate::{BTreeKeyFormatIdentity, BTreeOperationType};

    #[test]
    fn gate_allows_read_operations_for_v1_format() {
        let gate = KeyV1FormatGate::new(1, 0, BTreeKeyFormatIdentity::V1_0);
        assert!(gate.validate_operation(BTreeOperationType::Lookup).is_ok());
        assert!(
            gate.validate_operation(BTreeOperationType::RangeScan)
                .is_ok()
        );
        assert!(gate.is_format_compatible());
        assert_eq!(gate.storage_version_parts(), (1, 0));
    }

    #[test]
    fn gate_rejects_mutations_until_promotion() {
        let gate = KeyV1FormatGate::new(1, 0, BTreeKeyFormatIdentity::V1_0);
        let error = gate
            .validate_operation(BTreeOperationType::Insert)
            .expect_err("mutations must stay gated");
        assert!(error.message().contains("not promoted"));
    }

    #[test]
    fn gate_rejects_unknown_codec_versions() {
        let gate = KeyV1FormatGate::new(1, 0, BTreeKeyFormatIdentity::new(1, 0, 99, 4096));
        let error = gate
            .validate_operation(BTreeOperationType::Lookup)
            .expect_err("unsupported codec version must be rejected");
        assert!(error.message().contains("codec version 99 not supported"));
    }

    #[test]
    fn validator_preserves_storage_version_parts() {
        let validator = super::KeyV1FormatValidator::new(1, 0, BTreeKeyFormatIdentity::V1_0);

        assert_eq!(validator.storage_version_parts(), (1, 0));
        assert_eq!(validator.format_identity(), BTreeKeyFormatIdentity::V1_0);
        assert!(validator.is_format_compatible());
    }
}
