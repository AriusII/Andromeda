use andromeda_catalog::{
    CatalogBindingKind, INVENTORY_RESERVE_STOCK_PERMISSION, ProcedureContractRef,
    inventory_reserve_stock_catalog_bindings, inventory_reserve_stock_contract,
};
use andromeda_core::{
    AndromedaError, AndromedaResult, CatalogVersion, ContractHash, InvocationId, PipelineClass,
    ProcedureId, ResourceBudget, TransactionId,
};
use andromeda_exec::{
    AdmissionService, CompletionMappingService, CompletionStatus, ExecutionIoAdmissionDecision,
    ExecutionIoAdmissionRequest, InventoryBusinessMvccStore, InventoryReserveStockExecutor,
    InventoryStock, InvocationContext, InvocationReject, InvocationRequest, InvocationWal,
    LocalDispatchPlan, LocalDispatcher, LocalProcedure, LocalRollbackPlan, LocalVerticalRuntime,
    PreTransactionValidationService, ReserveStockCommand, ResultStreamMetadata,
    ResultValidationService,
};
use andromeda_observe::TraceId;
use andromeda_srpl::{
    Cardinality,
    compiler::{compile_narrow_procedure_signature, inventory_reserve_stock_body_ir},
    model::{SrplBusinessOperationKindIr, SrplPredicateIr, SrplValueIr},
};
use andromeda_storage::{
    CoreIoPlacementRequest, InMemoryWal, Lsn, OperationalProfile, PageSize, StorageIoBudgetScope,
    StorageWorkloadClass, WalRecordKind,
};
use andromeda_tx::{MvccIsolationPolicy, Snapshot, TransactionState, TransactionStatus};

