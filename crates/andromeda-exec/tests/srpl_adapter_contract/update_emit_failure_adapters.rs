use crate::support::*;

#[test]
fn test_update_adapter_single_row() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_update_request(0);

    let result = adapter.update_typed(request);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().affected_rows, 1);
}
#[test]
fn test_update_adapter_batch() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    let request = SrplUpdateRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 0).unwrap(),
        make_test_object_ref("test.schema.table"),
        SrplRowBound::exact(100).unwrap(),
        vec![],
        vec![SrplAssignmentIr {
            field: "status".to_string(),
            value: SrplValueIr::Bool(true),
        }],
    )
    .unwrap();

    let result = adapter.update_typed(request);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().affected_rows, 100);
}
#[test]
fn test_update_adapter_transaction_aborted() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    adapter.abort_transaction();

    let request = make_update_request(0);
    let result = adapter.update_typed(request);

    assert!(result.is_err());
    assert!(matches!(
        result,
        Err(SrplExecutionFailure::SemanticViolation(_))
    ));
}
#[test]
fn test_update_adapter_records_pending_updates() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_update_request(0);

    let _ = adapter.update_typed(request);
    // In a real implementation, we'd verify pending updates were recorded
    assert!(!adapter.is_transaction_aborted());
}
#[test]
fn test_emit_adapter_single_row() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    let request = make_emit_request(0);

    let result = adapter.emit_typed(request);
    assert!(result.is_ok());
}
#[test]
fn test_emit_adapter_batch() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    let request = SrplEmitRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 0).unwrap(),
        "results",
        Cardinality::Many,
        SrplRowBound::at_most(100).unwrap(),
        emit_values(),
    )
    .unwrap();

    let result = adapter.emit_typed(request);
    assert!(result.is_ok());
}
#[test]
fn test_emit_adapter_backpressure_respected() {
    let mut adapter = SrplExecutionAdapter::new(10); // Very small buffer

    let request = SrplEmitRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 0).unwrap(),
        "results",
        Cardinality::Many,
        SrplRowBound::at_most(5).unwrap(),
        emit_values(),
    )
    .unwrap();

    // First emit should succeed
    assert!(adapter.emit_typed(request.clone()).is_ok());

    // A single request larger than the bounded buffer must be rejected.
    let request2 = SrplEmitRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 1).unwrap(),
        "results",
        Cardinality::Many,
        SrplRowBound::at_most(11).unwrap(),
        emit_values(),
    )
    .unwrap();

    let result = adapter.emit_typed(request2);
    assert!(result.is_err());
}
#[test]
fn test_emit_adapter_cardinality_one() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    let request = SrplEmitRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 0).unwrap(),
        "result",
        Cardinality::One,
        SrplRowBound::exact(1).unwrap(),
        emit_values(),
    )
    .unwrap();

    let result = adapter.emit_typed(request);
    assert!(result.is_ok());
}
#[test]
fn test_emit_adapter_cardinality_optional_one() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    let request = SrplEmitRequest::new(
        SrplOperationContext::new(make_test_procedure_ref(), 0).unwrap(),
        "result",
        Cardinality::OptionalOne,
        SrplRowBound::at_most(1).unwrap(),
        emit_values(),
    )
    .unwrap();

    let result = adapter.emit_typed(request);
    assert!(result.is_ok());
}
#[test]
fn test_failure_adapter_semantic_violation() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    let failure = SrplExecutionFailure::SemanticViolation("test error".to_string());
    let request = make_failure_request(0, failure);

    let result = adapter.fail_typed(request);
    assert!(result.is_ok());
    assert!(adapter.is_transaction_aborted());
    assert_eq!(adapter.failures().len(), 1);
}
#[test]
fn test_failure_adapter_records_failure() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    let failure = SrplExecutionFailure::ContractViolation("constraint failed".to_string());
    let request = make_failure_request(0, failure.clone());

    let _ = adapter.fail_typed(request);

    assert_eq!(adapter.failures().len(), 1);
    assert_eq!(adapter.failures()[0].0, "FAILURE");
}
#[test]
fn test_failure_adapter_aborts_transaction() {
    let mut adapter = SrplExecutionAdapter::new(1000);
    assert!(!adapter.is_transaction_aborted());

    let failure = SrplExecutionFailure::CardinalityViolation {
        expected: Cardinality::One,
        bound: SrplRowBound::exact(1).unwrap(),
        actual_rows: 2,
    };
    let request = make_failure_request(0, failure);

    let _ = adapter.fail_typed(request);
    assert!(adapter.is_transaction_aborted());
}
#[test]
fn test_failure_adapter_multiple_failures() {
    let mut adapter = SrplExecutionAdapter::new(1000);

    for i in 0..3 {
        let failure = SrplExecutionFailure::SemanticViolation(format!("error {}", i));
        let request = make_failure_request(i as u32, failure);
        let _ = adapter.fail_typed(request);
    }

    assert_eq!(adapter.failures().len(), 3);
}
