use andromeda_catalog::{ProcedureContractRef, inventory_reserve_stock_contract};
use andromeda_core::{
    AndromedaResult, CatalogVersion, ContractHash, InvocationId, ProcedureId, TransactionId,
};
use andromeda_exec::{
    AdmissionService, CompletionStatus, InvocationContext, InvocationRequest, InvocationWal,
    LocalDispatchPlan, LocalDispatcher, LocalProcedure, LocalVerticalRuntime,
    PreTransactionValidationService, ResultStreamMetadata, ResultValidationService,
};
use andromeda_observe::TraceId;
use andromeda_srpl::Cardinality;
use andromeda_storage::{InMemoryWal, Lsn, WalRecordKind};
use andromeda_tx::TransactionState;

#[derive(Debug, Default)]
struct RecordingWal {
    records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
    durable_lsn: Lsn,
}

impl InvocationWal for RecordingWal {
    fn append(
        &mut self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &[u8],
    ) -> AndromedaResult<Lsn> {
        let lsn = Lsn::new(self.records.len() as u64 + 1);
        self.records
            .push((lsn, kind, transaction_id, payload.to_vec()));
        Ok(lsn)
    }

    fn flush_through(&mut self, lsn: Lsn) -> AndromedaResult<Lsn> {
        self.durable_lsn = lsn;
        Ok(lsn)
    }
}

fn request(expected_contract_hash: ContractHash) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(11),
        procedure: ProcedureContractRef {
            procedure_id: ProcedureId::new(22),
            contract_hash: ContractHash::test_vector(7),
            catalog_version: CatalogVersion::new(3),
        },
        expected_contract_hash,
        catalog_version: CatalogVersion::new(3),
        structured_parameters: Vec::new(),
    }
}

fn procedure(contract: ProcedureContractRef) -> LocalProcedure {
    LocalProcedure {
        contract,
        required_permissions: Vec::new(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            column_count: 1,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"reserve-stock".to_vec(),
        rows_affected: 1,
    }
}

#[test]
fn contract_or_procedure_mismatch_is_rejected_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mismatched_procedure = procedure(ProcedureContractRef {
        procedure_id: ProcedureId::new(99),
        ..request(ContractHash::test_vector(7)).procedure
    });

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &mismatched_procedure,
            TraceId::new(101),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn authorization_denial_is_rejected_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mut procedure = procedure(request(ContractHash::test_vector(7)).procedure);
    procedure.required_permissions = vec!["Inventory.ReserveStock.Execute".to_string()];

    let err = runtime
        .execute_authorized(
            request(ContractHash::test_vector(7)),
            &procedure,
            &InvocationContext::new(TraceId::new(102), Vec::new()),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Security);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn result_cardinality_rules_are_enforced_by_result_service() {
    let missing_exact_count = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: None,
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
        column_count: 1,
        cardinality: Cardinality::Many,
    };
    assert!(ResultValidationService::validate_before_payload(unbounded_many).is_ok());
}

#[test]
fn local_vertical_happy_path_commits_only_with_durable_wal_evidence() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let request = InvocationRequest {
        invocation_id: InvocationId::new(700),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    let procedure = LocalProcedure {
        contract: contract.as_ref(),
        required_permissions: contract.required_permissions.clone(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            column_count: contract.result_streams[0].columns.len() as u32,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"Inventory.ReserveStock".to_vec(),
        rows_affected: 1,
    };
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_authorized(
            request,
            &procedure,
            &InvocationContext::new(TraceId::new(7000), contract.required_permissions.clone()),
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::Committed)
    );
    assert_eq!(
        outcome.completion.durable_lsn,
        Some(runtime.wal().durable_lsn())
    );
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
    assert_eq!(runtime.wal().replay_durable().len(), 3);
    assert_eq!(
        runtime.wal().records()[2].header.kind,
        WalRecordKind::TxCommit
    );
}

#[test]
fn service_and_dispatcher_api_is_usable_externally() {
    let request = request(ContractHash::test_vector(7));
    let trace = PreTransactionValidationService::validate_invocation_contract(
        &request,
        request.procedure,
        TraceId::new(202),
    )
    .unwrap();
    assert!(trace.has_explanation());

    let auth_trace = AdmissionService::authorize(
        &InvocationContext::new(
            TraceId::new(203),
            vec!["Inventory.ReserveStock.Execute".into()],
        ),
        &["Inventory.ReserveStock.Execute".into()],
    )
    .unwrap();
    assert!(auth_trace.has_explanation());

    let mut wal = RecordingWal::default();
    let receipt = LocalDispatcher::new(&mut wal)
        .dispatch_commit(LocalDispatchPlan {
            transaction_id: TransactionId::new(request.invocation_id.get()),
            mutation_payload: b"reserve-stock".to_vec(),
            rows_affected: 1,
        })
        .unwrap();

    assert_eq!(receipt.transaction_state, TransactionState::Committed);
    assert_eq!(receipt.durable_lsn, Lsn::new(3));
    assert_eq!(wal.durable_lsn, Lsn::new(3));
}
