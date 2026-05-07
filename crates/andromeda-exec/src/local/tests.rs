use super::runtime::LocalVerticalRuntime;
use super::types::LocalProcedure;
use andromeda_catalog::{
    CatalogLifecycleTarget, CatalogSnapshot, CatalogSystemStore, DefinitionBatch,
    DefinitionBatchId, DefinitionOperation, InvocationRuntimeRecordOutcome, PolicyVersion,
    ProcedureContract, ProcedureContractBinding, ProcedureContractRef, ProcedureRuntimePlanId,
    ProcedureRuntimeStatus, ProcedureStore, ProcedureStoreEntry, StatsVersion,
    inventory_domain_definition_batch, inventory_reserve_stock_contract,
};
use andromeda_core::{
    AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, DatabaseId, InvocationId,
    NamespaceId, ProcedureId, TransactionId,
};
use andromeda_observe::{
    AuthorizationDenialReason, AuthorizationOutcome, CertificateIdentity, Permission,
    PrincipalBinding, PrincipalRegistry, SecurityAuditOutcome, SurfaceScope, TraceId,
    UserPrincipal, UserPrincipalKind,
};
use andromeda_quic::SurfacePlane;
use andromeda_srpl::Cardinality;
use andromeda_storage::{InMemoryWal, Lsn, WalRecordKind};
use andromeda_tx::TransactionState;

use crate::{
    CompletionStatus, EXEC_TX_ROLLBACK_PAYLOAD_LEN, InvocationContext, InvocationRequest,
    InvocationWal, LocalDispatcher, LocalRollbackPlan, ResultStreamMetadata, RollbackCause,
    SurfacePlaneAuthorizer,
};

#[derive(Debug, Default)]
struct TestWal {
    records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
    durable_lsn: Lsn,
}

impl InvocationWal for TestWal {
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
    let procedure = ProcedureContractRef {
        procedure_id: ProcedureId::new(2),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(3),
    };
    InvocationRequest {
        invocation_id: InvocationId::new(1),
        procedure,
        expected_binding: Some(test_binding(procedure)),
        expected_contract_hash,
        catalog_version: CatalogVersion::new(3),
        structured_parameters: Vec::new(),
    }
}

fn inventory_request(contract: &ProcedureContract) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(700),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    }
}

fn inventory_local_procedure(contract: &ProcedureContract) -> LocalProcedure {
    LocalProcedure {
        contract: contract.as_ref(),
        contract_binding: contract.binding(),
        required_permissions: contract.required_permissions.clone(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            row_count_max: Some(1),
            column_count: contract.result_streams[0].columns.len() as u32,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"Inventory.ReserveStock".to_vec(),
        rows_affected: 1,
    }
}

fn test_binding(procedure: ProcedureContractRef) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: procedure.procedure_id,
        catalog_version: procedure.catalog_version,
        contract_hash: procedure.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([7; PolicyVersion::LEN]),
    }
}

fn inventory_catalog_store() -> CatalogSystemStore {
    let mut store = CatalogSystemStore::empty(
        DatabaseId::new(0x1000),
        NamespaceId::new(0x1001),
        CatalogVersion::new(0),
    );
    store
        .apply_definition_batch(&inventory_domain_definition_batch().unwrap())
        .unwrap();
    store
}

fn principal_registry(bindings: Vec<PrincipalBinding>) -> PrincipalRegistry {
    let mut registry = PrincipalRegistry::new();
    for binding in bindings {
        registry.register(binding).unwrap();
    }
    registry
}

fn principal_binding(
    fingerprint: &str,
    surface: SurfaceScope,
    principal_id: &str,
    permissions: Vec<Permission>,
) -> PrincipalBinding {
    PrincipalBinding::new(
        CertificateIdentity::new(fingerprint, format!("CN={fingerprint}"), surface).unwrap(),
        UserPrincipal::new(principal_id, UserPrincipalKind::Service).unwrap(),
        permissions,
    )
    .unwrap()
}

#[test]
fn local_vertical_runtime_commits_only_after_durable_wal() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = LocalProcedure {
        contract: request(ContractHash::test_vector(7)).procedure,
        contract_binding: request(ContractHash::test_vector(7))
            .expected_binding
            .unwrap(),
        required_permissions: Vec::new(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            row_count_max: Some(1),
            column_count: 1,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"reserve-stock".to_vec(),
        rows_affected: 1,
    };

    let outcome = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(99),
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::Committed)
    );
    assert_eq!(outcome.completion.durable_lsn, Some(Lsn::new(3)));
    assert_eq!(runtime.wal().records.len(), 3);
    assert!(outcome.contract_trace.has_explanation());
}

