use crate::common::*;

#[test]
fn authorization_denial_is_rejected_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mut procedure = procedure(request(ContractHash::test_vector(7)).procedure);
    procedure.required_permissions = vec!["Inventory.ReserveStock.Execute".to_string()];

    let err = runtime
        .execute_internal_authorized(
            request(ContractHash::test_vector(7)),
            &procedure,
            &InvocationContext::new(TraceId::new(102), Vec::new()),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Security);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn local_vertical_runtime_preserves_authorization_check_before_rollback_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mut procedure = procedure(request(ContractHash::test_vector(7)).procedure);
    procedure.required_permissions = vec!["Inventory.ReserveStock.Execute".to_string()];

    let err = runtime
        .rollback_authorized_business_validation_failure_after_begin(
            request(ContractHash::test_vector(7)),
            &procedure,
            &InvocationContext::new(TraceId::new(8102), Vec::new()),
            "insufficient inventory stock for reservation",
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Security);
    assert!(runtime.wal().records.is_empty());
}
