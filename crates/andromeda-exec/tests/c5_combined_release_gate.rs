//! C5 combined execution/procedure release gate evidence.
//!
//! This contract joins the local execution admission surface with typed
//! Procedure contract validation, durable WAL completion evidence,
//! ResultStream metadata validation, observable audit events, and recovery
//! visibility. It deliberately uses the cataloged Inventory.ReserveStock
//! Procedure fixture instead of introducing ad hoc SQL or dynamic execution
//! shapes.

#[path = "inventory_runtime_e2e_gates/support.rs"]
mod support;

use andromeda_admission::AdmissionService;
use andromeda_inventory_demo::InventoryBusinessMvccStore;
use andromeda_mvcc::{MvccIsolationPolicy, Snapshot, TransactionStatus};
use andromeda_observe::{EventEmitter, InMemoryEventSink};
use andromeda_result_stream::ResultValidationService;
use support::*;

fn snapshot(
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

fn stock_row(quantity: i64, version: u64) -> InventoryStock {
    InventoryStock {
        product_id: 42,
        available_quantity: quantity,
        version,
    }
}

fn reserve_stock_procedure(contract: &ProcedureContract) -> andromeda_exec::LocalProcedure {
    InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 3,
        },
        stock_row(10, 7),
    )
    .unwrap()
    .to_local_procedure(contract)
    .unwrap()
}

#[test]
fn c5_contract_hash_rejection_is_audited_before_wal_or_transaction() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let trace_id = TraceId::new(51_001);
    let mut request = request_for(&contract, 51_001);
    request.expected_contract_hash = ContractHash::test_vector(0xC5);
    let procedure = reserve_stock_procedure(&contract);

    let reject = AdmissionService::validate_invocation_request(&request, trace_id)
        .expect_err("ContractHash drift must be rejected at admission");
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(reject.reason.contains("ContractHash"));

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let error = runtime
        .execute_authorized_io_admitted(
            request.clone(),
            &procedure,
            &InvocationContext::new(trace_id, contract.required_permissions.clone()),
            foreground_io_admission(trace_id),
        )
        .expect_err("ContractHash drift must not reach transaction creation");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(error.message().contains("ContractHash"));
    assert!(runtime.wal().records().is_empty());
    assert_eq!(runtime.transactions().live_count().unwrap(), 0);

    let audit_trace = reject
        .contract_rejected_trace(trace_id, ProtocolCorrelation::empty(), 1, 0xC5)
        .expect("ContractHash rejection must project contract audit evidence");
    let envelope = EventEnvelope::new(
        EventId::new(1),
        event_correlation(&contract, None, None),
        TraceEvent::ContractRejected(audit_trace),
    )
    .unwrap();
    assert!(envelope.correlation.has_no_transaction_evidence());

    let mut emitter = EventEmitter::new(InMemoryEventSink::new());
    emitter
        .emit_envelope(envelope)
        .expect("pre-transaction contract rejection audit must be accepted");
    assert_eq!(emitter.accepted_count(), 1);
    assert_eq!(emitter.rejected_count(), 0);
    assert_eq!(
        emitter.sink().events()[0].event.kind(),
        CriticalDecisionKind::ContractRejected
    );
}