#[derive(Debug, Default)]
struct RecordingWal {
    records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
    durable_lsn: Lsn,
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
struct LaggingFlushWal {
    records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
    durable_lsn: Lsn,
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
struct CommitAppendErrorWal {
    records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
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
struct FlushErrorWal {
    records: Vec<(Lsn, WalRecordKind, Option<TransactionId>, Vec<u8>)>,
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

fn request(expected_contract_hash: ContractHash) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(11),
        procedure: ProcedureContractRef {
            procedure_id: ProcedureId::new(22),
            contract_hash: ContractHash::test_vector(7),
            catalog_version: CatalogVersion::new(3),
        },
        expected_contract_hash,
        catalog_version: CatalogVersion::new(3),
        structured_parameters: Vec::new(),
    }
}

fn procedure(contract: ProcedureContractRef) -> LocalProcedure {
    LocalProcedure {
        contract,
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

fn foreground_io_admission(
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

fn rejected_io_admission(
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

fn inventory_reserve_stock_srpl_source() -> &'static str {
    "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) body { read Inventory.ProductStock Stock one; assert Quantity InsufficientStock; update Inventory.ProductStock AvailableQuantity; emit Reservation (Reserved); }"
}

fn tx_snapshot(
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

#[test]
fn local_vertical_runtime_can_require_io_admission_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let outcome = runtime
        .execute_io_admitted(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(103),
            foreground_io_admission(TraceId::new(104)),
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::Committed)
    );
    assert_eq!(runtime.wal().records.len(), 3);
}

#[test]
fn local_vertical_runtime_blocks_core_io_guarded_execution_when_io_admission_rejects() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute_io_admitted(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(105),
            rejected_io_admission(TraceId::new(106)),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Resource);
    assert!(err.message().contains("execution IO admission rejected"));
    assert!(err.message().contains("memory budget must not be zero"));
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn contract_or_procedure_mismatch_is_rejected_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mismatched_procedure = procedure(ProcedureContractRef {
        procedure_id: ProcedureId::new(99),
        ..request(ContractHash::test_vector(7)).procedure
    });

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &mismatched_procedure,
            TraceId::new(101),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn authorization_denial_is_rejected_before_tx_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mut procedure = procedure(request(ContractHash::test_vector(7)).procedure);
    procedure.required_permissions = vec!["Inventory.ReserveStock.Execute".to_string()];

    let err = runtime
        .execute_authorized(
            request(ContractHash::test_vector(7)),
            &procedure,
            &InvocationContext::new(TraceId::new(102), Vec::new()),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Security);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn result_cardinality_rules_are_enforced_by_result_service() {
    let missing_exact_count = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: None,
        row_count_max: None,
        column_count: 1,
        cardinality: Cardinality::One,
    };
    assert_eq!(
        ResultValidationService::validate_before_payload(missing_exact_count)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    let wrong_exact_count = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: Some(0),
        row_count_max: None,
        column_count: 1,
        cardinality: Cardinality::NonEmptyMany,
    };
    assert_eq!(
        wrong_exact_count
            .validate_before_payload()
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    let unbounded_many = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: None,
        row_count_max: None,
        column_count: 1,
        cardinality: Cardinality::Many,
    };
    assert!(ResultValidationService::validate_before_payload(unbounded_many).is_ok());

    let zero_stream = ResultStreamMetadata {
        stream_id: 0,
        row_count_exact: Some(1),
        row_count_max: Some(1),
        column_count: 1,
        cardinality: Cardinality::One,
    };
    assert_eq!(
        ResultValidationService::validate_before_payload(zero_stream)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    let row_count_mismatch = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: Some(1),
        row_count_max: Some(1),
        column_count: 1,
        cardinality: Cardinality::One,
    };
    assert_eq!(
        row_count_mismatch
            .validate_completed_stream(0)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    // Bounded Many: actual row count exceeding declared row_count_max must
    // be rejected before completion is admitted.
    let bounded_many = ResultStreamMetadata::bounded(1, 1, Cardinality::Many, 2);
    assert!(ResultValidationService::validate_before_payload(bounded_many).is_ok());
    assert_eq!(
        ResultValidationService::validate_completed_stream(bounded_many, 3)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );

    // Inconsistent metadata: declared row_count_max contradicts the
    // intrinsic max of `One`.
    let inconsistent_one = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: Some(1),
        row_count_max: Some(2),
        column_count: 1,
        cardinality: Cardinality::One,
    };
    assert_eq!(
        ResultValidationService::validate_before_payload(inconsistent_one)
            .unwrap_err()
            .kind(),
        andromeda_core::AndromedaErrorKind::Contract
    );
}

#[test]
fn local_vertical_happy_path_commits_only_with_durable_wal_evidence() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let request = InvocationRequest {
        invocation_id: InvocationId::new(700),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
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
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_authorized(
            request,
            &procedure,
            &InvocationContext::new(TraceId::new(7000), contract.required_permissions.clone()),
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::Committed)
    );
    assert_eq!(
        outcome.completion.durable_lsn,
        Some(runtime.wal().durable_lsn())
    );
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
    assert_eq!(runtime.wal().replay_durable().len(), 3);
    assert_eq!(outcome.completion.rows_affected, Some(2));
    assert!(
        effect
            .result_evidence()
            .matches_committed_completion(&outcome.completion)
    );
    assert_eq!(
        runtime.wal().records()[2].header.kind,
        WalRecordKind::TxCommit
    );
}

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
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };

    let outcome = runtime
        .execute_authorized(
            request,
            &procedure,
            &InvocationContext::new(
                TraceId::new(7100),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::Committed)
    );
    assert_eq!(outcome.completion.rows_affected, Some(2));
    assert_eq!(
        outcome.completion.durable_lsn,
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
fn inventory_mvcc_store_exposes_only_committed_business_stock_and_reservations() {
    let mut store = InventoryBusinessMvccStore::new();
    let seed_tx = TransactionId::new(100);
    store
        .seed_committed_stock(
            InventoryStock {
                product_id: 42,
                available_quantity: 10,
                version: 1,
            },
            10,
            seed_tx,
        )
        .unwrap();

    let writer = TransactionId::new(101);
    let reader = TransactionId::new(102);
    let writer_snapshot = tx_snapshot(20, writer, [writer]);
    let decision = store
        .reserve_stock(
            writer,
            21,
            &writer_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 3,
            },
        )
        .unwrap();
    assert!(
        decision
            .evidence
            .proves_reserve_stock_write(&decision.effect)
    );
    assert_eq!(decision.effect.next_stock.available_quantity, 7);

    let reader_snapshot_while_writer_active = tx_snapshot(22, reader, [writer]);
    let visible_before_commit = store
        .read_stock(42, &reader_snapshot_while_writer_active)
        .unwrap()
        .unwrap();
    assert_eq!(visible_before_commit.observed_quantity, 10);
    assert!(
        store
            .read_reservations(42, &reader_snapshot_while_writer_active)
            .unwrap()
            .is_empty()
    );

    store
        .commit_transaction_after_durable_wal(writer, 3)
        .unwrap();
    let reader_snapshot_after_commit = tx_snapshot(30, reader, []);
    let visible_after_commit = store
        .read_stock(42, &reader_snapshot_after_commit)
        .unwrap()
        .unwrap();
    assert_eq!(visible_after_commit.observed_quantity, 7);
    assert_eq!(visible_after_commit.stock_version, 2);
    let reservations = store
        .read_reservations(42, &reader_snapshot_after_commit)
        .unwrap();
    assert_eq!(reservations, vec![decision.reservation]);
}

#[test]
fn inventory_mvcc_store_hides_rolled_back_and_incomplete_business_effects() {
    let mut store = InventoryBusinessMvccStore::new();
    store
        .seed_committed_stock(
            InventoryStock {
                product_id: 42,
                available_quantity: 10,
                version: 1,
            },
            10,
            TransactionId::new(110),
        )
        .unwrap();

    let writer = TransactionId::new(111);
    let writer_snapshot = tx_snapshot(20, writer, [writer]);
    store
        .reserve_stock(
            writer,
            21,
            &writer_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 4,
            },
        )
        .unwrap();
    store
        .rollback_transaction_after_durable_wal(writer, 2)
        .unwrap();

