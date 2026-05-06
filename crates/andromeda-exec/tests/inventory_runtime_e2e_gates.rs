use andromeda_catalog::{
    INVENTORY_RESERVE_STOCK_PERMISSION, ProcedureContract,
    inventory_reserve_stock_catalog_bindings, inventory_reserve_stock_contract,
};
use andromeda_core::{
    AndromedaErrorKind, CatalogVersion, ContractHash, InvocationId, PipelineClass, RequestId,
    ResourceBudget, SessionId, TransactionId,
};
use andromeda_exec::{
    CompletionStatus, ExecutionIoAdmissionRequest, InventoryReserveStockExecutor, InventoryStock,
    InvocationContext, InvocationRequest, LocalVerticalRuntime, ReserveStockCommand,
};
use andromeda_observe::{
    CommitVisibleTrace, CompletionEmittedTrace, CriticalDecisionKind, EventCorrelation,
    EventEnvelope, EventId, ProtocolCorrelation, RecoveryTrace, RollbackDurableTrace, TraceEvent,
    TraceId, WalEventTrace, WalOperation,
};
use andromeda_srpl::{
    Cardinality, SrplBusinessOperationKindIr, SrplPredicateIr, SrplValueIr,
    inventory_reserve_stock_body_ir, procedure_compiler::compile_narrow_procedure_signature,
};
use andromeda_storage::publication::DatabaseManifest;
use andromeda_storage::{
    CoreIoPlacementPolicy, CoreIoPlacementRequest, DurableTransactionState, InMemoryWal, Lsn,
    OperationalProfile, PageSize, RecoveryPlan, RedoRecordDecision, StartupMode,
    StorageIoBudgetScope, StorageTier, StorageWorkloadClass, WalRecordKind,
    classify_durable_transactions,
};
use andromeda_tx::TransactionState;

fn inventory_reserve_stock_srpl_source() -> &'static str {
    "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }"
}

