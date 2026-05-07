pub use andromeda_catalog::{
    INVENTORY_RESERVE_STOCK_PERMISSION, ProcedureContract,
    inventory_reserve_stock_catalog_bindings, inventory_reserve_stock_contract,
};
pub use andromeda_core::{
    AndromedaErrorKind, CatalogVersion, ContractHash, InvocationId, PipelineClass, RequestId,
    ResourceBudget, SessionId, TransactionId,
};
pub use andromeda_exec::{
    CompletionStatus, ExecutionIoAdmissionRequest, HeapInventoryProductStockStore,
    InventoryProductStockCommitEvidence, InventoryProductStockDurableRedoEvidence,
    InventoryProductStockStore, InventoryReserveStockExecutor, InventoryStock, InvocationContext,
    InvocationRequest, LocalHeapRowInsertRedoTemplate, LocalHeapRowRedoContractBinding,
    LocalVerticalRuntime, ReserveStockCommand,
};
pub use andromeda_observe::{
    CommitVisibleTrace, CompletionEmittedTrace, CriticalDecisionKind, EventCorrelation,
    EventEnvelope, EventId, ProtocolCorrelation, RecoveryTrace, RollbackDurableTrace, TraceEvent,
    TraceId, WalEventTrace, WalOperation,
};
pub use andromeda_srpl::{
    Cardinality, SrplBusinessOperationKindIr, SrplPredicateIr, SrplValueIr,
    inventory_reserve_stock_body_ir, procedure_compiler::compile_narrow_procedure_signature,
};
pub use andromeda_storage::publication::DatabaseManifest;
pub use andromeda_storage::{
    CoreIoPlacementPolicy, CoreIoPlacementRequest, DurableTransactionState, InMemoryWal, Lsn,
    OperationalProfile, PageId, PageSize, ProductStockRow, RecoveryPlan, RedoRecordDecision,
    StartupMode, StorageIoBudgetScope, StorageTier, StorageWorkloadClass, WalRecordKind,
    classify_durable_transactions, write_ahead_log::HeapRowRedoPayloadV1,
};
pub use andromeda_tx::TransactionState;

pub(crate) fn inventory_reserve_stock_srpl_source() -> &'static str {
    "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }"
}

pub(crate) fn request_for(contract: &ProcedureContract, invocation_id: u64) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(invocation_id),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    }
}

pub(crate) fn product_stock_redo_binding(
    contract: &ProcedureContract,
) -> LocalHeapRowRedoContractBinding {
    let bindings =
        inventory_reserve_stock_catalog_bindings(contract.object.catalog_version).unwrap();
    LocalHeapRowRedoContractBinding::from_procedure_binding(
        bindings.product_stock_table.object_id,
        contract.binding(),
    )
    .unwrap()
}

pub(crate) fn foreground_io_admission(
    trace_id: TraceId,
) -> Result<andromeda_exec::ExecutionIoAdmissionDecision, andromeda_exec::InvocationReject> {
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

pub(crate) fn recovery_manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: required_wal_start_lsn,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 7,
    }
}

pub(crate) fn event_correlation(
    contract: &ProcedureContract,
    transaction_id: Option<TransactionId>,
    durable_lsn: Option<Lsn>,
) -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(9001)),
        session_id: Some(SessionId::new(9002)),
        contract_hash: Some(contract.contract_hash),
        catalog_version: Some(contract.object.catalog_version),
        catalog_object_id: Some(contract.object.object_id),
        transaction_id,
        durable_lsn: durable_lsn.map(|lsn| lsn.get()),
        protocol: Some(ProtocolCorrelation::empty()),
    }
}
