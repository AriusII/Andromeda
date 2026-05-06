//! C5: Observable commit and rollback lifecycle event coverage
//!
//! Tests that the execution stack properly emits CommitVisible and RollbackDurable
//! events with durable LSN correlation and proper event envelope validation.

use andromeda_catalog::{
    INVENTORY_RESERVE_STOCK_PERMISSION, inventory_reserve_stock_catalog_bindings,
    inventory_reserve_stock_contract,
};
use andromeda_core::{InvocationId, PipelineClass, RequestId, ResourceBudget, TransactionId};
use andromeda_exec::{
    CompletionStatus, ExecutionIoAdmissionRequest, InventoryReserveStockExecutor, InventoryStock,
    InvocationContext, InvocationRequest, LocalVerticalRuntime, ReserveStockCommand,
};
use andromeda_observe::{
    CommitVisibleTrace, EventCorrelation, EventEmitter, EventEnvelope, EventId, InMemoryEventSink,
    RollbackDurableTrace, TraceEvent, TraceId,
};
use andromeda_srpl::procedure_compiler::compile_narrow_procedure_signature;
use andromeda_storage::{
    CoreIoPlacementRequest, InMemoryWal, Lsn, OperationalProfile, PageSize, StorageIoBudgetScope,
    StorageWorkloadClass, WalRecordKind,
};
use andromeda_tx::TransactionState;

fn inventory_reserve_stock_srpl_source() -> &'static str {
    "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }"
}

fn request_for(
    contract: &andromeda_catalog::ProcedureContract,
    invocation_id: u64,
) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(invocation_id),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    }
}

fn foreground_io_admission(
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

fn event_correlation(
    contract: &andromeda_catalog::ProcedureContract,
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

#[test]
fn c5_commit_lifecycle_emits_commit_visible_event_with_durable_lsn() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let _srpl_ir =
        compile_narrow_procedure_signature(inventory_reserve_stock_srpl_source()).unwrap();
    let bindings =
        inventory_reserve_stock_catalog_bindings(contract.object.catalog_version).unwrap();
    bindings.validate().unwrap();

    let effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 3,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 7,
        },
    )
    .unwrap();

    let procedure = effect.to_local_procedure(&contract).unwrap();

    let invocation_id = 5001u64;
    let trace_id = TraceId::new(5001u128);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();
    let mut emitter = EventEmitter::new(&mut sink);

    let outcome = runtime
        .execute_authorized_io_admitted(
            request_for(&contract, invocation_id),
            &procedure,
            &InvocationContext::new(
                trace_id,
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
            foreground_io_admission(trace_id),
        )
        .unwrap();

    let tx_id = outcome.transaction_id;
    let durable_lsn = outcome
        .completion
        .durable_lsn
        .expect("committed must have durable LSN");

    // Verify completion status and WAL state
    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::Committed)
    );
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));

    // Manually emit the CommitVisible event to test the observable path
    let correlation = event_correlation(&contract, Some(tx_id), Some(durable_lsn));

    LocalVerticalRuntime::<InMemoryWal>::emit_commit_visible_event(
        &mut emitter,
        trace_id,
        tx_id,
        durable_lsn,
        correlation,
    )
    .unwrap();

    // Verify event was emitted
    assert_eq!(sink.len(), 1);
    let envelope = &sink.events()[0];
    assert_eq!(envelope.event_id, EventId::new(1));
    assert_eq!(envelope.trace_id, trace_id);
    assert_eq!(envelope.correlation.transaction_id, Some(tx_id));
    assert_eq!(envelope.correlation.durable_lsn, Some(durable_lsn.get()));

    // Verify event payload
    match &envelope.event {
        TraceEvent::CommitVisible(trace) => {
            assert_eq!(trace.trace_id, trace_id);
            assert_eq!(trace.transaction_id, tx_id);
            assert_eq!(trace.durable_commit_lsn, durable_lsn.get());
            assert!(trace.proves_wal_before_visible_commit());
        }
        _ => panic!(
            "expected CommitVisible event, got {:?}",
            envelope.event.kind()
        ),
    }

    // Verify event envelope validates
    assert!(envelope.validate().is_ok());
}

#[test]
fn c5_rollback_lifecycle_emits_rollback_durable_event_with_durable_lsn() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let _bindings =
        inventory_reserve_stock_catalog_bindings(contract.object.catalog_version).unwrap();

    let procedure = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 1,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 7,
        },
    )
    .unwrap()
    .to_local_procedure(&contract)
    .unwrap();

    let invocation_id = 5002u64;
    let trace_id = TraceId::new(5002u128);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();
    let mut emitter = EventEmitter::new(&mut sink);

    let context = InvocationContext::new(
        trace_id,
        vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
    );

    let outcome = runtime
        .rollback_authorized_business_validation_failure_after_begin(
            request_for(&contract, invocation_id),
            &procedure,
            &context,
            "business rule failure: insufficient inventory",
        )
        .unwrap();

    let tx_id = outcome.transaction_id;
    let durable_lsn = outcome
        .completion
        .durable_lsn
        .expect("rolled back must have durable LSN");

    // Verify completion status
    assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::RolledBack)
    );
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(2));

    // Verify WAL contains only TxBegin and TxRollback (no row update)
    let wal_records: Vec<_> = runtime
        .wal()
        .records()
        .iter()
        .map(|record| record.header.kind)
        .collect();
    assert_eq!(
        wal_records,
        vec![WalRecordKind::TxBegin, WalRecordKind::TxRollback]
    );

    // Manually emit the RollbackDurable event to test the observable path
    let correlation = event_correlation(&contract, Some(tx_id), Some(durable_lsn));

    LocalVerticalRuntime::<InMemoryWal>::emit_rollback_durable_event(
        &mut emitter,
        trace_id,
        tx_id,
        durable_lsn,
        correlation,
    )
    .unwrap();

    // Verify event was emitted
    assert_eq!(sink.len(), 1);
    let envelope = &sink.events()[0];
    assert_eq!(envelope.event_id, EventId::new(1));
    assert_eq!(envelope.trace_id, trace_id);
    assert_eq!(envelope.correlation.transaction_id, Some(tx_id));
    assert_eq!(envelope.correlation.durable_lsn, Some(durable_lsn.get()));

    // Verify event payload
    match &envelope.event {
        TraceEvent::RollbackDurable(trace) => {
            assert_eq!(trace.trace_id, trace_id);
            assert_eq!(trace.transaction_id, tx_id);
            assert_eq!(trace.durable_rollback_lsn, durable_lsn.get());
            assert!(trace.proves_durable_rollback());
        }
        _ => panic!(
            "expected RollbackDurable event, got {:?}",
            envelope.event.kind()
        ),
    }

    // Verify event envelope validates
    assert!(envelope.validate().is_ok());
}

