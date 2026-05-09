use andromeda_storage_index::{BTreeKeyFormatIdentity, BTreeOperationType, KeyV1FormatValidator};

#[test]
fn test_dec038_validation_deterministic_same_format_same_result() {
    let fmt = BTreeKeyFormatIdentity::V1_0;
    let validator1 = KeyV1FormatValidator::new(1, 0, fmt);
    let validator2 = KeyV1FormatValidator::new(1, 0, fmt);

    let mutations = [BTreeOperationType::Insert, BTreeOperationType::Delete];

    for mutation in mutations {
        let result1_first = validator1.validate_operation(mutation);
        let result1_second = validator1.validate_operation(mutation);
        let result2_first = validator2.validate_operation(mutation);
        let result2_second = validator2.validate_operation(mutation);

        assert!(result1_first.is_err());
        assert!(result1_second.is_err());
        assert!(result2_first.is_err());
        assert!(result2_second.is_err());

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
