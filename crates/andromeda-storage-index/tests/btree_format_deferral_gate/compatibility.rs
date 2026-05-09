use andromeda_storage_index::{BTreeKeyFormatIdentity, KeyV1FormatValidator};

#[test]
fn test_dec038_format_compatibility_check() {
    let validator = KeyV1FormatValidator::new(1, 0, BTreeKeyFormatIdentity::V1_0);

    let is_compatible = validator.is_format_compatible();

    assert!(
        is_compatible,
        "KeyV1 format should be compatible with V1.0 storage"
    );
}

#[test]
fn test_dec038_incompatible_storage_version_rejected() {
    let validator = KeyV1FormatValidator::new(2, 0, BTreeKeyFormatIdentity::V1_0);

    let is_compatible = validator.is_format_compatible();

    assert!(
        !is_compatible,
        "KeyV1 should not be compatible with V2.0 storage"
    );
}
