use andromeda_storage_index::{BTreeKeyFormatIdentity, KeyV1FormatValidator};

#[test]
fn test_dec038_format_identity_backward_compatible() {
    let v1_0 = BTreeKeyFormatIdentity::V1_0;
    let v1_1 = BTreeKeyFormatIdentity::new(1, 1, 1, 4096);

    assert!(
        v1_1.is_backward_compatible_with(v1_0),
        "v1.1 should be backward compatible with v1.0"
    );
    assert!(
        !v1_0.is_backward_compatible_with(v1_1),
        "v1.0 should NOT be backward compatible with v1.1"
    );
}

#[test]
fn test_dec038_validator_preserves_format_identity() {
    let fmt = BTreeKeyFormatIdentity::new(1, 0, 1, 4096);
    let validator = KeyV1FormatValidator::new(1, 0, fmt);

    let retrieved_fmt = validator.format_identity();

    assert_eq!(retrieved_fmt.major, fmt.major);
    assert_eq!(retrieved_fmt.minor, fmt.minor);
    assert_eq!(retrieved_fmt.codec_version, fmt.codec_version);
    assert_eq!(retrieved_fmt.max_key_size, fmt.max_key_size);
}

#[test]
fn test_dec038_format_display_includes_version() {
    let fmt = BTreeKeyFormatIdentity::V1_0;

    let display_str = format!("{}", fmt);

    assert!(
        display_str.contains("1.0"),
        "Display should include version: {}",
        display_str
    );
    assert!(
        display_str.contains("codec"),
        "Display should include codec info: {}",
        display_str
    );
}
