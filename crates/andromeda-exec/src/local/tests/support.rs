pub(crate) use super::super::{LocalProcedure, LocalVerticalRuntime};
pub(crate) use andromeda_catalog::{
    CatalogSystemStore, ProcedureStore, inventory_domain_definition_batch,
    inventory_reserve_stock_contract,
};
pub(crate) use andromeda_core::{
    AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, DatabaseId, InvocationId,
    NamespaceId, PipelineClass, ProcedureId, ResourceBudget, TransactionId,
};
pub(crate) use andromeda_definition_batch::{
    CatalogLifecycleTarget, DefinitionBatch, DefinitionBatchId, DefinitionOperation,
};
pub(crate) use andromeda_observe::{
    AuthorizationDenialReason, AuthorizationOutcome, CertificateIdentity, Permission,
    PrincipalBinding, PrincipalRegistry, SecurityAuditOutcome, SurfaceScope, TraceId,
    UserPrincipal, UserPrincipalKind,
};
pub(crate) use andromeda_procedure_contract::{
    PolicyVersion, ProcedureContract, ProcedureContractBinding, ProcedureContractRef, StatsVersion,
};
pub(crate) use andromeda_procedure_store::{
    InvocationRuntimeRecordOutcome, ProcedureRuntimePlanId, ProcedureRuntimeStatus,
    ProcedureStoreEntry,
};
pub(crate) use andromeda_quic::SurfacePlane;
pub(crate) use andromeda_srpl_ir::Cardinality;
pub(crate) use andromeda_storage::{
    CoreIoPlacementRequest, OperationalProfile, StorageIoBudgetScope, StorageWorkloadClass,
};
pub(crate) use andromeda_storage_page::PageSize;
pub(crate) use andromeda_transaction::TransactionState;
pub(crate) use andromeda_wal::{InMemoryWal, Lsn, WalRecordKind};

pub(crate) use crate::{
    CompletionStatus, ExecutionIoAdmissionDecision, ExecutionIoAdmissionRequest, InvocationContext,
    InvocationReject, InvocationRequest, InvocationWal, LocalDispatcher, LocalRollbackPlan,
    ResultStreamMetadata, RollbackCause, SurfacePlaneAuthorizer,
};

#[derive(Debug, Default)]
pub(crate) struct TestWal {
    pub(crate) records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
    pub(crate) durable_lsn: Lsn,
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

pub(crate) fn request(expected_contract_hash: ContractHash) -> InvocationRequest {
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

pub(crate) fn simple_local_procedure() -> LocalProcedure {
    let request = request(ContractHash::test_vector(7));
    LocalProcedure {
        contract: request.procedure,
        contract_binding: request.expected_binding.unwrap(),
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

pub(crate) fn inventory_request(contract: &ProcedureContract) -> InvocationRequest {
    inventory_request_with_id(contract, 700)
}

pub(crate) fn inventory_request_with_id(
    contract: &ProcedureContract,
    invocation_id: u64,
) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(invocation_id),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    }
}

pub(crate) fn inventory_local_procedure(contract: &ProcedureContract) -> LocalProcedure {
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

pub(crate) fn inventory_context(contract: &ProcedureContract, trace_id: u128) -> InvocationContext {
    InvocationContext::new(
        TraceId::new(trace_id),
        contract.required_permissions.clone(),
    )
}

pub(crate) fn foreground_io_admission(
    trace_id: TraceId,
) -> Result<ExecutionIoAdmissionDecision, InvocationReject> {
    let profile = OperationalProfile::hot_write();
    ExecutionIoAdmissionRequest::new(
        profile.clone(),
        PipelineClass::ForegroundExecution,
        ResourceBudget::new(8 * 1024 * 1024, 1024 * 1024, 2),
        CoreIoPlacementRequest::new(
            StorageWorkloadClass::HotAppend,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            profile.workflow.page_budget.path_budget,
            false,
        ),
    )
    .validate_admission(trace_id)
}

pub(crate) fn rejected_resource_budget_io_admission(
    trace_id: TraceId,
) -> Result<ExecutionIoAdmissionDecision, InvocationReject> {
    let profile = OperationalProfile::hot_write();
    ExecutionIoAdmissionRequest::new(
        profile.clone(),
        PipelineClass::ForegroundExecution,
        ResourceBudget::new(0, 1024 * 1024, 2),
        CoreIoPlacementRequest::new(
            StorageWorkloadClass::HotAppend,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            profile.workflow.page_budget.path_budget,
            false,
        ),
    )
    .validate_admission(trace_id)
}

pub(crate) fn test_binding(procedure: ProcedureContractRef) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: procedure.procedure_id,
        catalog_version: procedure.catalog_version,
        contract_hash: procedure.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([7; PolicyVersion::LEN]),
    }
}

pub(crate) fn staged_inventory_catalog_store() -> CatalogSystemStore {
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

pub(crate) fn inventory_catalog_store() -> CatalogSystemStore {
    let mut store = CatalogSystemStore::empty(
        DatabaseId::new(0x1000),
        NamespaceId::new(0x1001),
        CatalogVersion::new(0),
    );
    apply_definition_batch_durably_for_test(
        &mut store,
        &inventory_domain_definition_batch().unwrap(),
    );
    store
}

pub(crate) fn apply_definition_batch_durably_for_test(
    store: &mut CatalogSystemStore,
    batch: &DefinitionBatch,
) {
    let mut next_lsn = store
        .snapshot()
        .visible_publication_receipt()
        .and_then(|receipt| receipt.durable_lsn)
        .unwrap_or(0);
    store
        .apply_definition_batch_durably(
            batch,
            |_kind, _payload| {
                next_lsn += 1;
                Ok(next_lsn)
            },
            Ok,
        )
        .unwrap();
}

pub(crate) fn principal_registry(bindings: Vec<PrincipalBinding>) -> PrincipalRegistry {
    let mut registry = PrincipalRegistry::new();
    for binding in bindings {
        registry.register(binding).unwrap();
    }
    registry
}

pub(crate) fn principal_binding(
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

pub(crate) fn failure_request() -> InvocationRequest {
    request(ContractHash::test_vector(7))
}

pub(crate) fn failure_procedure() -> LocalProcedure {
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
