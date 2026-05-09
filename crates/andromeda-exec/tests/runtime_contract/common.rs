pub(crate) use andromeda_catalog::{
    CatalogBindingKind, INVENTORY_RESERVE_STOCK_PERMISSION, PolicyVersion,
    ProcedureContractBinding, ProcedureContractRef, StatsVersion,
    inventory_reserve_stock_catalog_bindings, inventory_reserve_stock_contract,
};
pub(crate) use andromeda_core::{
    AndromedaError, AndromedaResult, CatalogVersion, ContractHash, InvocationId, PipelineClass,
    ProcedureId, ResourceBudget, TransactionId,
};
pub(crate) use andromeda_exec::{
    AdmissionService, CompletionMappingService, CompletionStatus, ExecutionIoAdmissionDecision,
    ExecutionIoAdmissionRequest, InventoryBusinessMvccStore, InventoryReserveStockExecutor,
    InventoryStock, InvocationContext, InvocationReject, InvocationRequest, InvocationWal,
    LocalDispatchPlan, LocalDispatcher, LocalProcedure, LocalRollbackPlan, LocalVerticalRuntime,
    PreTransactionValidationService, ReserveStockCommand, ResultStreamMetadata,
    ResultValidationService,
};
pub(crate) use andromeda_observe::TraceId;
pub(crate) use andromeda_srpl::{
    Cardinality, SrplBusinessOperationKindIr, SrplPredicateIr, SrplValueIr,
    inventory_reserve_stock_body_ir, procedure_compiler::compile_narrow_procedure_signature,
};
pub(crate) use andromeda_storage::{
    CoreIoPlacementRequest, InMemoryWal, Lsn, OperationalProfile, PageSize, StorageIoBudgetScope,
    StorageWorkloadClass, WalRecordKind,
};
pub(crate) use andromeda_tx::TransactionState;
pub(crate) use andromeda_tx::{MvccIsolationPolicy, Snapshot, TransactionStatus};

#[derive(Debug, Default)]
pub(crate) struct RecordingWal {
    pub(crate) records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
    pub(crate) durable_lsn: Lsn,
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

#[derive(Debug, Default)]
pub(crate) struct LaggingFlushWal {
    pub(crate) records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
    pub(crate) durable_lsn: Lsn,
}

impl InvocationWal for LaggingFlushWal {
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
        self.durable_lsn = Lsn::new(lsn.get() - 1);
        Ok(self.durable_lsn)
    }
}

#[derive(Debug, Default)]
pub(crate) struct CommitAppendErrorWal {
    pub(crate) records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
}

impl InvocationWal for CommitAppendErrorWal {
    fn append(
        &mut self,
        kind: WalRecordKind,
        transaction_id: Option<TransactionId>,
        payload: &[u8],
    ) -> AndromedaResult<Lsn> {
        if kind == WalRecordKind::TxCommit {
            return Err(AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Storage,
                "injected commit append failure",
            ));
        }

        let lsn = Lsn::new(self.records.len() as u64 + 1);
        self.records
            .push((lsn, kind, transaction_id, payload.to_vec()));
        Ok(lsn)
    }

    fn flush_through(&mut self, _lsn: Lsn) -> AndromedaResult<Lsn> {
        unreachable!("commit append failure must prevent flush");
    }
}

#[derive(Debug, Default)]
pub(crate) struct FlushErrorWal {
    pub(crate) records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
}

impl InvocationWal for FlushErrorWal {
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

    fn flush_through(&mut self, _lsn: Lsn) -> AndromedaResult<Lsn> {
        Err(AndromedaError::new(
            andromeda_core::AndromedaErrorKind::Storage,
            "injected WAL flush failure",
        ))
    }
}

pub(crate) fn request(expected_contract_hash: ContractHash) -> InvocationRequest {
    let procedure = ProcedureContractRef {
        procedure_id: ProcedureId::new(22),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(3),
    };
    InvocationRequest {
        invocation_id: InvocationId::new(11),
        procedure,
        expected_binding: Some(test_binding(procedure)),
        expected_contract_hash,
        catalog_version: CatalogVersion::new(3),
        structured_parameters: Vec::new(),
    }
}

pub(crate) fn procedure(contract: ProcedureContractRef) -> LocalProcedure {
    LocalProcedure {
        contract,
        contract_binding: test_binding(contract),
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

fn test_binding(procedure: ProcedureContractRef) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: procedure.procedure_id,
        catalog_version: procedure.catalog_version,
        contract_hash: procedure.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([7; PolicyVersion::LEN]),
    }
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

pub(crate) fn rejected_io_admission(
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

pub(crate) fn inventory_reserve_stock_srpl_source() -> &'static str {
    "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }"
}

pub(crate) fn tx_snapshot(
    timestamp: u64,
    transaction_id: TransactionId,
    active_tx_ids: impl IntoIterator<Item = TransactionId>,
) -> Snapshot {
    Snapshot::with_context(
        timestamp,
        CatalogVersion::new(3),
        MvccIsolationPolicy::RepeatableRead,
        Some(transaction_id),
        active_tx_ids,
    )
    .unwrap()
}