    let reader_snapshot = tx_snapshot(30, TransactionId::new(112), []);
    let visible = store.read_stock(42, &reader_snapshot).unwrap().unwrap();
    assert_eq!(visible.observed_quantity, 10);
    assert_eq!(visible.stock_version, 1);
    assert!(
        store
            .read_reservations(42, &reader_snapshot)
            .unwrap()
            .is_empty()
    );

    let second_writer = TransactionId::new(113);
    let second_snapshot = tx_snapshot(31, second_writer, [second_writer]);
    let second_decision = store
        .reserve_stock(
            second_writer,
            32,
            &second_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 2,
            },
        )
        .unwrap();
    assert_eq!(second_decision.effect.previous_stock.available_quantity, 10);
    store
        .commit_transaction_after_durable_wal(second_writer, 4)
        .unwrap();

    let final_visible = store
        .read_stock(42, &tx_snapshot(40, TransactionId::new(114), []))
        .unwrap()
        .unwrap();
    assert_eq!(final_visible.observed_quantity, 8);
}

#[test]
fn inventory_mvcc_store_rejects_stale_or_concurrent_reservations_with_evidence() {
    let mut store = InventoryBusinessMvccStore::new();
    store
        .seed_committed_stock(
            InventoryStock {
                product_id: 42,
                available_quantity: 10,
                version: 1,
            },
            10,
            TransactionId::new(120),
        )
        .unwrap();

    let first_writer = TransactionId::new(121);
    let stale_writer = TransactionId::new(122);
    let stale_snapshot = tx_snapshot(20, stale_writer, [stale_writer]);
    let first_decision = store
        .reserve_stock(
            first_writer,
            21,
            &tx_snapshot(20, first_writer, [first_writer]),
            ReserveStockCommand {
                product_id: 42,
                quantity: 6,
            },
        )
        .unwrap();
    assert_eq!(first_decision.evidence.read_stock.stock_version, 1);

    let concurrent_err = store
        .reserve_stock(
            stale_writer,
            22,
            &stale_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 1,
            },
        )
        .unwrap_err();
    assert_eq!(
        concurrent_err.kind(),
        andromeda_core::AndromedaErrorKind::Transaction
    );
    assert!(concurrent_err.message().contains("stale"));

    store
        .commit_transaction_after_durable_wal(first_writer, 3)
        .unwrap();
    let stale_err = store
        .reserve_stock(
            stale_writer,
            23,
            &stale_snapshot,
            ReserveStockCommand {
                product_id: 42,
                quantity: 1,
            },
        )
        .unwrap_err();
    assert_eq!(
        stale_err.kind(),
        andromeda_core::AndromedaErrorKind::Transaction
    );
    assert!(stale_err.message().contains("stale"));

    let fresh_writer = TransactionId::new(123);
    let insufficient_err = store
        .reserve_stock(
            fresh_writer,
            31,
            &tx_snapshot(30, fresh_writer, [fresh_writer]),
            ReserveStockCommand {
                product_id: 42,
                quantity: 5,
            },
        )
        .unwrap_err();
    assert_eq!(
        insufficient_err.kind(),
        andromeda_core::AndromedaErrorKind::Execution
    );
    assert!(
        insufficient_err
            .message()
            .contains("insufficient inventory stock")
    );
}

