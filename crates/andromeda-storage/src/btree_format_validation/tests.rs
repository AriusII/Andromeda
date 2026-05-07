use super::*;
use crate::format_version::FormatVersion;
use andromeda_core::AndromedaErrorKind;

#[test]
fn test_format_identity_v1_0_valid() {
    let fmt = BTreeKeyFormatIdentity::V1_0;
    assert_eq!(fmt.major, 1);
    assert_eq!(fmt.minor, 0);
    assert_eq!(fmt.codec_version, 1);
}

#[test]
fn test_format_identity_try_new_rejects_reserved_metadata() {
    let error = BTreeKeyFormatIdentity::try_new(0, 0, 1, 4096)
        .expect_err("reserved version must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("reserved"));

    let error = BTreeKeyFormatIdentity::try_new(1, 0, 1, 0)
        .expect_err("zero max_key_size must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("max_key_size"));

    assert_eq!(
        BTreeKeyFormatIdentity::try_new(1, 0, 1, 4096).expect("v1"),
        BTreeKeyFormatIdentity::V1_0
    );
}

#[test]
fn test_format_identity_new_does_not_panic_but_validator_rejects_invalid_parts() {
    let reserved = BTreeKeyFormatIdentity::new(0, 0, 1, 4096);
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, reserved);
    let error = validator
        .validate_operation(BTreeOperationType::Lookup)
        .expect_err("reserved version must be rejected by validator");
    assert!(error.message().contains("reserved"));

    let zero_key_limit = BTreeKeyFormatIdentity::new(1, 0, 1, 0);
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, zero_key_limit);
    let error = validator
        .validate_operation(BTreeOperationType::Lookup)
        .expect_err("zero key size must be rejected by validator");
    assert!(error.message().contains("max_key_size"));
}

#[test]
fn test_validator_rejects_incompatible_storage_version_before_operation() {
    let validator = KeyV1FormatValidator::new(FormatVersion::V2_0, BTreeKeyFormatIdentity::V1_0);
    let error = validator
        .validate_operation(BTreeOperationType::Lookup)
        .expect_err("storage/index format mismatch must be rejected");
    assert!(error.message().contains("incompatible"));
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
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

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
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    let result = validator.validate_operation(BTreeOperationType::Insert);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message().contains("not promoted"));
    assert!(err.message().contains("DEC-038"));
}

#[test]
fn test_validator_delete_deferred() {
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

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

    let r1 = v1.validate_operation(BTreeOperationType::Insert);
    let r2 = v2.validate_operation(BTreeOperationType::Insert);

    assert!(r1.is_err());
    assert!(r2.is_err());
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
