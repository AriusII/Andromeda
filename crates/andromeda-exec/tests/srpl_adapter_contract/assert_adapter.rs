use crate::support::*;

#[test]
fn test_assert_adapter_true_predicate() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.add_input("expected_id", FieldValue::Integer(42));

    let row = StructuredObject::new().with_field("actual_id", FieldValue::Integer(42));
    adapter.environment_mut().bind_read("data", vec![row]);

    let request = make_assert_request(0);
    let result = adapter.assert_typed(request);

    assert!(result.is_ok());
    let assert_result = result.unwrap();
    assert!(assert_result.passed);
}
#[test]
fn test_assert_adapter_false_predicate() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.add_input("expected_id", FieldValue::Integer(42));

    let row = StructuredObject::new().with_field("actual_id", FieldValue::Integer(99));
    adapter.environment_mut().bind_read("data", vec![row]);

    let request = make_assert_request(0);
    let result = adapter.assert_typed(request);

    assert!(result.is_ok());
    let assert_result = result.unwrap();
    assert!(!assert_result.passed);
}
#[test]
fn test_assert_adapter_comparison() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.add_input("threshold", FieldValue::Integer(50));

    let row = StructuredObject::new().with_field("value", FieldValue::Integer(75));
    adapter.environment_mut().bind_read("items", vec![row]);

    let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: "items".to_string(),
        field: "value".to_string(),
        input: "threshold".to_string(),
    };

    let request = SrplAssertRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 0).unwrap(),
        predicate,
        "ASSERTION_FAILED",
    )
    .unwrap();

    let result = adapter.assert_typed(request);
    assert!(result.is_ok());
    assert!(result.unwrap().passed);
}
#[test]
fn test_assert_adapter_comparison_false() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.add_input("threshold", FieldValue::Integer(100));

    let row = StructuredObject::new().with_field("value", FieldValue::Integer(50));
    adapter.environment_mut().bind_read("items", vec![row]);

    let predicate = SrplPredicateIr::FieldGreaterThanOrEqualInput {
        binding: "items".to_string(),
        field: "value".to_string(),
        input: "threshold".to_string(),
    };

    let request = SrplAssertRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 0).unwrap(),
        predicate,
        "ASSERTION_FAILED",
    )
    .unwrap();

    let result = adapter.assert_typed(request);
    assert!(result.is_ok());
    assert!(!result.unwrap().passed);
}
#[test]
fn test_assert_adapter_missing_input() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_assert_request(0);

    let result = adapter.assert_typed(request);
    assert!(result.is_err());
}
#[test]
fn test_assert_adapter_missing_binding() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.add_input("expected_id", FieldValue::Integer(42));

    let request = make_assert_request(0);
    let result = adapter.assert_typed(request);

    assert!(result.is_err());
}
