use super::support::*;

#[test]
fn local_vertical_runtime_rejects_contract_before_begin() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = simple_local_procedure();

    let err = runtime
        .execute(
            request(ContractHash::test_vector(8)),
            &procedure,
            TraceId::new(99),
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn local_vertical_runtime_rejects_executable_contract_mismatch_before_begin() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let mut procedure = simple_local_procedure();
    procedure.contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(99),
        ..request(ContractHash::test_vector(7)).procedure
    };
    procedure.contract_binding = test_binding(procedure.contract);

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(99),
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().records.is_empty());
}
