use crate::support::*;

#[test]
fn test_adapter_read_and_assert_workflow() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    adapter.add_input("expected_id", FieldValue::Integer(5));
    let row = StructuredObject::new().with_field("actual_id", FieldValue::Integer(5));
    adapter.environment_mut().bind_read("data", vec![row]);

    let request = make_assert_request(0);
    let result = adapter.assert_typed(request);

    assert!(result.is_ok());
    assert!(result.unwrap().passed);
}
#[test]
fn test_adapter_update_and_emit_workflow() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    // Step 1: Update
    let update_request = make_update_request(0);
    let update_result = adapter.update_typed(update_request);
    assert!(update_result.is_ok());

    // Step 2: Emit
    let emit_request = make_emit_request(1);
    let emit_result = adapter.emit_typed(emit_request);
    assert!(emit_result.is_ok());

    assert!(!adapter.is_transaction_aborted());
}
#[test]
fn test_adapter_transaction_rollback_on_failure() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    // Step 1: Update
    let update_request = make_update_request(0);
    adapter.update_typed(update_request).ok();
    assert!(!adapter.is_transaction_aborted());

    // Step 2: Failure
    let failure = SrplExecutionFailure::SemanticViolation("constraint violated".to_string());
    let failure_request = make_failure_request(1, failure);
    adapter.fail_typed(failure_request).ok();

    // Transaction should be aborted
    assert!(adapter.is_transaction_aborted());

    // Further updates should fail
    let another_update = make_update_request(2);
    let result = adapter.update_typed(another_update);
    assert!(result.is_err());
}
#[test]
fn test_adapter_cardinality_preserved_through_operations() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    let read_request = make_read_request(0, SrplRowBound::exact(1).unwrap(), Cardinality::One);
    let read_result = adapter.read_typed(read_request);
    assert!(matches!(
        read_result,
        Err(SrplExecutionFailure::CardinalityViolation { .. })
    ));

    adapter.add_input("expected_id", FieldValue::Integer(1));
    let row = StructuredObject::new().with_field("actual_id", FieldValue::Integer(1));
    adapter.environment_mut().bind_read("data", vec![row]);

    let assert_request = make_assert_request(1);
    let assert_result = adapter.assert_typed(assert_request);
    assert!(assert_result.is_ok());

    let emit_request = SrplEmitRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 2).unwrap(),
        "result",
        Cardinality::One,
        SrplRowBound::exact(1).unwrap(),
        emit_values(),
    )
    .unwrap();

    let emit_result = adapter.emit_typed(emit_request);
    assert!(emit_result.is_ok());
}
#[test]
fn test_adapter_error_determinism() {
    let mut adapter1 = SrplExecutionAdapter::new(1000);
    let mut adapter2 = SrplExecutionAdapter::new(1000);

    let request = make_assert_request(0);

    let result1 = adapter1.assert_typed(request.clone());
    let result2 = adapter2.assert_typed(request);

    // Both should fail with same error (missing binding)
    assert!(result1.is_err());
    assert!(result2.is_err());
}
#[test]
fn test_adapter_multiple_independent_operations() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    for i in 0..5 {
        let request = make_read_request(i, SrplRowBound::at_most(10).unwrap(), Cardinality::Many);
        let result = adapter.read_typed(request);
        assert!(result.is_ok());
    }
}
#[test]
fn test_adapter_recovery_failure_tracking() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    let failures = vec![
        SrplExecutionFailure::SemanticViolation("error1".to_string()),
        SrplExecutionFailure::ContractViolation("error2".to_string()),
        SrplExecutionFailure::ResourceLimitExceeded("error3".to_string()),
    ];

    for (i, failure) in failures.into_iter().enumerate() {
        let request = make_failure_request(i as u32, failure);
        let _ = adapter.fail_typed(request);
    }

    assert_eq!(adapter.failures().len(), 3);
}
#[test]
fn test_adapter_bounded_operations_count() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    // Each operation should be independently bounded
    for i in 0..16 {
        let request = make_read_request(
            i as u32,
            SrplRowBound::at_most(1).unwrap(),
            Cardinality::Many,
        );
        assert!(adapter.read_typed(request).is_ok());
    }
}
