//! Storage compatibility checks for index-owned B-Tree durable promotion gates.

#![forbid(unsafe_code)]

use andromeda_error::AndromedaErrorKind;
use andromeda_manifest::format_version::FormatVersion;
use andromeda_storage_index::{BTreeKeyFormatIdentity, BTreeOperationType, KeyV1FormatValidator};

#[test]
fn index_key_v1_validator_preserves_durable_mutation_gate() {
    let storage_version = FormatVersion::V1_0;
    let validator = KeyV1FormatValidator::new(
        storage_version.major,
        storage_version.minor,
        BTreeKeyFormatIdentity::V1_0,
    );

    assert_eq!(
        validator.storage_version_parts(),
        (storage_version.major, storage_version.minor)
    );
    assert_eq!(validator.format_identity(), BTreeKeyFormatIdentity::V1_0);
    assert!(validator.is_format_compatible());
    assert_eq!(
        validator
            .validate_operation(BTreeOperationType::Insert)
            .expect_err("storage wrapper must delegate mutation gate to storage-index")
            .kind(),
        AndromedaErrorKind::Storage
    );
}