#[test]
fn local_vertical_rejects_visibility_when_flush_does_not_cover_commit_lsn() {
    let mut runtime = LocalVerticalRuntime::new(LaggingFlushWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(8000),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Storage);
    assert_eq!(runtime.wal().records.len(), 3);
    assert_eq!(runtime.wal().records[2].1, WalRecordKind::TxCommit);
    assert!(runtime.wal().durable_lsn < runtime.wal().records[2].0);
}

#[test]
fn local_vertical_commit_append_failure_does_not_publish_terminal_status() {
    let mut runtime = LocalVerticalRuntime::new(CommitAppendErrorWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(8001),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Storage);
    assert_eq!(runtime.wal().records.len(), 2);
    assert_eq!(runtime.wal().records[0].1, WalRecordKind::TxBegin);
    assert_eq!(runtime.wal().records[1].1, WalRecordKind::RowUpdate);
    assert_eq!(
        runtime.transactions().status(TransactionId::new(1)),
        Some(TransactionStatus::InFlight),
        "commit append failure occurs before request_commit/commit_durable, so status must not become Committed"
    );
    assert_eq!(
        runtime
            .transactions()
            .snapshot(TransactionId::new(1))
            .unwrap()
            .state_machine
            .state,
        TransactionState::Active
    );
}

#[test]
fn local_vertical_flush_error_does_not_publish_terminal_status() {
    let mut runtime = LocalVerticalRuntime::new(FlushErrorWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .execute(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(8002),
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Storage);
    assert_eq!(runtime.wal().records.len(), 3);
    assert_eq!(runtime.wal().records[2].1, WalRecordKind::TxCommit);
    assert_eq!(
        runtime.transactions().status(TransactionId::new(1)),
        Some(TransactionStatus::InFlight),
        "flush failure occurs before commit_durable, so visible commit status must not be published"
    );
    assert_eq!(
        runtime
            .transactions()
            .snapshot(TransactionId::new(1))
            .unwrap()
            .state_machine
            .state,
        TransactionState::Active
    );
}

#[test]
fn local_dispatcher_rolls_back_only_with_durable_wal_evidence() {
    let mut wal = RecordingWal::default();
    let receipt = LocalDispatcher::new(&mut wal)
        .dispatch_rollback(LocalRollbackPlan {
            transaction_id: TransactionId::new(900),
            rollback_payload: b"business-validation-failed".to_vec(),
        })
        .unwrap();

    assert_eq!(receipt.transaction_state, TransactionState::RolledBack);
    assert_eq!(receipt.durable_lsn, Lsn::new(2));
    assert_eq!(receipt.wal_evidence.begin_lsn, Lsn::new(1));
    assert_eq!(receipt.wal_evidence.rollback_lsn, Lsn::new(2));
    assert_eq!(receipt.wal_evidence.durable_lsn, Lsn::new(2));
    assert_eq!(wal.records.len(), 2);
    assert_eq!(wal.records[0].1, WalRecordKind::TxBegin);
    assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
    assert_eq!(wal.durable_lsn, wal.records[1].0);
}

#[test]
fn local_dispatcher_rejects_rolled_back_completion_when_flush_lags_rollback_lsn() {
    let mut wal = LaggingFlushWal::default();

    let err = LocalDispatcher::new(&mut wal)
        .dispatch_rollback(LocalRollbackPlan {
            transaction_id: TransactionId::new(901),
            rollback_payload: b"business-validation-failed".to_vec(),
        })
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Storage);
    assert_eq!(wal.records.len(), 2);
    assert_eq!(wal.records[1].1, WalRecordKind::TxRollback);
    assert!(wal.durable_lsn < wal.records[1].0);
}

