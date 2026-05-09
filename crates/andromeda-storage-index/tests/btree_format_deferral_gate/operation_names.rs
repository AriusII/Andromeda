use andromeda_storage_index::BTreeOperationType;

#[test]
fn test_dec038_operation_type_names() {
    let ops = [
        (BTreeOperationType::Lookup, "lookup"),
        (BTreeOperationType::RangeScan, "range_scan"),
        (BTreeOperationType::Insert, "insert"),
        (BTreeOperationType::Delete, "delete"),
        (BTreeOperationType::Split, "split"),
        (BTreeOperationType::Merge, "merge"),
    ];

    for (op, expected_name) in ops {
        assert_eq!(
            op.name(),
            expected_name,
            "Operation {:?} should have name '{}'",
            op,
            expected_name
        );
    }
}
