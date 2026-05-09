use andromeda_error::AndromedaErrorKind;
use andromeda_result_stream::{ResultStreamMetadata, ResultValidationService};
use andromeda_srpl_cardinality::Cardinality;

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
        AndromedaErrorKind::Contract
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
        AndromedaErrorKind::Contract
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
        AndromedaErrorKind::Contract
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
        AndromedaErrorKind::Contract
    );

    let bounded_many = ResultStreamMetadata::bounded(1, 1, Cardinality::Many, 2);
    assert!(ResultValidationService::validate_before_payload(bounded_many).is_ok());
    assert_eq!(
        ResultValidationService::validate_completed_stream(bounded_many, 3)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

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
        AndromedaErrorKind::Contract
    );
}