#[test]
fn local_vertical_runtime_rolls_back_business_validation_failure_after_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let outcome = runtime
        .rollback_business_validation_failure_after_begin(
            request(ContractHash::test_vector(7)),
            &procedure,
            TraceId::new(8100),
            "insufficient inventory stock for reservation",
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected, Some(0));
    assert_eq!(outcome.completion.durable_lsn, Some(Lsn::new(2)));
    assert_eq!(runtime.wal().records.len(), 2);
    assert_eq!(runtime.wal().records[0].1, WalRecordKind::TxBegin);
    assert_eq!(runtime.wal().records[1].1, WalRecordKind::TxRollback);
    assert!(
        runtime.wal().records[1]
            .3
            .starts_with(b"andromeda.exec.business-validation-failed.v1\0")
    );
    assert_eq!(runtime.wal().durable_lsn, runtime.wal().records[1].0);
}

#[test]
fn inventory_reserve_stock_business_failure_rolls_back_after_authorized_begin_without_commit() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let valid_effect = InventoryReserveStockExecutor::reserve(
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
    let procedure = valid_effect.to_local_procedure(&contract).unwrap();

    let rejected_command = ReserveStockCommand {
        product_id: 42,
        quantity: 11,
    };
    let observed_stock = InventoryStock {
        product_id: 42,
        available_quantity: 10,
        version: 7,
    };
    let error =
        InventoryReserveStockExecutor::reserve(rejected_command, observed_stock).unwrap_err();
    assert_eq!(error.kind(), andromeda_core::AndromedaErrorKind::Execution);
    assert!(error.message().contains("insufficient inventory stock"));

    let rejection_evidence = InventoryReserveStockExecutor::rejection_evidence(
        rejected_command,
        observed_stock,
        error.message(),
    );
    assert!(rejection_evidence.has_business_rule_evidence());
    assert!(
        rejection_evidence
            .decision_trace(TraceId::new(8200))
            .has_explanation()
    );

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let request = InvocationRequest {
        invocation_id: InvocationId::new(820),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };

    let outcome = runtime
        .rollback_authorized_business_validation_failure_after_begin(
            request,
            &procedure,
            &InvocationContext::new(
                TraceId::new(8200),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
            rejection_evidence.reason.as_str(),
        )
        .unwrap();

    assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected, Some(0));
    assert_eq!(
        outcome.completion.durable_lsn,
        Some(runtime.wal().durable_lsn())
    );
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(2));
    assert_eq!(runtime.wal().records().len(), 2);
    assert_eq!(
        runtime.wal().records()[0].header.kind,
        WalRecordKind::TxBegin
    );
    assert_eq!(
        runtime.wal().records()[1].header.kind,
        WalRecordKind::TxRollback
    );
    assert!(
        runtime.wal().records()[1]
            .payload
            .starts_with(b"andromeda.exec.business-validation-failed.v1\0")
    );
    assert!(
        std::str::from_utf8(&runtime.wal().records()[1].payload)
            .unwrap()
            .contains("insufficient inventory stock")
    );
    assert!(
        !runtime
            .wal()
            .records()
            .iter()
            .any(|record| record.header.kind == WalRecordKind::TxCommit)
    );
    assert!(outcome.authorization_trace.is_some());
}

#[test]
fn completion_mapping_rejects_commit_or_rollback_outcome_mismatches() {
    assert_eq!(
        CompletionMappingService::committed(
            InvocationId::new(8200),
            2,
            TransactionState::RolledBack,
            Lsn::new(10),
            TraceId::new(8200),
        )
        .unwrap_err()
        .kind(),
        andromeda_core::AndromedaErrorKind::Transaction
    );

    assert_eq!(
        CompletionMappingService::committed(
            InvocationId::new(8201),
            2,
            TransactionState::Committed,
            Lsn::ZERO,
            TraceId::new(8201),
        )
        .unwrap_err()
        .kind(),
        andromeda_core::AndromedaErrorKind::Storage
    );

    assert_eq!(
        CompletionMappingService::rolled_back(
            InvocationId::new(8202),
            TransactionState::Committed,
            Lsn::new(11),
            TraceId::new(8202),
        )
        .unwrap_err()
        .kind(),
        andromeda_core::AndromedaErrorKind::Transaction
    );

    let rolled_back = CompletionMappingService::rolled_back(
        InvocationId::new(8203),
        TransactionState::RolledBack,
        Lsn::new(12),
        TraceId::new(8203),
    )
    .unwrap();
    assert_eq!(rolled_back.status, CompletionStatus::RolledBack);
    assert_eq!(rolled_back.rows_affected, Some(0));
    assert_eq!(
        rolled_back.transaction_state,
        Some(TransactionState::RolledBack)
    );
}