#[test]
fn local_vertical_runtime_rejects_contract_before_begin() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = LocalProcedure {
        contract: request(ContractHash::test_vector(7)).procedure,
        contract_binding: request(ContractHash::test_vector(7))
            .expected_binding
            .unwrap(),
        required_permissions: Vec::new(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            row_count_max: Some(1),
            column_count: 1,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"reserve-stock".to_vec(),
        rows_affected: 1,
    };

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
    let procedure = LocalProcedure {
        contract: ProcedureContractRef {
            procedure_id: ProcedureId::new(99),
            ..request(ContractHash::test_vector(7)).procedure
        },
        contract_binding: test_binding(ProcedureContractRef {
            procedure_id: ProcedureId::new(99),
            ..request(ContractHash::test_vector(7)).procedure
        }),
        required_permissions: Vec::new(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            row_count_max: Some(1),
            column_count: 1,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"reserve-stock".to_vec(),
        rows_affected: 1,
    };

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

#[test]
fn local_vertical_runtime_uses_inventory_contract_and_in_memory_wal() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let request = InvocationRequest {
        invocation_id: InvocationId::new(700),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    let procedure = LocalProcedure {
        contract: contract.as_ref(),
        contract_binding: contract.binding(),
        required_permissions: contract.required_permissions.clone(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            row_count_max: Some(1),
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
    assert!(outcome.authorization_trace.is_some());
    assert_eq!(outcome.completion.durable_lsn, Some(Lsn::new(3)));
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
    assert_eq!(runtime.wal().replay_durable().len(), 3);

    let runtime_record = outcome.procedure_runtime_record();
    assert_eq!(runtime_record.invocation_id, InvocationId::new(700));
    assert_eq!(runtime_record.binding, contract.binding());
    assert_eq!(runtime_record.status, ProcedureRuntimeStatus::Committed);
    assert_eq!(runtime_record.error_kind, None);
    assert!(runtime_record.completed_at >= runtime_record.started_at);
    assert_eq!(
        runtime_record.duration_millis,
        runtime_record.completed_at.as_unix_millis() - runtime_record.started_at.as_unix_millis()
    );
    assert_eq!(runtime_record.counters.rows_read, 1);
    assert_eq!(runtime_record.counters.rows_written, 1);
    assert!(runtime_record.counters.wal_bytes > 0);
    assert_eq!(runtime_record.counters.temp_bytes, 0);
    let plan_key = runtime_record
        .plan_key
        .expect("local runtime emits a singleton plan key");
    assert_eq!(plan_key.catalog_version, contract.object.catalog_version);
    assert_eq!(plan_key.contract_hash, contract.contract_hash);
    assert_eq!(
        runtime_record.plan_id,
        Some(ProcedureRuntimePlanId::from_plan_cache_key(&plan_key))
    );

    let mut procedure_store = ProcedureStore::new();
    procedure_store
        .register(ProcedureStoreEntry::from_contract(&contract).unwrap())
        .unwrap();
    assert_eq!(
        outcome
            .attach_runtime_to_procedure_store(&mut procedure_store)
            .unwrap(),
        InvocationRuntimeRecordOutcome::Stored
    );
    assert_eq!(
        outcome
            .attach_runtime_to_procedure_store(&mut procedure_store)
            .unwrap(),
        InvocationRuntimeRecordOutcome::Duplicate
    );
    assert_eq!(procedure_store.total_recorded_runtime_invocations(), 1);
}

#[test]
fn local_vertical_runtime_rejects_missing_permission_before_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let request = InvocationRequest {
        invocation_id: InvocationId::new(701),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    let procedure = LocalProcedure {
        contract: contract.as_ref(),
        contract_binding: contract.binding(),
        required_permissions: contract.required_permissions.clone(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            row_count_max: Some(1),
            column_count: contract.result_streams[0].columns.len() as u32,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"Inventory.ReserveStock".to_vec(),
        rows_affected: 1,
    };
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let err = runtime
        .execute_authorized(
            request,
            &procedure,
            &InvocationContext::new(TraceId::new(7001), Vec::new()),
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(runtime.wal().is_empty());
}

#[test]
fn local_vertical_runtime_rejects_permissioned_execute_without_context_before_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let err = runtime
        .execute(inventory_request(&contract), &procedure, TraceId::new(7005))
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(err.message().contains("authorization context"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn local_vertical_runtime_rejects_permissioned_rollback_without_context_before_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let err = runtime
        .rollback_business_validation_failure_after_begin(
            inventory_request(&contract),
            &procedure,
            TraceId::new(7006),
            "insufficient inventory stock for reservation",
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(err.message().contains("authorization context"));
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn surface_dispatch_denial_stops_before_transaction_creation() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let context = InvocationContext::new(TraceId::new(7002), contract.required_permissions.clone());
    let registry = principal_registry(Vec::new());
    let gate = SurfacePlaneAuthorizer::new(&registry);
    let runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let denied = gate
        .authorize_procedure_dispatch(
            context.trace_id,
            SurfacePlane::Application,
            "fp-not-registered",
        )
        .unwrap()
        .unwrap_err();

    assert!(denied.is_denied());
    if let AuthorizationOutcome::Denied { reason, audit } = denied {
        assert_eq!(reason, AuthorizationDenialReason::UnknownCertificate);
        assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
        assert!(audit.has_identity_evidence());
    }
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
}

#[test]
fn surface_authorized_dispatch_token_executes_through_local_runtime() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = InvocationContext::new(TraceId::new(7003), contract.required_permissions.clone());
    let registry = principal_registry(vec![principal_binding(
        "fp-app-dispatch",
        SurfaceScope::Application,
        "svc-dispatch",
        vec![Permission::ExecuteProcedure],
    )]);
    let gate = SurfacePlaneAuthorizer::new(&registry);
    let token = gate
        .authorize_procedure_dispatch(
            context.trace_id,
            SurfacePlane::Application,
            "fp-app-dispatch",
        )
        .unwrap()
        .unwrap();
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_surface_authorized(inventory_request(&contract), &procedure, &context, &token)
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(token.audit().outcome, SecurityAuditOutcome::Allowed);
    assert_eq!(runtime.wal().replay_durable().len(), 3);
}

#[test]
fn administration_surface_cannot_present_procedure_dispatch_as_external_runtime_path() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let registry = principal_registry(vec![principal_binding(
        "fp-admin-with-exec",
        SurfaceScope::Administration,
        "ops-dispatch",
        vec![Permission::ExecuteProcedure],
    )]);
    let gate = SurfacePlaneAuthorizer::new(&registry);
    let runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let denied = gate
        .authorize_procedure_dispatch(
            TraceId::new(7004),
            SurfacePlane::Administration,
            "fp-admin-with-exec",
        )
        .unwrap()
        .unwrap_err();

    assert!(denied.is_denied());
    if let AuthorizationOutcome::Denied { reason, audit } = denied {
        assert_eq!(
            reason,
            AuthorizationDenialReason::SurfaceDoesNotPermitPermission
        );
        assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
        assert!(audit.reason.contains("requires_application_surface"));
    }
    assert!(runtime.wal().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);
    assert!(contract.required_permissions.iter().all(|p| !p.is_empty()));
}

#[test]
fn catalog_backed_runtime_resolves_visible_procedure_before_begin() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let context = InvocationContext::new(TraceId::new(7100), contract.required_permissions.clone());
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_authorized_catalog_resolved(
            inventory_request(&contract),
            &procedure,
            store.snapshot(),
            &context,
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(runtime.wal().replay_durable().len(), 3);
}

#[test]
fn catalog_backed_runtime_rejects_absent_procedure_before_begin() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let catalog = CatalogSnapshot::empty(
        DatabaseId::new(0x1000),
        NamespaceId::new(0x1001),
        CatalogVersion::new(1),
    );
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_catalog_resolved(
            inventory_request(&contract),
            &procedure,
            &catalog,
            TraceId::new(7101),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("absent"));
    assert!(runtime.wal().is_empty());
}

#[test]
fn catalog_backed_runtime_rejects_inactive_procedure_before_begin() {
    let mut store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    store
        .apply_definition_batch(&DefinitionBatch {
            batch_id: DefinitionBatchId::new(0x7102),
            database_id: DatabaseId::new(0x1000),
            namespace_id: NamespaceId::new(0x1001),
            base_version: CatalogVersion::new(1),
            operations: vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: contract.object.clone(),
            })],
        })
        .unwrap();
    let mut request = inventory_request(&contract);
    request.catalog_version = store.snapshot().version;
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_catalog_resolved(request, &procedure, store.snapshot(), TraceId::new(7102))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("not active"));
    assert!(runtime.wal().is_empty());
}

#[test]
fn catalog_backed_runtime_rejects_stale_catalog_version_before_begin() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut request = inventory_request(&contract);
    request.catalog_version = CatalogVersion::new(2);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_catalog_resolved(request, &procedure, store.snapshot(), TraceId::new(7103))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("CatalogVersion mismatch"));
    assert!(runtime.wal().is_empty());
}