#[test]
fn c5_commit_path_links_admission_wal_resultstream_audit_and_recovery_visibility() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let bindings = inventory_reserve_stock_catalog_bindings(contract.object.catalog_version)
        .expect("Inventory.ReserveStock bindings must exist");
    bindings.validate().unwrap();
    let srpl_ir = compile_narrow_procedure_signature(inventory_reserve_stock_srpl_source())
        .expect("cataloged SRPL fixture must compile");
    assert_eq!(srpl_ir.name, contract.object.name);
    assert!(srpl_ir.body.validate_bounded().is_ok());

    let trace_id = TraceId::new(51_100);
    let request = request_for(&contract, 51_100);
    let admission_trace = AdmissionService::validate_invocation_request(&request, trace_id)
        .expect("valid request must pass admission before runtime dispatch");
    assert_eq!(
        admission_trace.decision,
        CriticalDecisionKind::ResourceGovernance
    );
    assert!(admission_trace.has_explanation());

    let procedure = reserve_stock_procedure(&contract);
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let outcome = runtime
        .execute_authorized_io_admitted(
            request,
            &procedure,
            &InvocationContext::new(
                trace_id,
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
            foreground_io_admission(trace_id),
        )
        .expect("admitted typed Procedure invocation must commit");

    let transaction_id = outcome.transaction_id;
    let durable_lsn = outcome
        .completion
        .durable_lsn()
        .expect("committed completion must carry durable WAL evidence");
    let wal_evidence = outcome
        .wal_evidence
        .expect("committed invocation must carry WAL durability evidence");
    wal_evidence.validate().unwrap();

    assert_eq!(outcome.completion.status(), CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::Committed)
    );
    assert_eq!(outcome.completion.rows_affected(), Some(2));
    assert_eq!(
        outcome.completion.durable_lsn(),
        Some(runtime.wal().durable_lsn())
    );
    assert_eq!(durable_lsn, wal_evidence.durable_lsn);
    assert_eq!(wal_evidence.commit_lsn, durable_lsn);
    assert!(wal_evidence.begin_lsn < wal_evidence.commit_lsn);
    assert_eq!(
        runtime
            .wal()
            .records()
            .iter()
            .map(|record| record.header.kind)
            .collect::<Vec<_>>(),
        vec![
            WalRecordKind::TxBegin,
            WalRecordKind::RowUpdate,
            WalRecordKind::TxCommit,
        ]
    );
    assert_eq!(
        runtime.transactions().status(transaction_id).unwrap(),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        outcome.admission_trace.decision,
        CriticalDecisionKind::ResourceGovernance
    );
    assert_eq!(
        outcome.contract_trace.decision,
        CriticalDecisionKind::ContractValidation
    );
    assert_eq!(
        outcome
            .authorization_trace
            .as_ref()
            .expect("authorized path must record authorization decision")
            .decision,
        CriticalDecisionKind::SecurityAuthorization
    );

    ResultValidationService::validate_before_payload(outcome.result_metadata).unwrap();
    outcome
        .result_metadata
        .validate_terminal_completion(TransactionState::Committed, durable_lsn, 1)
        .expect("ResultStream completion must be bound to durable terminal state");
    assert_eq!(outcome.result_metadata.row_count_exact, Some(1));
    assert_eq!(outcome.result_metadata.cardinality, Cardinality::One);

    let mut emitter = EventEmitter::new(InMemoryEventSink::new());
    LocalVerticalRuntime::<InMemoryWal>::emit_commit_visible_event(
        &mut emitter,
        trace_id,
        transaction_id,
        durable_lsn,
        event_correlation(&contract, Some(transaction_id), Some(durable_lsn)),
    )
    .expect("commit-visible audit must be emitted after durable WAL");
    assert_eq!(emitter.accepted_count(), 1);
    assert_eq!(emitter.rejected_count(), 0);
    let audit_event = &emitter.sink().events()[0];
    assert_eq!(
        audit_event.event.kind(),
        CriticalDecisionKind::CommitVisible
    );
    assert_eq!(audit_event.correlation.transaction_id, Some(transaction_id));
    assert_eq!(audit_event.correlation.durable_lsn, Some(durable_lsn.get()));

    let durable_records = runtime.wal().replay_durable();
    let recovery_plan = RecoveryPlan::from_manifest_and_wal(
        &recovery_manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &durable_records,
    )
    .expect("durable WAL prefix must build a recovery plan");
    let recovered = recovery_plan
        .transaction_evidence
        .iter()
        .find(|resume| resume.transaction_id == transaction_id)
        .expect("committed transaction must appear in recovery evidence");
    assert_eq!(recovered.state, DurableTransactionState::Committed);
    assert_eq!(recovered.begin_lsn, Some(wal_evidence.begin_lsn));
    assert_eq!(recovered.commit_lsn, Some(wal_evidence.commit_lsn));
    assert_eq!(
        recovery_plan.committed_replay_lsns().collect::<Vec<_>>(),
        vec![
            wal_evidence
                .mutation_lsn
                .expect("committed mutation must be replay-visible")
        ]
    );
    assert_eq!(
        recovery_plan.recovered_transaction_id_floor(),
        transaction_id.get()
    );

    let mut store = InventoryBusinessMvccStore::new();
    store
        .seed_committed_stock(stock_row(10, 1), 10, TransactionId::new(50_000))
        .unwrap();
    let writer_snapshot = snapshot(20, transaction_id, [transaction_id]);
    let decision = store
        .reserve_stock(
            transaction_id,
            21,
            &writer_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 3,
            },
        )
        .expect("typed business write must prepare under writer snapshot");

    let before_commit = store
        .read_stock(
            42,
            &snapshot(22, TransactionId::new(50_001), [transaction_id]),
        )
        .unwrap()
        .expect("seed stock must remain visible before durable commit");
    assert_eq!(before_commit.observed_quantity, 10);

    store
        .commit_transaction_after_durable_wal(
            transaction_id,
            recovered
                .commit_lsn
                .expect("recovered committed transaction must carry commit LSN")
                .get(),
        )
        .expect("MVCC visibility publication must require recovered durable WAL evidence");
    let reader_after_recovery = snapshot(30, TransactionId::new(50_002), []);
    let visible_after_recovery = store
        .read_stock(42, &reader_after_recovery)
        .unwrap()
        .expect("committed stock must be visible after recovery evidence");
    assert_eq!(
        visible_after_recovery.observed_quantity,
        decision.effect.next_stock.available_quantity
    );
    assert_eq!(
        store.read_reservations(42, &reader_after_recovery).unwrap(),
        vec![decision.reservation]
    );
    assert_eq!(
        store.transaction_status(transaction_id),
        Some(TransactionStatus::Committed)
    );
}
