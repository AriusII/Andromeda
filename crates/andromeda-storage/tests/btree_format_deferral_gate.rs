#![forbid(unsafe_code)]

//! B-Tree Format Deferral Gate Tests — DEC-038 Validation
//!
//! This test suite validates that:
//! 1. B-Tree mutations are properly deferred to Wave 18
//! 2. KeyV1 format validation gate enforces constraints
//! 3. Read-only operations remain available in Wave 13
//! 4. Unknown formats are rejected fail-fast
//! 5. Validation results are deterministic
//! 6. Recovery can validate formats before replay
//!
//! All tests map to DEC-038 validation plan requirements.

use andromeda_storage::BTREE_DURABLE_FORMAT_PROMOTED;
use andromeda_storage::btree_format_validation::{
    BTreeKeyFormatIdentity, BTreeOperationType, KeyV1FormatValidator,
};
use andromeda_storage::format_version::FormatVersion;

// ============================================================================
// Test 0: Durable B-Tree Format Must Remain Non-Promoted
// ============================================================================

#[test]
fn test_dec038_durable_btree_format_is_not_promoted() {
    assert!(
        !BTREE_DURABLE_FORMAT_PROMOTED,
        "durable B-Tree page format must remain non-promoted while engine is in-memory"
    );
}

// ============================================================================
// Test 1: KeyV1 Read Operations Are Allowed
// ============================================================================

#[test]
fn test_dec038_read_operations_allowed_keyv1() {
    // GIVEN: KeyV1 format validator
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    // WHEN: Read-only operations are validated
    let lookup_result = validator.validate_operation(BTreeOperationType::Lookup);
    let range_scan_result = validator.validate_operation(BTreeOperationType::RangeScan);

    // THEN: Both succeed
    assert!(
        lookup_result.is_ok(),
        "Lookup should be allowed on KeyV1 format"
    );
    assert!(
        range_scan_result.is_ok(),
        "RangeScan should be allowed on KeyV1 format"
    );
}

// ============================================================================
// Test 2: Insert Mutation Rejected with Clear Deferral Message
// ============================================================================

#[test]
fn test_dec038_insert_mutation_rejected_with_wave18_message() {
    // GIVEN: KeyV1 format validator with Wave 13 storage version
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    // WHEN: Insert operation is validated
    let result = validator.validate_operation(BTreeOperationType::Insert);

    // THEN: Operation is rejected with explicit Wave 18 deferral message
    assert!(result.is_err(), "Insert must be rejected");

    let error = result.unwrap_err();
    let message = error.message();

    // Verify error contains deferral references
    assert!(
        message.contains("Wave 18"),
        "Error must reference Wave 18 deferral: {}",
        message
    );
    assert!(
        message.contains("DEC-038"),
        "Error must reference DEC-038 decision record: {}",
        message
    );
    assert!(
        message.contains("insert"),
        "Error must mention the operation: {}",
        message
    );
}

// ============================================================================
// Test 3: Delete Mutation Rejected with Clear Deferral Message
// ============================================================================

#[test]
fn test_dec038_delete_mutation_rejected_with_wave18_message() {
    // GIVEN: KeyV1 format validator
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    // WHEN: Delete operation is validated
    let result = validator.validate_operation(BTreeOperationType::Delete);

    // THEN: Operation is rejected with deferral message
    assert!(result.is_err(), "Delete must be rejected");

    let error = result.unwrap_err();
    let message = error.message();

    assert!(message.contains("Wave 18"), "Error must reference Wave 18");
    assert!(message.contains("DEC-038"), "Error must reference DEC-038");
    assert!(
        message.contains("delete"),
        "Error must mention delete operation"
    );
}

// ============================================================================
// Test 4: All Mutation Types Rejected
// ============================================================================

#[test]
fn test_dec038_all_mutation_types_rejected() {
    // GIVEN: KeyV1 format validator
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    // List of all mutation operations
    let mutations = [
        BTreeOperationType::Insert,
        BTreeOperationType::Delete,
        BTreeOperationType::Split,
        BTreeOperationType::Merge,
    ];

    // WHEN: Each mutation is validated
    for mutation in mutations.iter() {
        // THEN: All mutations are rejected
        let result = validator.validate_operation(*mutation);
        assert!(result.is_err(), "Mutation {:?} must be rejected", mutation);

        let message = result.unwrap_err().message().to_string();
        assert!(
            message.contains("Wave 18"),
            "Mutation {:?} error must reference Wave 18",
            mutation
        );
    }
}

// ============================================================================
// Test 5: Unknown Format Major Version Rejected Fail-Fast
// ============================================================================

