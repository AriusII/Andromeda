#![allow(dead_code, unused_imports)]

pub use andromeda_error::AndromedaErrorKind;
pub use andromeda_exec::{
    CompletionStatus, ExecutionIoAdmissionRequest, InvocationContext, InvocationRequest,
    LocalVerticalRuntime,
};
pub use andromeda_hardware::{PipelineClass, ResourceBudget};
pub use andromeda_inventory_demo::{
    HeapInventoryProductStockStore, INVENTORY_RESERVE_STOCK_PERMISSION,
    InventoryProductStockCommitEvidence, InventoryProductStockDurableRedoEvidence,
    InventoryProductStockStore, InventoryReserveStockExecutor, InventoryStock, ReserveStockCommand,
    inventory_reserve_stock_catalog_bindings, inventory_reserve_stock_contract,
};
pub use andromeda_manifest::DatabaseManifest;
pub use andromeda_observability::{
    CompletionEmittedTrace, CriticalDecisionKind, EventCorrelation, EventId, ProtocolCorrelation,
    TraceId,
};
pub use andromeda_observe::{
    CommitVisibleTrace, EventEnvelope, RecoveryTrace, RollbackDurableTrace, TraceEvent,
    WalEventTrace, WalOperation,
};
pub use andromeda_procedure_contract::ProcedureContract;
pub use andromeda_recovery::{RecoveryPlan, RedoRecordDecision, StartupMode};
pub use andromeda_srpl_binder::inventory_reserve_stock_body_ir;
pub use andromeda_srpl_definition_batch::compile_narrow_procedure_signature;
pub use andromeda_srpl_ir::{
    Cardinality, SrplBusinessOperationKindIr, SrplPredicateIr, SrplValueIr,
};
pub use andromeda_storage_heap::{
    HeapRowRedoPayloadV1, LocalHeapRowInsertRedoTemplate, LocalHeapRowRedoContractBinding,
    ProductStockRow,
};
pub use andromeda_storage_page::{PageId, PageSize};
pub use andromeda_storage_placement::{
    CoreIoPlacementPolicy, CoreIoPlacementRequest, OperationalProfile, StorageIoBudgetScope,
    StorageTier, StorageWorkloadClass,
};
pub use andromeda_transaction::TransactionState;
pub use andromeda_types::{
    CatalogVersion, ContractHash, InvocationId, RequestId, SessionId, TransactionId,
};
pub use andromeda_wal::{
    DurableTransactionState, InMemoryWal, Lsn, WalRecordKind, classify_durable_transactions,
};

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
    let contract = inventory_reserve_stock_contract().unwrap();
    ExecutionIoAdmissionRequest::for_procedure(
        contract.procedure_id,
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
        segment_index_file_id: 0,
        btree_root_page_id: 0,
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
