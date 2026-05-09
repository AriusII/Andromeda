use andromeda_storage_index::BTreeOperationType;

use crate::support::{key_v1_validator, unknown_major_validator};

#[test]
fn test_dec038_all_doctrine_checks_pass() {
    let validator = key_v1_validator();

    assert!(
        validator
            .validate_operation(BTreeOperationType::Insert)
            .is_err(),
        "validation gate must prevent unrecoverable durable B-Tree mutations"
    );

    assert!(
        unknown_major_validator()
            .validate_operation(BTreeOperationType::Lookup)
            .is_err(),
        "unknown formats must fail fast before operation dispatch"
    );

    let result1 = validator.validate_operation(BTreeOperationType::Insert);
    let result2 = validator.validate_operation(BTreeOperationType::Insert);
    assert_eq!(
        result1.unwrap_err().message(),
        result2.unwrap_err().message()
    );

    assert!(validator.is_format_compatible());
    assert_eq!(validator.storage_version_parts(), (1, 0));
}
