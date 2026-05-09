pub use andromeda_admission::{
    ExecutionIoAdmissionDecision, ExecutionIoAdmissionRequest, InvocationContext, InvocationReject,
    InvocationRequest,
};
pub use andromeda_exec::LocalVerticalRuntime;
pub use andromeda_execution::LocalProcedure;
pub use andromeda_hardware::{PipelineClass, ResourceBudget};
pub use andromeda_inventory_demo::{
    INVENTORY_RESERVE_STOCK_PERMISSION, InventoryReserveStockExecutor, InventoryStock,
    ReserveStockCommand, inventory_reserve_stock_contract,
};
pub use andromeda_observe::{EventCorrelation, EventEmitter, EventId, InMemoryEventSink, TraceId};
pub use andromeda_procedure_contract::ProcedureContract;
pub use andromeda_result_stream::CompletionStatus;
pub use andromeda_storage_page::PageSize;
pub use andromeda_storage_placement::{
    CoreIoPlacementRequest, OperationalProfile, StorageIoBudgetScope, StorageWorkloadClass,
};
pub use andromeda_transaction::TransactionState;
pub use andromeda_types::{ContractHash, InvocationId, RequestId, TransactionId};
pub use andromeda_wal::{InMemoryWal, Lsn, WalRecordKind};

// SHARED FIXTURES

/// Inventory contract reference
pub(crate) fn valid_contract() -> ProcedureContract {
    inventory_reserve_stock_contract().expect("inventory contract should load")
}

/// Build a valid InvocationRequest for the inventory procedure
pub(crate) fn build_invocation_request(
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

/// Build a valid InvocationContext with required permissions
pub(crate) fn build_invocation_context(trace_id: u128) -> InvocationContext {
    InvocationContext::new(
        TraceId::new(trace_id),
        vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
    )
}

/// Standard foreground IO admission decision
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

/// Execute a reserved stock operation to get a LocalProcedure
pub(crate) fn execute_reserve_stock(
    quantity_available: i64,
    quantity_reserved: i64,
) -> LocalProcedure {
    let contract = valid_contract();
    let effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: quantity_reserved,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: quantity_available,
            version: 7,
        },
    )
    .expect("reserve should succeed");

    effect
        .to_local_procedure(&contract)
        .expect("procedure conversion should succeed")
}

/// Build EventCorrelation for traced events
pub(crate) fn build_event_correlation(
    contract: &ProcedureContract,
    transaction_id: Option<TransactionId>,
    durable_lsn: Option<Lsn>,
) -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(5001)),
        session_id: None,
        contract_hash: Some(contract.contract_hash),
        catalog_version: Some(contract.object.catalog_version),
        catalog_object_id: Some(contract.object.object_id),
        transaction_id,
        durable_lsn: durable_lsn.map(|lsn| lsn.get()),
        protocol: None,
    }
}
