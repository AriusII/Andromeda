//! Storage compatibility checks for index-owned B-Tree durable promotion gates.

#![forbid(unsafe_code)]

use andromeda_error::AndromedaErrorKind;
use andromeda_storage::format_version::FormatVersion;
use andromeda_storage::{BTreeKeyFormatIdentity, BTreeOperationType, KeyV1FormatValidator};

#[test]
fn storage_key_v1_validator_wrapper_preserves_legacy_constructor_shape() {
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    assert_eq!(validator.storage_version(), FormatVersion::V1_0);
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