#[test]
fn test_dec038_unknown_major_version_rejected_failfast() {
    // GIVEN: Index with unknown major version (99)
    let unknown_format = BTreeKeyFormatIdentity::new(99, 0, 1, 4096);
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, unknown_format);

    // WHEN: Any operation is attempted (even read-only)
    let result = validator.validate_operation(BTreeOperationType::Lookup);

    // THEN: Fail-fast rejection of unknown format
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

// ============================================================================
// Test 6: Unknown Codec Version Rejected Fail-Fast
// ============================================================================

#[test]
fn test_dec038_unknown_codec_version_rejected_failfast() {
    // GIVEN: Index with unknown codec version
    let unknown_format = BTreeKeyFormatIdentity::new(1, 0, 99, 4096);
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, unknown_format);

    // WHEN: Any operation is attempted
    let result = validator.validate_operation(BTreeOperationType::Lookup);

    // THEN: Fail-fast rejection of unknown codec
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

// ============================================================================
// Test 7: Format Validation Results Are Deterministic
// ============================================================================

#[test]
fn test_dec038_validation_deterministic_same_format_same_result() {
    // GIVEN: Two validators with identical format
    let fmt = BTreeKeyFormatIdentity::V1_0;
    let validator1 = KeyV1FormatValidator::new(FormatVersion::V1_0, fmt);
    let validator2 = KeyV1FormatValidator::new(FormatVersion::V1_0, fmt);

    // WHEN: Both validate the same operation multiple times
    let mutations = [BTreeOperationType::Insert, BTreeOperationType::Delete];

    for mutation in mutations.iter() {
        // Validate with first validator
        let result1_first = validator1.validate_operation(*mutation);
        let result1_second = validator1.validate_operation(*mutation);

        // Validate with second validator
        let result2_first = validator2.validate_operation(*mutation);
        let result2_second = validator2.validate_operation(*mutation);

        // THEN: Results are identical across all validations
        assert!(result1_first.is_err());
        assert!(result1_second.is_err());
        assert!(result2_first.is_err());
        assert!(result2_second.is_err());

        // Error messages must be identical (determinism)
        let msg1_first = result1_first.unwrap_err().message().to_string();
        let msg1_second = result1_second.unwrap_err().message().to_string();
        let msg2_first = result2_first.unwrap_err().message().to_string();
        let msg2_second = result2_second.unwrap_err().message().to_string();

        assert_eq!(
            msg1_first, msg1_second,
            "Same validator should produce identical messages"
        );
        assert_eq!(
            msg1_first, msg2_first,
            "Different validators with same format should produce identical messages"
        );
        assert_eq!(
            msg2_first, msg2_second,
            "Second validator should also be deterministic"
        );
    }
}

// ============================================================================
// Test 8: Format Identity Backward Compatibility Check
// ============================================================================

#[test]
fn test_dec038_format_identity_backward_compatible() {
    // GIVEN: Two format versions (1.0 and 1.1)
    let v1_0 = BTreeKeyFormatIdentity::V1_0;
    let v1_1 = BTreeKeyFormatIdentity::new(1, 1, 1, 4096);

    // WHEN: Checking backward compatibility
    // THEN: v1.1 is backward compatible with v1.0, but not vice versa
    assert!(
        v1_1.is_backward_compatible_with(v1_0),
        "v1.1 should be backward compatible with v1.0"
    );
    assert!(
        !v1_0.is_backward_compatible_with(v1_1),
        "v1.0 should NOT be backward compatible with v1.1"
    );
}

// ============================================================================
// Test 9: Validator Preserves Format Identity
// ============================================================================

#[test]
fn test_dec038_validator_preserves_format_identity() {
    // GIVEN: A validator with specific format identity
    let fmt = BTreeKeyFormatIdentity::new(1, 0, 1, 4096);
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, fmt);

    // WHEN: Format identity is retrieved
    let retrieved_fmt = validator.format_identity();

    // THEN: Retrieved format is identical to original
    assert_eq!(retrieved_fmt.major, fmt.major);
    assert_eq!(retrieved_fmt.minor, fmt.minor);
    assert_eq!(retrieved_fmt.codec_version, fmt.codec_version);
    assert_eq!(retrieved_fmt.max_key_size, fmt.max_key_size);
}

// ============================================================================
// Test 10: Split and Merge Operations Deferred
// ============================================================================

