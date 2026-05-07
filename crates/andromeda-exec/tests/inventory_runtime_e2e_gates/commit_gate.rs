use crate::support::*;

#[test]
fn reserve_stock_commit_gate_links_catalog_srpl_business_runtime_wal_recovery_observability_and_core_io()
 {
    let contract = inventory_reserve_stock_contract().unwrap();
    let bindings =
        inventory_reserve_stock_catalog_bindings(contract.object.catalog_version).unwrap();
    bindings.validate().unwrap();
    let redo_binding = product_stock_redo_binding(&contract);

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

    let mut product_stock = HeapInventoryProductStockStore::from_cold_snapshot(
        PageId::new(42_201),
        PageSize::KiB16,
        effect.previous_stock,
    )
    .unwrap()
    .with_redo_contract_binding(redo_binding)
    .unwrap();
    assert_eq!(product_stock.redo_contract_binding(), Some(redo_binding));
    let prepared_product_stock = product_stock
        .prepare_reserve_stock(ReserveStockCommand {
            product_id: 42,
            quantity: 3,
        })
        .unwrap();
    assert_eq!(prepared_product_stock.effect, effect);
    assert_eq!(
        product_stock.visible_stock().unwrap(),
        effect.previous_stock
    );
    assert_eq!(
        product_stock.visible_product_stock_row().unwrap(),
        ProductStockRow::new(42, 10).unwrap()
    );
    assert!(product_stock.published_commit().is_none());
    let redo_template = product_stock
        .prepared_reserve_stock_redo_template(&prepared_product_stock)
        .unwrap();
    assert_eq!(redo_template.redo_binding(), Some(redo_binding));

    let mut procedure = effect.to_local_procedure(&contract).unwrap();
    assert_eq!(
        procedure.required_permissions,
        vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()]
    );
    assert_eq!(procedure.result_metadata.row_count_exact, Some(1));
    assert_eq!(procedure.result_metadata.cardinality, Cardinality::One);
    assert_eq!(procedure.rows_affected, 2);
    assert_eq!(procedure.mutation_payload, effect.mutation_payload());
    let logical_mutation_payload = procedure.mutation_payload.clone();
    procedure.mutation_payload = redo_template.encode_template().unwrap();
    assert_eq!(
        LocalHeapRowInsertRedoTemplate::try_decode_template(&procedure.mutation_payload)
            .unwrap()
            .unwrap()
            .redo_binding(),
        Some(redo_binding)
    );
    assert_ne!(procedure.mutation_payload, logical_mutation_payload);
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
            foreground_io_admission(TraceId::new(9100)),
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
            WalRecordKind::RowInsert,
            WalRecordKind::TxCommit,
        ]
    );
    let wal_redo = HeapRowRedoPayloadV1::decode(
        &runtime.wal().records()[1].payload,
        runtime.wal().records()[1].header.kind,
    )
    .unwrap();
    assert_eq!(wal_redo.resulting_page_lsn(), Lsn::new(2));
    assert_eq!(
        ProductStockRow::decode(wal_redo.tuple()).unwrap(),
        ProductStockRow::new(42, 7).unwrap()
    );
    assert!(outcome.result_metadata.validate_completed_stream(1).is_ok());
    assert!(
        effect
            .result_evidence()
            .matches_committed_completion(&outcome.completion)
    );

    let product_stock_commit =
        InventoryProductStockCommitEvidence::new(tx_id, runtime.wal().durable_lsn()).unwrap();
    let wal_evidence = outcome.wal_evidence.unwrap();
    let redo_record_lsn = wal_evidence.mutation_lsn.unwrap();
    let redo_payload = redo_template
        .materialize_heap_redo_payload(redo_record_lsn)
        .unwrap();
    assert_eq!(redo_payload, wal_redo);
    let product_stock_redo = InventoryProductStockDurableRedoEvidence::new(
        tx_id,
        runtime.wal().durable_lsn(),
        redo_payload.clone(),
    )
    .unwrap();
    let missing_binding_error = product_stock
        .publish_committed_reserve_stock_with_redo(
            &prepared_product_stock,
            product_stock_commit,
            product_stock_redo,
        )
        .unwrap_err();
    assert_eq!(missing_binding_error.kind(), AndromedaErrorKind::Contract);
    assert_eq!(
        product_stock.visible_stock().unwrap(),
        effect.previous_stock
    );

    let product_stock_redo = InventoryProductStockDurableRedoEvidence::new_with_binding(
        tx_id,
        runtime.wal().durable_lsn(),
        redo_binding,
        redo_payload,
    )
    .unwrap();
    assert_eq!(product_stock_redo.redo_binding, Some(redo_binding));
    product_stock
        .publish_committed_reserve_stock_with_redo(
            &prepared_product_stock,
            product_stock_commit,
            product_stock_redo.clone(),
        )
        .unwrap();
    assert_eq!(product_stock.visible_stock().unwrap(), effect.next_stock);
    assert_eq!(
        product_stock.visible_product_stock_row().unwrap(),
        ProductStockRow::new(42, 7).unwrap()
    );
    assert_eq!(product_stock.active_heap_slot_count(), 2);
    assert_eq!(product_stock.published_commit(), Some(product_stock_commit));
    assert_eq!(product_stock.page_lsn(), product_stock_redo.redo_record_lsn);
    assert_eq!(
        product_stock
            .last_redo_payload()
            .unwrap()
            .resulting_page_lsn(),
        product_stock_redo.redo_record_lsn
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
    assert_eq!(committed_redo[0].kind, WalRecordKind::RowInsert);
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
        product_stock_redo.redo_payload.encode()
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
