use andromeda_storage_index::BTreeOperationType;

use crate::support::{key_v1_validator, unknown_codec_validator, unknown_major_validator};

#[test]
fn test_dec038_read_operations_allowed_keyv1() {
    let validator = key_v1_validator();

    let lookup_result = validator.validate_operation(BTreeOperationType::Lookup);
    let range_scan_result = validator.validate_operation(BTreeOperationType::RangeScan);

    assert!(
        lookup_result.is_ok(),
        "Lookup should be allowed on KeyV1 format"
    );
    assert!(
        range_scan_result.is_ok(),
        "RangeScan should be allowed on KeyV1 format"
    );
}

#[test]
fn test_dec038_unknown_major_version_rejected_failfast() {
    let result = unknown_major_validator().validate_operation(BTreeOperationType::Lookup);

    assert!(
        result.is_err(),
        "Unknown major version must be rejected before operation"
    );

    let error = result.unwrap_err();
    let message = error.message();

    assert!(
        message.contains("not supported"),
        "Error must indicate format is unsupported: {}",
        message
    );
    assert!(
        message.contains("ForensicStart"),
        "Error should mention ForensicStart recovery mode: {}",
        message
    );
}

#[test]
fn test_dec038_unknown_codec_version_rejected_failfast() {
    let result = unknown_codec_validator().validate_operation(BTreeOperationType::Lookup);

    assert!(result.is_err(), "Unknown codec must be rejected");

    let error = result.unwrap_err();
    let message = error.message();

    assert!(
        message.contains("codec version"),
        "Error must mention codec version: {}",
        message
    );
    assert!(
        message.contains("not supported"),
        "Error must indicate unsupported: {}",
        message
    );
}