#[test]
fn catalog_backed_runtime_rejects_contract_hash_mismatch_before_begin() {
    let store = inventory_catalog_store();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = inventory_local_procedure(&contract);
    let mut request = inventory_request(&contract);
    request.expected_contract_hash = ContractHash::test_vector(0x99);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let error = runtime
        .execute_catalog_resolved(request, &procedure, store.snapshot(), TraceId::new(7104))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("ContractHash mismatch"));
    assert!(runtime.wal().is_empty());
}

// --- failure routing through Failed/Poisoned before durable rollback ---

fn failure_request() -> InvocationRequest {
    request(ContractHash::test_vector(7))
}

fn failure_procedure() -> LocalProcedure {
    LocalProcedure {
        contract: failure_request().procedure,
        contract_binding: failure_request().expected_binding.unwrap(),
        required_permissions: Vec::new(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            row_count_max: Some(1),
            column_count: 1,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"reserve-stock".to_vec(),
        rows_affected: 1,
    }
}

#[test]
fn business_failure_routes_through_failed_before_durable_rollback() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = failure_procedure();

    let outcome = runtime
        .rollback_business_validation_failure_after_begin(
            failure_request(),
            &procedure,
            TraceId::new(9100),
            "insufficient inventory stock for reservation",
        )
        .unwrap();

    // No success-shaped completion — RolledBack only after durable evidence.
    assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected, Some(0));
    assert!(outcome.completion.durable_lsn.is_some());

    // WAL evidence: TxBegin then TxRollback (no TxCommit), and durable
    // flush must reach the rollback LSN.
    let wal = runtime.wal();
    assert_eq!(wal.records.len(), 2);
    assert_eq!(wal.records[0].1, WalRecordKind::TxBegin);
    assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
    assert!(
        !wal.records
            .iter()
            .any(|(_, kind, _, _)| *kind == WalRecordKind::TxCommit)
    );
    assert!(wal.durable_lsn >= wal.records[1].0);
    assert_eq!(wal.records[1].3.len(), EXEC_TX_ROLLBACK_PAYLOAD_LEN);

    let runtime_record = outcome.procedure_runtime_record();
    assert_eq!(runtime_record.binding, procedure.contract_binding);
    assert_eq!(runtime_record.status, ProcedureRuntimeStatus::RolledBack);
    assert_eq!(
        runtime_record.error_kind,
        Some(AndromedaErrorKind::Execution)
    );
    assert_eq!(runtime_record.counters.rows_written, 0);
    assert!(runtime_record.counters.rows_read > 0);
    assert!(runtime_record.counters.wal_bytes > 0);
    assert!(runtime_record.plan_key.is_some());
    assert!(runtime_record.plan_id.is_some());
}

