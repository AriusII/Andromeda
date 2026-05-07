use crate::support::*;

#[test]
fn test_read_adapter_single_row() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_read_request(0, SrplRowBound::exact(1).unwrap(), Cardinality::One);

    let result = adapter.read_typed(request);
    assert!(matches!(
        result,
        Err(SrplExecutionFailure::CardinalityViolation { .. })
    ));
}
#[test]
fn test_read_adapter_multiple_rows() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_read_request(0, SrplRowBound::at_most(100).unwrap(), Cardinality::Many);

    let result = adapter.read_typed(request);
    assert!(result.is_ok());
}
#[test]
fn test_read_adapter_empty_result_set() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_read_request(0, SrplRowBound::at_most(10).unwrap(), Cardinality::Many);

    let result = adapter.read_typed(request);
    assert!(result.is_ok());
    if let Ok(read_result) = result {
        assert_eq!(read_result.rows.len(), 0);
    }
}
#[test]
fn test_read_adapter_cardinality_one() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_read_request(0, SrplRowBound::exact(1).unwrap(), Cardinality::One);

    let result = adapter.read_typed(request);
    assert!(matches!(
        result,
        Err(SrplExecutionFailure::CardinalityViolation { .. })
    ));
}
#[test]
fn test_read_adapter_cardinality_optional_one() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_read_request(
        0,
        SrplRowBound::at_most(1).unwrap(),
        Cardinality::OptionalOne,
    );

    let result = adapter.read_typed(request);
    assert!(result.is_ok());
}
#[test]
fn test_read_adapter_cardinality_nonempty_many() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_read_request(
        0,
        SrplRowBound::at_most(1000).unwrap(),
        Cardinality::NonEmptyMany,
    );

    let result = adapter.read_typed(request);
    assert!(matches!(
        result,
        Err(SrplExecutionFailure::CardinalityViolation { .. })
    ));
}
#[test]
fn test_read_adapter_with_predicates() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.add_input("user_id", FieldValue::Integer(42));

    let predicates = vec![SrplPredicateIr::InputEqualsField {
        input: "user_id".to_string(),
        binding: "users".to_string(),
        field: "id".to_string(),
    }];

    let request = SrplReadRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 0).unwrap(),
        make_test_object_ref("test.schema.users"),
        Cardinality::Many,
        SrplRowBound::at_most(1).unwrap(),
        predicates,
    )
    .unwrap();

    let result = adapter.read_typed(request);
    assert!(result.is_ok());
}
