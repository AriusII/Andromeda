use crate::common::*;

#[test]
fn result_cardinality_rules_are_enforced_by_result_service() {
    let missing_exact_count = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: None,
        row_count_max: None,
        column_count: 1,
        cardinality: Cardinality::One,
    };
    assert_eq!(
        ResultValidationService::validate_before_payload(missing_exact_count)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    let wrong_exact_count = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: Some(0),
        row_count_max: None,
        column_count: 1,
        cardinality: Cardinality::NonEmptyMany,
    };
    assert_eq!(
        wrong_exact_count
            .validate_before_payload()
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    let unbounded_many = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: None,
        row_count_max: None,
        column_count: 1,
        cardinality: Cardinality::Many,
    };
    assert!(ResultValidationService::validate_before_payload(unbounded_many).is_ok());

    let zero_stream = ResultStreamMetadata {
        stream_id: 0,
        row_count_exact: Some(1),
        row_count_max: Some(1),
        column_count: 1,
        cardinality: Cardinality::One,
    };
    assert_eq!(
        ResultValidationService::validate_before_payload(zero_stream)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    let row_count_mismatch = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: Some(1),
        row_count_max: Some(1),
        column_count: 1,
        cardinality: Cardinality::One,
    };
    assert_eq!(
        row_count_mismatch
            .validate_completed_stream(0)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    // Bounded Many: actual row count exceeding declared row_count_max must
    // be rejected before completion is admitted.
    let bounded_many = ResultStreamMetadata::bounded(1, 1, Cardinality::Many, 2);
    assert!(ResultValidationService::validate_before_payload(bounded_many).is_ok());
    assert_eq!(
        ResultValidationService::validate_completed_stream(bounded_many, 3)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    // Inconsistent metadata: declared row_count_max contradicts the
    // intrinsic max of `One`.
    let inconsistent_one = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: Some(1),
        row_count_max: Some(2),
        column_count: 1,
        cardinality: Cardinality::One,
    };
    assert_eq!(
        ResultValidationService::validate_before_payload(inconsistent_one)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );
}

#[test]
fn completion_mapping_rejects_commit_or_rollback_outcome_mismatches() {
    assert_eq!(
        CompletionMappingService::committed(
            InvocationId::new(8200),
            2,
            TransactionState::RolledBack,
            Lsn::new(10),
            TraceId::new(8200),
        )
        .unwrap_err()
        .kind(),
        andromeda_core::AndromedaErrorKind::Transaction
    );

    assert_eq!(
        CompletionMappingService::committed(
            InvocationId::new(8201),
            2,
            TransactionState::Committed,
            Lsn::ZERO,
            TraceId::new(8201),
        )
        .unwrap_err()
        .kind(),
        andromeda_core::AndromedaErrorKind::Storage
    );

    assert_eq!(
        CompletionMappingService::rolled_back(
            InvocationId::new(8202),
            TransactionState::Committed,
            Lsn::new(11),
            TraceId::new(8202),
        )
        .unwrap_err()
        .kind(),
        andromeda_core::AndromedaErrorKind::Transaction
    );

    let rolled_back = CompletionMappingService::rolled_back(
        InvocationId::new(8203),
        TransactionState::RolledBack,
        Lsn::new(12),
        TraceId::new(8203),
    )
    .unwrap();
    assert_eq!(rolled_back.status(), CompletionStatus::RolledBack);
    assert_eq!(rolled_back.rows_affected(), Some(0));
    assert_eq!(
        rolled_back.transaction_state(),
        Some(TransactionState::RolledBack)
    );
}