#[test]
fn test_dec038_split_merge_operations_deferred() {
    // GIVEN: KeyV1 format validator
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    // WHEN: Split and merge operations are validated
    let split_result = validator.validate_operation(BTreeOperationType::Split);
    let merge_result = validator.validate_operation(BTreeOperationType::Merge);

    // THEN: Both are rejected with Wave 18 deferral message
    assert!(split_result.is_err(), "Split must be deferred");
    assert!(merge_result.is_err(), "Merge must be deferred");

    let split_msg = split_result.unwrap_err().message().to_string();
    let merge_msg = merge_result.unwrap_err().message().to_string();

    assert!(
        split_msg.contains("Wave 18"),
        "Split error must reference Wave 18"
    );
    assert!(
        merge_msg.contains("Wave 18"),
        "Merge error must reference Wave 18"
    );
}

// ============================================================================
// Test 11: Format Compatibility Check
// ============================================================================

#[test]
fn test_dec038_format_compatibility_check() {
    // GIVEN: Storage version V1.0 and KeyV1 format
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    // WHEN: Format compatibility is checked
    let is_compatible = validator.is_format_compatible();

    // THEN: Format is compatible
    assert!(
        is_compatible,
        "KeyV1 format should be compatible with V1.0 storage"
    );
}

// ============================================================================
// Test 12: Incompatible Storage Version Rejected
// ============================================================================

#[test]
fn test_dec038_incompatible_storage_version_rejected() {
    // GIVEN: Storage version V2.0 (future) with KeyV1 format
    let validator = KeyV1FormatValidator::new(FormatVersion::V2_0, BTreeKeyFormatIdentity::V1_0);

    // WHEN: Format compatibility is checked
    let is_compatible = validator.is_format_compatible();

    // THEN: Format is not compatible (major version mismatch)
    assert!(
        !is_compatible,
        "KeyV1 should not be compatible with V2.0 storage"
    );
}

// ============================================================================
// Test 13: Error Message Contains Governance Escalation Note
// ============================================================================

#[test]
fn test_dec038_error_contains_escalation_guidance() {
    // GIVEN: KeyV1 format validator
    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    // WHEN: Mutation is attempted
    let result = validator.validate_operation(BTreeOperationType::Insert);

    // THEN: Error message includes guidance to escalate if operator believes it's wrong
    assert!(result.is_err());
    let message = result.unwrap_err().message().to_string();

    assert!(
        message.contains("Release Governance") || message.contains("escalate"),
        "Error should guide operator to escalate if needed: {}",
        message
    );
}

// ============================================================================
// Test 14: Operation Type Names Are Correct
// ============================================================================

#[test]
fn test_dec038_operation_type_names() {
    // GIVEN: All operation types
    let ops = [
        (BTreeOperationType::Lookup, "lookup"),
        (BTreeOperationType::RangeScan, "range_scan"),
        (BTreeOperationType::Insert, "insert"),
        (BTreeOperationType::Delete, "delete"),
        (BTreeOperationType::Split, "split"),
        (BTreeOperationType::Merge, "merge"),
    ];

    // WHEN: Operation names are retrieved
    for (op, expected_name) in ops.iter() {
        // THEN: Names are correct
        assert_eq!(
            op.name(),
            *expected_name,
            "Operation {:?} should have name '{}'",
            op,
            expected_name
        );
    }
}

// ============================================================================
// Test 15: Format Display Includes Version Information
// ============================================================================

#[test]
fn test_dec038_format_display_includes_version() {
    // GIVEN: KeyV1 format
    let fmt = BTreeKeyFormatIdentity::V1_0;

    // WHEN: Format is displayed
    let display_str = format!("{}", fmt);

    // THEN: Display includes version information
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

// ============================================================================
// Summary Test: All Doctrine Checks Pass
// ============================================================================

#[test]
fn test_dec038_all_doctrine_checks_pass() {
    // This test verifies that the validation gate implementation complies
    // with Andromeda doctrine as stated in DEC-038.

    let validator = KeyV1FormatValidator::new(FormatVersion::V1_0, BTreeKeyFormatIdentity::V1_0);

    // ✅ No-go rule: Unrecoverable state changes
    // Validation gate prevents mutations, so state cannot become unrecoverable
    assert!(
        validator
            .validate_operation(BTreeOperationType::Insert)
            .is_err()
    );

    // ✅ Fail-fast on unknown formats
    let unknown = BTreeKeyFormatIdentity::new(99, 0, 1, 4096);
    let unknown_validator = KeyV1FormatValidator::new(FormatVersion::V1_0, unknown);
    assert!(
        unknown_validator
            .validate_operation(BTreeOperationType::Lookup)
            .is_err()
    );

    // ✅ Deterministic validation results
    let result1 = validator.validate_operation(BTreeOperationType::Insert);
    let result2 = validator.validate_operation(BTreeOperationType::Insert);
    assert_eq!(
        result1.unwrap_err().message(),
        result2.unwrap_err().message()
    );

    // ✅ Format compatibility checkable
    assert!(validator.is_format_compatible());

    // All doctrine checks pass
}