fn request_for(contract: &ProcedureContract, invocation_id: u64) -> InvocationRequest {
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

fn recovery_manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
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

fn event_correlation(
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

#[test]
fn reserve_stock_commit_gate_links_catalog_srpl_business_runtime_wal_recovery_observability_and_core_io()
 {
    let contract = inventory_reserve_stock_contract().unwrap();
    let bindings =
        inventory_reserve_stock_catalog_bindings(contract.object.catalog_version).unwrap();
    bindings.validate().unwrap();

    let srpl_ir =
        compile_narrow_procedure_signature(inventory_reserve_stock_srpl_source()).unwrap();
    assert_eq!(srpl_ir.name, contract.object.name);
    assert_eq!(srpl_ir.inputs, contract.inputs);
    assert_eq!(
        srpl_ir.result_streams[0].name,
        contract.result_streams[0].name
    );
    assert_eq!(
        srpl_ir.result_streams[0].columns,
        contract.result_streams[0].columns
    );
    assert_eq!(srpl_ir.result_streams[0].cardinality, Cardinality::One);
    assert!(srpl_ir.body.validate_bounded().is_ok());
    assert!(matches!(
        &srpl_ir.body.operations[0].kind,
        SrplBusinessOperationKindIr::Read {
            source,
            binding,
            cardinality,
            ..
        } if source == &bindings.product_stock_table.name
            && binding == "Stock"
            && *cardinality == Cardinality::One
    ));
    assert!(matches!(
        &srpl_ir.body.operations[3].kind,
        SrplBusinessOperationKindIr::Emit { stream, values }
            if stream == "Reservation" && values[0].column == "Reserved"
    ));

    let canonical_body = inventory_reserve_stock_body_ir().unwrap();
    assert!(matches!(
        &canonical_body.operations[1].kind,
        SrplBusinessOperationKindIr::Assert {
            predicate:
                SrplPredicateIr::FieldGreaterThanOrEqualInput {
                    binding,
                    field,
                    input,
                },
            failure_code,
        } if binding == "Stock"
            && field == "AvailableQuantity"
            && input == "Quantity"
            && failure_code == "InsufficientStock"
    ));
    assert!(matches!(
        &canonical_body.operations[2].kind,
        SrplBusinessOperationKindIr::Update { assignments, .. }
            if matches!(
                assignments.first(),
                Some(assignment)
                    if assignment.field == "AvailableQuantity"
                        && matches!(
                            &assignment.value,
                            SrplValueIr::SubtractInput {
                                binding,
                                field,
                                input,
                            } if binding == "Stock"
                                && field == "AvailableQuantity"
                                && input == "Quantity"
                        )
            )
    ));

    let profile = OperationalProfile::hot_write();
    profile.validate().unwrap();
    let page_budget = profile.workflow.page_budget.path_budget;
    let core_io_plan =
        CoreIoPlacementPolicy::new(profile.hardware.clone(), profile.workflow.thresholds)
            .plan(CoreIoPlacementRequest::new(
                StorageWorkloadClass::WalAppend,
                StorageIoBudgetScope::Page(PageSize::KiB16),
                page_budget,
                false,
            ))
            .unwrap();
    assert_eq!(core_io_plan.pipeline_class, PipelineClass::WalAppend);
    assert_eq!(core_io_plan.placement.target_tier, StorageTier::HotStore);
    assert!(!core_io_plan.gpu_enabled);

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
    assert_eq!(effect.next_stock.available_quantity, 7);
    assert_eq!(effect.next_stock.version, 8);
    assert!(effect.result.reserved);
    assert!(
        effect
            .result_evidence()
            .proves_exact_result_and_remaining_stock()
    );

    let procedure = effect.to_local_procedure(&contract).unwrap();
    assert_eq!(
        procedure.required_permissions,
        vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()]
    );
    assert_eq!(procedure.result_metadata.row_count_exact, Some(1));
    assert_eq!(procedure.result_metadata.cardinality, Cardinality::One);
    assert_eq!(procedure.rows_affected, 2);
    assert_eq!(procedure.mutation_payload, effect.mutation_payload());
    assert_ne!(
        procedure.mutation_payload.as_slice(),
        inventory_reserve_stock_srpl_source().as_bytes()
    );

    let invocation_id = 9100;
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let outcome = runtime
        .execute_authorized_io_admitted(
            request_for(&contract, invocation_id),
            &procedure,
            &InvocationContext::new(
                TraceId::new(9100),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
            foreground_io_admission(TraceId::new(9101)),
        )
        .unwrap();

    let tx_id = outcome.transaction_id;
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
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
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
        runtime.wal().records()[1].payload,
        effect.mutation_payload()
    );
    assert!(outcome.result_metadata.validate_completed_stream(1).is_ok());
    assert!(
        effect
            .result_evidence()
            .matches_committed_completion(&outcome.completion)
    );

    let durable_records = runtime.wal().replay_durable();
    let recovery_plan = RecoveryPlan::from_manifest_and_wal(
        &recovery_manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &durable_records,
    )
    .unwrap();
    let committed_redo = recovery_plan.committed_redo_records().collect::<Vec<_>>();
    assert_eq!(committed_redo.len(), 1);
    assert_eq!(committed_redo[0].kind, WalRecordKind::RowUpdate);
    assert_eq!(committed_redo[0].transaction_id, Some(tx_id));
    assert_eq!(
        committed_redo[0].transaction_state,
        Some(DurableTransactionState::Committed)
    );
    assert_eq!(
        durable_records
            .iter()
            .find(|record| record.header.lsn == committed_redo[0].lsn)
            .unwrap()
            .payload,
        effect.mutation_payload()
    );
    assert_eq!(
        recovery_plan.replay_lsns().collect::<Vec<_>>(),
        vec![Lsn::new(2)]
    );

    let durable_lsn = runtime.wal().durable_lsn();
    let correlation = event_correlation(&contract, Some(tx_id), Some(durable_lsn));
    let envelopes = [
        EventEnvelope::new(
            EventId::new(9101),
            correlation,
            TraceEvent::WalEvent(WalEventTrace {
                trace_id: TraceId::new(9101),
                transaction_id: Some(tx_id),
                operation: WalOperation::Flush,
                appended_lsn: durable_lsn.get(),
                durable_lsn: Some(durable_lsn.get()),
            }),
        )
        .unwrap(),
        EventEnvelope::new(
            EventId::new(9102),
            correlation,
            TraceEvent::CommitVisible(CommitVisibleTrace {
                trace_id: TraceId::new(9102),
                transaction_id: tx_id,
                durable_commit_lsn: durable_lsn.get(),
            }),
        )
        .unwrap(),
        EventEnvelope::new(
            EventId::new(9103),
            correlation,
            TraceEvent::CompletionEmitted(CompletionEmittedTrace {
                trace_id: TraceId::new(9103),
                protocol: ProtocolCorrelation::empty(),
                completion_code: Some(1),
                committed: true,
                durable_lsn: Some(durable_lsn.get()),
                reason: "Inventory.ReserveStock committed exact result metadata after durable WAL"
                    .to_string(),
            }),
        )
        .unwrap(),
        EventEnvelope::new(
            EventId::new(9104),
            correlation,
            TraceEvent::RecoveryStartup(RecoveryTrace {
                trace_id: TraceId::new(9104),
                last_durable_lsn: durable_lsn.get(),
                corruption_boundary_lsn: None,
            }),
        )
        .unwrap(),
        EventEnvelope::new(
            EventId::new(9105),
            correlation,
            TraceEvent::Decision(effect.result_evidence().decision_trace(TraceId::new(9105))),
        )
        .unwrap(),
    ];

    assert_eq!(
        envelopes
            .iter()
            .map(|envelope| envelope.event.kind())
            .collect::<Vec<_>>(),
        vec![
            CriticalDecisionKind::WalFlush,
            CriticalDecisionKind::CommitVisible,
            CriticalDecisionKind::CompletionEmitted,
            CriticalDecisionKind::RecoveryStartup,
            CriticalDecisionKind::BusinessRuleDecision,
        ]
    );
}

#[test]
fn insufficient_stock_rolls_back_with_typed_rejection_and_committed_only_recovery_evidence() {
    let contract = inventory_reserve_stock_contract().unwrap();
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

    let rejected_command = ReserveStockCommand {
        product_id: 42,
        quantity: 11,
    };
    let observed_stock = InventoryStock {
        product_id: 42,
        available_quantity: 10,
        version: 7,
    };
    let business_error =
        InventoryReserveStockExecutor::reserve(rejected_command, observed_stock).unwrap_err();
    assert_eq!(business_error.kind(), AndromedaErrorKind::Execution);
    assert!(
        business_error
            .message()
            .contains("insufficient inventory stock")
    );

    let rejection_evidence = InventoryReserveStockExecutor::rejection_evidence(
        rejected_command,
        observed_stock,
        business_error.message(),
    );
    assert!(rejection_evidence.has_business_rule_evidence());
    assert!(
        rejection_evidence
            .decision_trace(TraceId::new(9200))
            .reason
            .contains("Insufficient")
            || rejection_evidence
                .decision_trace(TraceId::new(9200))
                .reason
                .contains("insufficient")
    );

    let invocation_id = 9200;
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let outcome = runtime
        .rollback_authorized_business_validation_failure_after_begin(
            request_for(&contract, invocation_id),
            &procedure,
            &InvocationContext::new(
                TraceId::new(9200),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
            rejection_evidence.reason.as_str(),
        )
        .unwrap();

    let tx_id = outcome.transaction_id;
    assert_eq!(outcome.completion.status(), CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state(),
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected(), Some(0));
    assert_eq!(
        runtime
            .wal()
            .records()
            .iter()
            .map(|record| record.header.kind)
            .collect::<Vec<_>>(),
        vec![WalRecordKind::TxBegin, WalRecordKind::TxRollback]
    );
    assert!(!runtime.wal().records().iter().any(|record| matches!(
        record.header.kind,
        WalRecordKind::RowUpdate | WalRecordKind::TxCommit
    )));
    assert_eq!(observed_stock.available_quantity, 10);
    assert_eq!(observed_stock.version, 7);

    let durable_records = runtime.wal().replay_durable();
    let recovery_plan = RecoveryPlan::from_manifest_and_wal(
        &recovery_manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &durable_records,
    )
    .unwrap();
    assert!(recovery_plan.replay_lsns().next().is_none());
    assert!(recovery_plan.committed_redo_records().next().is_none());
    assert_eq!(
        recovery_plan
            .records
            .iter()
            .find(|record| record.lsn == Lsn::new(2))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipNonRedoRecord
    );

    let classifications = classify_durable_transactions(durable_records.iter());
    assert_eq!(
        classifications
            .rolled_back_transaction_ids()
            .collect::<Vec<_>>(),
        vec![tx_id]
    );
    assert!(classifications.committed_transaction_ids().next().is_none());

    let durable_lsn = runtime.wal().durable_lsn();
    let correlation = event_correlation(&contract, Some(tx_id), Some(durable_lsn));
    let rollback_envelope = EventEnvelope::new(
        EventId::new(9201),
        correlation,
        TraceEvent::RollbackDurable(RollbackDurableTrace {
            trace_id: TraceId::new(9201),
            transaction_id: tx_id,
            durable_rollback_lsn: durable_lsn.get(),
        }),
    )
    .unwrap();
    let completion_envelope = EventEnvelope::new(
        EventId::new(9202),
        correlation,
        TraceEvent::CompletionEmitted(CompletionEmittedTrace {
            trace_id: TraceId::new(9202),
            protocol: ProtocolCorrelation::empty(),
            completion_code: Some(2),
            committed: false,
            durable_lsn: Some(durable_lsn.get()),
            reason: "Inventory.ReserveStock rolled back business rejection with durable WAL"
                .to_string(),
        }),
    )
    .unwrap();
    let business_envelope = EventEnvelope::new(
        EventId::new(9203),
        correlation,
        TraceEvent::Decision(rejection_evidence.decision_trace(TraceId::new(9203))),
    )
    .unwrap();

    assert_eq!(
        rollback_envelope.event.kind(),
        CriticalDecisionKind::RollbackDurable
    );
    assert_eq!(
        completion_envelope.event.kind(),
        CriticalDecisionKind::CompletionEmitted
    );
    assert_eq!(
        business_envelope.event.kind(),
        CriticalDecisionKind::BusinessRuleDecision
    );
}

#[test]
fn stale_contract_rejection_stays_pre_transaction_with_no_wal_or_transaction_observability() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let effect = InventoryReserveStockExecutor::reserve(
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
    .unwrap();
    let procedure = effect.to_local_procedure(&contract).unwrap();

    let stale_request = InvocationRequest {
        expected_contract_hash: ContractHash::test_vector(99),
        catalog_version: CatalogVersion::new(99),
        ..request_for(&contract, 9300)
    };
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let error = runtime
        .execute_authorized(
            stale_request.clone(),
            &procedure,
            &InvocationContext::new(
                TraceId::new(9300),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
        )
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());

    let reject = stale_request
        .validate_before_transaction(procedure.contract, TraceId::new(9301))
        .unwrap_err();
    assert_eq!(reject.status, CompletionStatus::ContractRejected);
    assert!(
        reject.reason.contains("ContractHash") || reject.reason.contains("CatalogVersion"),
        "stale request rejection should identify contract/catalog mismatch: {}",
        reject.reason
    );

    let correlation = EventCorrelation {
        transaction_id: None,
        durable_lsn: None,
        ..event_correlation(&contract, None, None)
    };
    assert!(correlation.has_no_transaction_evidence());
    let envelope = EventEnvelope::new(
        EventId::new(9301),
        correlation,
        TraceEvent::ContractRejected(
            reject
                .contract_rejected_trace(TraceId::new(9301), ProtocolCorrelation::empty(), 1, 99)
                .unwrap(),
        ),
    )
    .unwrap();

    assert_eq!(
        envelope.event.kind(),
        CriticalDecisionKind::ContractRejected
    );
    assert!(envelope.correlation.has_no_transaction_evidence());
}