#[test]
fn local_vertical_runtime_preserves_contract_check_before_rollback_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let procedure = procedure(request(ContractHash::test_vector(7)).procedure);

    let err = runtime
        .rollback_business_validation_failure_after_begin(
            request(ContractHash::test_vector(8)),
            &procedure,
            TraceId::new(8101),
            "insufficient inventory stock for reservation",
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn local_vertical_runtime_preserves_authorization_check_before_rollback_begin() {
    let mut runtime = LocalVerticalRuntime::new(RecordingWal::default());
    let mut procedure = procedure(request(ContractHash::test_vector(7)).procedure);
    procedure.required_permissions = vec!["Inventory.ReserveStock.Execute".to_string()];

    let err = runtime
        .rollback_authorized_business_validation_failure_after_begin(
            request(ContractHash::test_vector(7)),
            &procedure,
            &InvocationContext::new(TraceId::new(8102), Vec::new()),
            "insufficient inventory stock for reservation",
        )
        .unwrap_err();

    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Security);
    assert!(runtime.wal().records.is_empty());
}

#[test]
fn service_and_dispatcher_api_is_usable_externally() {
    let request = request(ContractHash::test_vector(7));
    let trace = PreTransactionValidationService::validate_invocation_contract(
        &request,
        request.procedure,
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

#[test]
fn poison_rollback_routes_through_poisoned_state_with_durable_evidence() {
    use andromeda_exec::RollbackCause;

    let contract = inventory_reserve_stock_contract().unwrap();
    let valid_effect = InventoryReserveStockExecutor::reserve(
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
    let procedure = valid_effect.to_local_procedure(&contract).unwrap();

    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let request = InvocationRequest {
        invocation_id: InvocationId::new(821),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };

    let outcome = runtime
        .rollback_authorized_poison_after_begin(
            request,
            &procedure,
            &InvocationContext::new(
                TraceId::new(8210),
                vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
            ),
            "post-begin runtime witness contradicted MVCC snapshot",
        )
        .unwrap();

    // Visible completion: RolledBack only after durable rollback evidence.
    assert_eq!(outcome.completion.status, CompletionStatus::RolledBack);
    assert_eq!(
        outcome.completion.transaction_state,
        Some(TransactionState::RolledBack)
    );
    assert_eq!(outcome.completion.rows_affected, Some(0));
    assert_eq!(
        outcome.completion.durable_lsn,
        Some(runtime.wal().durable_lsn())
    );

    // WAL evidence: TxBegin then TxRollback only (never TxCommit).
    assert_eq!(runtime.wal().records().len(), 2);
    assert_eq!(
        runtime.wal().records()[0].header.kind,
        WalRecordKind::TxBegin
    );
    assert_eq!(
        runtime.wal().records()[1].header.kind,
        WalRecordKind::TxRollback
    );
    assert!(
        !runtime
            .wal()
            .records()
            .iter()
            .any(|record| record.header.kind == WalRecordKind::TxCommit)
    );
    // Rollback payload uses the poisoned-rollback domain tag, distinct from
    // ordinary business validation failure rollback payloads.
    assert!(
        runtime.wal().records()[1]
            .payload
            .starts_with(b"andromeda.exec.poisoned-rollback.v1\0")
    );
    assert!(outcome.authorization_trace.is_some());

    // Dispatcher level: poison cause must record the Poisoned intermediate
    // state on the receipt so audit/recovery can prove the routing.
    let mut wal = RecordingWal::default();
    let receipt = LocalDispatcher::new(&mut wal)
        .dispatch_rollback_with_cause(
            LocalRollbackPlan {
                transaction_id: TransactionId::new(0xCAFEBABE),
                rollback_payload: b"engine invariant breach".to_vec(),
            },
            RollbackCause::Poison,
        )
        .unwrap();
    assert_eq!(receipt.cause, RollbackCause::Poison);
    assert_eq!(receipt.intermediate_state, Some(TransactionState::Poisoned));
    assert_eq!(receipt.transaction_state, TransactionState::RolledBack);
    assert!(receipt.durable_lsn >= receipt.wal_evidence.rollback_lsn);
}