#[test]
fn poison_failure_routes_through_poisoned_before_durable_rollback() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = failure_procedure();

    let outcome = runtime
        .rollback_poison_after_begin(
            failure_request(),
            &procedure,
            TraceId::new(9101),
            "engine invariant violated by post-begin runtime witness",
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected, Some(0));
    assert!(outcome.completion.durable_lsn.is_some());

    let wal = runtime.wal();
    assert_eq!(wal.records.len(), 2);
    assert_eq!(wal.records[0].1, WalRecordKind::TxBegin);
    assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
    assert!(
        !wal.records
            .iter()
            .any(|(_, kind, _, _)| *kind == WalRecordKind::TxCommit)
    );
    assert!(wal.durable_lsn >= wal.records[1].0);
    assert_eq!(wal.records[1].3.len(), EXEC_TX_ROLLBACK_PAYLOAD_LEN);
}

#[test]
fn dispatcher_rollback_with_cause_records_intermediate_state() {
    for (cause, expected_intermediate) in [
        (RollbackCause::Direct, None),
        (
            RollbackCause::BusinessFailure,
            Some(TransactionState::Failed),
        ),
        (RollbackCause::Poison, Some(TransactionState::Poisoned)),
    ] {
        let mut wal = TestWal::default();
        let receipt = LocalDispatcher::new(&mut wal)
            .dispatch_rollback_with_cause(
                LocalRollbackPlan {
                    transaction_id: TransactionId::new(0xDEAD_BEEF),
                    rollback_payload: b"cause-routing".to_vec(),
                },
                cause,
            )
            .unwrap();

        assert_eq!(receipt.cause, cause);
        assert_eq!(receipt.intermediate_state, expected_intermediate);
        // RolledBack only after durable rollback flush regardless of cause.
        assert_eq!(receipt.transaction_state, TransactionState::RolledBack);
        assert!(receipt.durable_lsn >= receipt.wal_evidence.rollback_lsn);
    }
}

#[test]
fn poison_rollback_rejects_empty_reason() {
    let mut runtime = LocalVerticalRuntime::new(TestWal::default());
    let procedure = failure_procedure();

    let err = runtime
        .rollback_poison_after_begin(failure_request(), &procedure, TraceId::new(9102), "  ")
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Execution);
    assert!(runtime.wal().records.is_empty());
}
