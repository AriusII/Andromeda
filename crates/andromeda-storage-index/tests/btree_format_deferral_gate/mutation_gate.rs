use andromeda_storage_index::BTREE_DURABLE_FORMAT_PROMOTED;
use andromeda_storage_index::BTreeOperationType;

use crate::support::{key_v1_validator, mutation_error_message};

#[test]
fn test_dec038_durable_btree_format_is_not_promoted() {
    const {
        assert!(
            !BTREE_DURABLE_FORMAT_PROMOTED,
            "durable B-Tree page format must remain non-promoted while engine is in-memory"
        );
    }
}

#[test]
fn test_dec038_insert_mutation_rejected_with_promotion_gate_message() {
    let result = key_v1_validator().validate_operation(BTreeOperationType::Insert);

    assert!(result.is_err(), "Insert must be rejected");

    let error = result.unwrap_err();
    let message = error.message();

    assert!(
        message.contains("not promoted"),
        "Error must identify the durable B-Tree promotion gate: {}",
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

#[test]
fn test_dec038_delete_mutation_rejected_with_promotion_gate_message() {
    let result = key_v1_validator().validate_operation(BTreeOperationType::Delete);

    assert!(result.is_err(), "Delete must be rejected");

    let error = result.unwrap_err();
    let message = error.message();

    assert!(
        message.contains("not promoted"),
        "Error must identify the durable B-Tree promotion gate"
    );
    assert!(message.contains("DEC-038"), "Error must reference DEC-038");
    assert!(
        message.contains("delete"),
        "Error must mention delete operation"
    );
}

#[test]
fn test_dec038_all_mutation_types_rejected() {
    let mutations = [
        BTreeOperationType::Insert,
        BTreeOperationType::Delete,
        BTreeOperationType::Split,
        BTreeOperationType::Merge,
    ];

    for mutation in mutations {
        let result = key_v1_validator().validate_operation(mutation);
        assert!(result.is_err(), "Mutation {:?} must be rejected", mutation);

        let message = result.unwrap_err().message().to_string();
        assert!(
            message.contains("not promoted"),
            "Mutation {:?} error must identify the durable B-Tree promotion gate",
            mutation
        );
    }
}

#[test]
fn test_dec038_split_merge_operations_gated() {
    let split_msg = mutation_error_message(BTreeOperationType::Split);
    let merge_msg = mutation_error_message(BTreeOperationType::Merge);

    assert!(
        split_msg.contains("not promoted"),
        "Split error must identify the durable B-Tree promotion gate"
    );
    assert!(
        merge_msg.contains("not promoted"),
        "Merge error must identify the durable B-Tree promotion gate"
    );
}

#[test]
fn test_dec038_error_contains_escalation_guidance() {
    let message = mutation_error_message(BTreeOperationType::Insert);

    assert!(
        message.contains("Release Governance") || message.contains("escalate"),
        "Error should guide operator to escalate if needed: {}",
        message
    );
}