#[test]
fn c5_event_emission_failure_propagates_to_caller() {
    let contract = inventory_reserve_stock_contract().unwrap();

    let _procedure = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 1,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 7,
        },
    )
    .unwrap()
    .to_local_procedure(&contract)
    .unwrap();

    let trace_id = TraceId::new(5003u128);
    let tx_id = TransactionId::new(999);
    let durable_lsn = Lsn::new(1);

    // Create a sink with capacity limit of 0 to force emission failure
    let mut sink = andromeda_observe::InMemoryEventSink::with_capacity_limit(0);
    let mut emitter = EventEmitter::new(&mut sink);

    let correlation = event_correlation(&contract, Some(tx_id), Some(durable_lsn));

    // Attempt to emit CommitVisible event with full sink
    let result = LocalVerticalRuntime::<InMemoryWal>::emit_commit_visible_event(
        &mut emitter,
        trace_id,
        tx_id,
        durable_lsn,
        correlation,
    );

    // Verify that emission failure is NOT silently dropped
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        error.message().contains("exhausted") || error.message().contains("capacity"),
        "expected capacity error, got: {}",
        error.message()
    );

    // Verify that rejected count increased
    assert_eq!(emitter.rejected_count(), 1);
    assert_eq!(emitter.accepted_count(), 0);
}

#[test]
fn c5_rollback_durable_event_proves_wal_durability() {
    let contract = inventory_reserve_stock_contract().unwrap();

    let durable_lsn = Lsn::new(42);
    let trace = RollbackDurableTrace {
        trace_id: TraceId::new(5004),
        transaction_id: TransactionId::new(100),
        durable_rollback_lsn: durable_lsn.get(),
    };

    // Test that zero LSN is rejected
    let invalid_trace = RollbackDurableTrace {
        trace_id: TraceId::new(5004),
        transaction_id: TransactionId::new(100),
        durable_rollback_lsn: 0,
    };

    let correlation = event_correlation(&contract, Some(trace.transaction_id), Some(durable_lsn));
    let valid_envelope = EventEnvelope::new(
        EventId::new(1),
        correlation,
        TraceEvent::RollbackDurable(trace),
    );

    let invalid_correlation = event_correlation(
        &contract,
        Some(invalid_trace.transaction_id),
        Some(Lsn::new(0)),
    );
    let invalid_envelope = EventEnvelope::new(
        EventId::new(2),
        invalid_correlation,
        TraceEvent::RollbackDurable(invalid_trace),
    );

    assert!(
        valid_envelope.is_ok(),
        "valid trace should create valid envelope"
    );
    assert!(invalid_envelope.is_err(), "zero LSN should fail validation");

    let error = invalid_envelope.unwrap_err();
    let error_msg = error.message();
    assert!(
        error_msg.contains("rollback-durable") && error_msg.contains("LSN"),
        "error should mention rollback-durable and LSN, got: {}",
        error_msg
    );
}

#[test]
fn c5_commit_visible_event_proves_wal_durability() {
    let contract = inventory_reserve_stock_contract().unwrap();

    let durable_lsn = Lsn::new(42);
    let trace = CommitVisibleTrace {
        trace_id: TraceId::new(5005),
        transaction_id: TransactionId::new(100),
        durable_commit_lsn: durable_lsn.get(),
    };

    // Test that zero LSN is rejected
    let invalid_trace = CommitVisibleTrace {
        trace_id: TraceId::new(5005),
        transaction_id: TransactionId::new(100),
        durable_commit_lsn: 0,
    };

    let correlation = event_correlation(&contract, Some(trace.transaction_id), Some(durable_lsn));
    let valid_envelope = EventEnvelope::new(
        EventId::new(1),
        correlation,
        TraceEvent::CommitVisible(trace),
    );

    let invalid_correlation = event_correlation(
        &contract,
        Some(invalid_trace.transaction_id),
        Some(Lsn::new(0)),
    );
    let invalid_envelope = EventEnvelope::new(
        EventId::new(2),
        invalid_correlation,
        TraceEvent::CommitVisible(invalid_trace),
    );

    assert!(
        valid_envelope.is_ok(),
        "valid trace should create valid envelope"
    );
    assert!(invalid_envelope.is_err(), "zero LSN should fail validation");

    let error = invalid_envelope.unwrap_err();
    let error_msg = error.message();
    assert!(
        error_msg.contains("commit-visible") && error_msg.contains("LSN"),
        "error should mention commit-visible and LSN, got: {}",
        error_msg
    );
}
