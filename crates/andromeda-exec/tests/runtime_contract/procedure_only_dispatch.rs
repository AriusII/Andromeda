use crate::common::*;

#[test]
fn inventory_reserve_stock_e2e_stitches_catalog_srpl_business_effect_and_authorized_commit() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let bindings =
        inventory_reserve_stock_catalog_bindings(contract.object.catalog_version).unwrap();
    bindings.validate().unwrap();
    assert_eq!(
        bindings.product_stock_table_binding.kind,
        CatalogBindingKind::WritesTable
    );
    assert_eq!(
        bindings.reservation_result_binding.kind,
        CatalogBindingKind::EmitsStructuredObject
    );

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
    assert_eq!(srpl_ir.body.operations.len(), 4);
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
        &srpl_ir.body.operations[2].kind,
        SrplBusinessOperationKindIr::Update {
            target,
            assignments,
            ..
        } if target == &bindings.product_stock_table.name
            && assignments.len() == 1
            && assignments[0].field == "AvailableQuantity"
    ));
    assert!(matches!(
        &srpl_ir.body.operations[3].kind,
        SrplBusinessOperationKindIr::Emit { stream, values }
            if stream == "Reservation"
                && values.len() == 1
                && values[0].column == "Reserved"
    ));

    let canonical_body = inventory_reserve_stock_body_ir().unwrap();
    assert!(matches!(
        &canonical_body.operations[0].kind,
        SrplBusinessOperationKindIr::Read { predicates, .. }
            if matches!(
                predicates.first(),
                Some(SrplPredicateIr::InputEqualsField {
                    input,
                    binding,
                    field,
                }) if input == "ProductId" && binding == "Stock" && field == "ProductId"
            )
    ));
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

    let command = ReserveStockCommand {
        product_id: 42,
        quantity: 3,
    };
    let stock = InventoryStock {
        product_id: 42,
        available_quantity: 10,
        version: 7,
    };
    let effect = InventoryReserveStockExecutor::reserve(command, stock).unwrap();
    assert_eq!(effect.next_stock.available_quantity, 7);
    assert_eq!(effect.next_stock.version, 8);
    assert!(effect.result.reserved);

    let procedure = effect.to_local_procedure(&contract).unwrap();
    assert_eq!(
        procedure.required_permissions,
        vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()]
    );
    assert_eq!(procedure.rows_affected, 2);
    assert_ne!(
        procedure.mutation_payload.as_slice(),
        inventory_reserve_stock_srpl_source().as_bytes()
    );

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let request = InvocationRequest {
        invocation_id: InvocationId::new(710),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };

    let outcome = runtime
        .execute_internal_authorized(
            request,
            &procedure,
            &InvocationContext::new(
                TraceId::new(7100),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
        )
        .unwrap();

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
    assert_eq!(runtime.wal().records().len(), 3);
    assert_eq!(
        runtime.wal().records()[0].header.kind,
        WalRecordKind::TxBegin
    );
    assert_eq!(
        runtime.wal().records()[1].header.kind,
        WalRecordKind::RowUpdate
    );
    assert_eq!(
        runtime.wal().records()[2].header.kind,
        WalRecordKind::TxCommit
    );
    assert_eq!(runtime.wal().replay_durable().len(), 3);
    assert!(outcome.authorization_trace.is_some());
    assert!(
        effect
            .result_evidence()
            .proves_exact_result_and_remaining_stock()
    );
    assert!(
        effect
            .result_evidence()
            .matches_committed_completion(&outcome.completion)
    );
}

#[test]
fn service_and_dispatcher_api_is_usable_externally() {
    let request = request(ContractHash::test_vector(7));
    let trace = PreTransactionValidationService::validate_invocation_contract(
        &request,
        request.expected_binding.unwrap(),
        TraceId::new(202),
    )
    .unwrap();
    assert!(trace.has_explanation());

    let auth_trace = AdmissionService::authorize(
        &InvocationContext::new(
            TraceId::new(203),
            vec!["Inventory.ReserveStock.Execute".into()],
        ),
        &["Inventory.ReserveStock.Execute".into()],
    )
    .unwrap();
    assert!(auth_trace.has_explanation());

    let mut wal = RecordingWal::default();
    let receipt = LocalDispatcher::new(&mut wal)
        .dispatch_commit(LocalDispatchPlan {
            transaction_id: TransactionId::new(request.invocation_id.get()),
            mutation_payload: b"reserve-stock".to_vec(),
            rows_affected: 1,
        })
        .unwrap();

    assert_eq!(receipt.transaction_state, TransactionState::Committed);
    assert_eq!(receipt.durable_lsn, Lsn::new(3));
    assert_eq!(receipt.wal_evidence.commit_lsn, Lsn::new(3));
    assert_eq!(receipt.wal_evidence.durable_lsn, Lsn::new(3));
    assert_eq!(wal.durable_lsn, Lsn::new(3));

    let rollback_receipt = LocalDispatcher::new(&mut wal)
        .dispatch_rollback(LocalRollbackPlan {
            transaction_id: TransactionId::new(request.invocation_id.get() + 1),
            rollback_payload: b"business-validation-failed".to_vec(),
        })
        .unwrap();

    assert_eq!(
        rollback_receipt.transaction_state,
        TransactionState::RolledBack
    );
    assert_eq!(rollback_receipt.durable_lsn, Lsn::new(5));
    assert_eq!(rollback_receipt.wal_evidence.rollback_lsn, Lsn::new(5));
    // Direct rollback (no cause routing) keeps `intermediate_state` empty so
    // tooling can distinguish it from a Failed/Poisoned rollback.
    assert!(rollback_receipt.intermediate_state.is_none());
    assert_eq!(
        rollback_receipt.cause,
        andromeda_exec::RollbackCause::Direct
    );
}
