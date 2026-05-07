use andromeda_catalog::{
    CatalogSnapshot, INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID, ProcedureContract,
    inventory_domain_definition_batch, inventory_reserve_stock_contract,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, ContractHash, InvocationId, RequestId,
    SessionId, TransactionId,
};
use andromeda_exec::{
    CompletionStatus, HeapInventoryProductStockStore, InventoryProductStockCommitEvidence,
    InventoryProductStockDurableRedoEvidence, InventoryProductStockReservationIntent,
    InventoryProductStockStore, InventoryStock, InvocationContext, InvocationRequest,
    InvocationWal, LocalHeapRowInsertRedoTemplate, ObservedInventoryProductStockStore,
    V0InventoryRecoverableRuntime, V0InventoryReserveStockRpcPayload,
    encode_inventory_reserve_stock_v0_execute_frame, inventory_reserve_stock_v0_pdf_srpl_source,
};
use andromeda_observe::{
    CriticalDecisionKind, EventEmitter, EventEnvelope, EventSink, InMemoryEventSink, TraceEvent,
    TraceId,
};
use andromeda_quic::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameCodec, FrameHeader, FrameType, StreamRole,
    validate_result_stream_sequence, validate_single_frame_on_stream,
};
use andromeda_storage::{
    DatabaseManifest, FileWal, InMemoryWal, Lsn, PageId, PageSize, ProductStockRow,
    RedoRecordDecision, StartupMode, WalRecordKind, recover_from_file_wal,
    write_ahead_log::HeapRowRedoPayloadV1,
};

fn inventory_catalog_snapshot() -> CatalogSnapshot {
    let batch = inventory_domain_definition_batch().unwrap();
    let plan = batch.dry_run().unwrap();
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot.apply_mutation_plan(&plan.mutation_plan).unwrap();
    snapshot
}

fn request(contract: &ProcedureContract, invocation_id: u64) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(invocation_id),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    }
}

fn context(contract: &ProcedureContract, trace_id: u64) -> InvocationContext {
    InvocationContext::new(
        TraceId::new(trace_id.into()),
        contract.required_permissions.clone(),
    )
}

fn stock() -> InventoryStock {
    InventoryStock {
        product_id: 42,
        available_quantity: 10,
        version: 7,
    }
}

fn encoded_execute_frame() -> Vec<u8> {
    encode_inventory_reserve_stock_v0_execute_frame(
        RequestId::new(900),
        SessionId::new(901),
        V0InventoryReserveStockRpcPayload::new(42, 3).unwrap(),
    )
    .unwrap()
}

fn encoded_execute_frame_with_transaction_id() -> Vec<u8> {
    let payload = V0InventoryReserveStockRpcPayload::new(42, 3)
        .unwrap()
        .encode()
        .unwrap();
    FrameCodec::encode(&FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcExecuteRequest,
            request_id: RequestId::new(902),
            session_id: SessionId::new(903),
            tx_id: Some(TransactionId::new(904)),
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    })
    .unwrap()
}

#[derive(Debug)]
struct CountingObservedProductStockStore {
    inner: ObservedInventoryProductStockStore,
    prepare_count: usize,
    publish_count: usize,
    abort_count: usize,
}

impl CountingObservedProductStockStore {
    fn new(stock: InventoryStock) -> Self {
        Self {
            inner: ObservedInventoryProductStockStore::new(stock).unwrap(),
            prepare_count: 0,
            publish_count: 0,
            abort_count: 0,
        }
    }
}

impl InventoryProductStockStore for CountingObservedProductStockStore {
    fn prepare_reserve_stock(
        &mut self,
        command: andromeda_exec::ReserveStockCommand,
    ) -> AndromedaResult<InventoryProductStockReservationIntent> {
        self.prepare_count += 1;
        self.inner.prepare_reserve_stock(command)
    }

    fn publish_committed_reserve_stock(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        commit: InventoryProductStockCommitEvidence,
    ) -> AndromedaResult<()> {
        self.publish_count += 1;
        self.inner.publish_committed_reserve_stock(intent, commit)
    }

    fn abort_prepared_reserve_stock(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        reason: &str,
    ) -> AndromedaResult<()> {
        self.abort_count += 1;
        self.inner.abort_prepared_reserve_stock(intent, reason)
    }
}

fn assert_product_stock_untouched(product_stock: &CountingObservedProductStockStore) {
    assert_eq!(product_stock.prepare_count, 0);
    assert_eq!(product_stock.publish_count, 0);
    assert_eq!(product_stock.abort_count, 0);
    assert_eq!(product_stock.inner.visible_stock(), stock());
    assert!(product_stock.inner.prepared_intent().is_none());
    assert!(product_stock.inner.published_commit().is_none());
}

#[test]
fn v0_inventory_executes_from_bound_pdf_style_srpl_and_emits_ordered_result_frames() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_encoded_inventory_reserve_stock(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 700),
            &context(&contract, 7000),
            stock(),
        )
        .unwrap();

    assert_eq!(
        outcome.vertical.completion.status(),
        CompletionStatus::Committed
    );
    assert_eq!(outcome.vertical.completion.rows_affected(), Some(2));
    assert_eq!(outcome.vertical.completion.durable_lsn(), Some(Lsn::new(3)));
    assert_eq!(
        outcome.srpl_plan.evidence.procedure_contract,
        contract.as_ref()
    );
    assert_eq!(outcome.effect.result.remaining_quantity, 7);
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
    assert_eq!(runtime.wal().replay_durable().len(), 3);

    assert_eq!(outcome.result_frames.len(), 3);
    assert_eq!(
        outcome.result_frames[0].header.frame_type,
        FrameType::RpcMetadata
    );
    assert_eq!(
        outcome.result_frames[1].header.frame_type,
        FrameType::RpcBatch
    );
    assert_eq!(
        outcome.result_frames[2].header.frame_type,
        FrameType::RpcCompletion
    );
    validate_result_stream_sequence(&outcome.result_frames).unwrap();
    for frame in &outcome.result_frames {
        validate_single_frame_on_stream(frame, StreamRole::ResultUnidirectional).unwrap();
    }
}

#[test]
fn v0_inventory_rejects_malformed_execute_frame_before_product_stock_or_wal() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = CountingObservedProductStockStore::new(stock());

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            b"short frame",
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 715),
            &context(&contract, 7015),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(err.message().contains("AE-V0-RSVSTK-FRAME-LEN"));
    assert!(runtime.wal().is_empty());
    assert_product_stock_untouched(&product_stock);
}

#[test]
fn v0_inventory_rejects_invalid_payload_domain_before_product_stock_or_wal() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = CountingObservedProductStockStore::new(stock());
    let mut encoded = encoded_execute_frame();
    encoded[FrameCodec::HEADER_LEN] ^= 0x01;

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded,
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 716),
            &context(&contract, 7016),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(err.message().contains("AE-V0-RSVSTK-PAYLOAD-DOMAIN"));
    assert!(runtime.wal().is_empty());
    assert_product_stock_untouched(&product_stock);
}

#[test]
fn v0_inventory_rejects_transaction_bearing_execute_frame_before_product_stock_or_wal() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = CountingObservedProductStockStore::new(stock());

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded_execute_frame_with_transaction_id(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 717),
            &context(&contract, 7017),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(err.message().contains("AE-V0-RSVSTK-FRAME-TXID"));
    assert!(runtime.wal().is_empty());
    assert_product_stock_untouched(&product_stock);
}

#[test]
fn v0_inventory_product_stock_adapter_publishes_only_after_durable_commit() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = ObservedInventoryProductStockStore::new(stock()).unwrap();

    let outcome = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 711),
            &context(&contract, 7011),
            &mut product_stock,
        )
        .unwrap();

    assert_eq!(
        outcome.vertical.completion.status(),
        CompletionStatus::Committed
    );
    assert_eq!(
        outcome.product_stock_commit.durable_commit_lsn,
        outcome.vertical.completion.durable_lsn().unwrap()
    );
    assert_eq!(
        outcome.product_stock_commit.transaction_id,
        outcome.vertical.transaction_id
    );
    assert_eq!(product_stock.visible_stock(), outcome.effect.next_stock);
    assert!(product_stock.prepared_intent().is_none());
    assert_eq!(
        product_stock.published_commit(),
        Some(outcome.product_stock_commit)
    );
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
        outcome.effect.mutation_payload()
    );
}

#[test]
fn v0_inventory_heap_product_stock_store_publishes_only_after_durable_commit() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = HeapInventoryProductStockStore::from_cold_snapshot(
        PageId::new(42_101),
        PageSize::KiB16,
        stock(),
    )
    .unwrap();

    let outcome = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 714),
            &context(&contract, 7014),
            &mut product_stock,
        )
        .unwrap();

    assert_eq!(
        outcome.product_stock_commit.transaction_id,
        outcome.vertical.transaction_id
    );
    assert_eq!(
        outcome.product_stock_commit.durable_commit_lsn,
        outcome.vertical.completion.durable_lsn().unwrap()
    );
    assert_eq!(
        product_stock.visible_stock().unwrap(),
        outcome.effect.next_stock
    );
    assert_eq!(
        product_stock.visible_product_stock_row().unwrap(),
        ProductStockRow::new(42, 7).unwrap()
    );
    assert_eq!(product_stock.active_heap_slot_count(), 2);
    assert!(product_stock.prepared_intent().is_none());
    assert_eq!(
        product_stock.published_commit(),
        Some(outcome.product_stock_commit)
    );
    let redo_evidence = outcome
        .product_stock_redo
        .as_ref()
        .expect("heap-backed ProductStock must expose durable redo evidence");
    assert_eq!(
        redo_evidence.transaction_id,
        outcome.vertical.transaction_id
    );
    assert_eq!(redo_evidence.redo_record_lsn, Lsn::new(2));
    assert_eq!(
        redo_evidence.durable_commit_lsn,
        outcome.product_stock_commit.durable_commit_lsn
    );
    assert_eq!(product_stock.page_lsn(), redo_evidence.redo_record_lsn);

    let insert = product_stock.last_committed_insert().unwrap();
    assert_eq!(insert.slot_id(), 1);
    assert_eq!(insert.row(), ProductStockRow::new(42, 7).unwrap());

    let redo = product_stock.last_redo_payload().unwrap();
    assert_eq!(redo.expected_previous_page_lsn(), Lsn::ZERO);
    assert_eq!(redo.resulting_page_lsn(), redo_evidence.redo_record_lsn);
    assert_eq!(redo.after_slot_id(), insert.slot_id());
    assert_eq!(ProductStockRow::decode(redo.tuple()).unwrap(), insert.row());
    assert_eq!(redo, &redo_evidence.redo_payload);
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
    assert_eq!(wal_redo, redo.clone());
}

#[test]
fn v0_inventory_product_stock_adapter_aborts_prepared_heap_state_when_commit_evidence_is_absent() {
    #[derive(Debug, Default)]
    struct CommitAppendFailWal {
        records: Vec<(WalRecordKind, Option<TransactionId>, Vec<u8>)>,
    }

    impl InvocationWal for CommitAppendFailWal {
        fn append(
            &mut self,
            kind: WalRecordKind,
            transaction_id: Option<TransactionId>,
            payload: &[u8],
        ) -> AndromedaResult<Lsn> {
            if kind == WalRecordKind::TxCommit {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "injected commit append failure before ProductStock publication",
                ));
            }

            let lsn = Lsn::new(self.records.len() as u64 + 1);
            self.records.push((kind, transaction_id, payload.to_vec()));
            Ok(lsn)
        }

        fn flush_through(&mut self, _lsn: Lsn) -> AndromedaResult<Lsn> {
            unreachable!("commit append failure must prevent durable commit evidence")
        }
    }

    #[derive(Debug)]
    struct CountingProductStockStore {
        inner: HeapInventoryProductStockStore,
        prepare_count: usize,
        publish_count: usize,
        abort_count: usize,
    }

    impl CountingProductStockStore {
        fn new(stock: InventoryStock) -> Self {
            Self {
                inner: HeapInventoryProductStockStore::from_cold_snapshot(
                    PageId::new(42_102),
                    PageSize::KiB16,
                    stock,
                )
                .unwrap(),
                prepare_count: 0,
                publish_count: 0,
                abort_count: 0,
            }
        }
    }

    impl InventoryProductStockStore for CountingProductStockStore {
        fn prepare_reserve_stock(
            &mut self,
            command: andromeda_exec::ReserveStockCommand,
        ) -> AndromedaResult<InventoryProductStockReservationIntent> {
            self.prepare_count += 1;
            self.inner.prepare_reserve_stock(command)
        }

        fn publish_committed_reserve_stock(
            &mut self,
            intent: &InventoryProductStockReservationIntent,
            commit: InventoryProductStockCommitEvidence,
        ) -> AndromedaResult<()> {
            self.publish_count += 1;
            self.inner.publish_committed_reserve_stock(intent, commit)
        }

        fn prepared_reserve_stock_redo_template(
            &self,
            intent: &InventoryProductStockReservationIntent,
        ) -> AndromedaResult<Option<LocalHeapRowInsertRedoTemplate>> {
            self.inner.prepared_reserve_stock_redo_template(intent)
        }

        fn publish_committed_reserve_stock_with_redo(
            &mut self,
            intent: &InventoryProductStockReservationIntent,
            commit: InventoryProductStockCommitEvidence,
            redo: InventoryProductStockDurableRedoEvidence,
        ) -> AndromedaResult<()> {
            self.publish_count += 1;
            self.inner
                .publish_committed_reserve_stock_with_redo(intent, commit, redo)
        }

        fn abort_prepared_reserve_stock(
            &mut self,
            intent: &InventoryProductStockReservationIntent,
            reason: &str,
        ) -> AndromedaResult<()> {
            self.abort_count += 1;
            self.inner.abort_prepared_reserve_stock(intent, reason)
        }
    }

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(CommitAppendFailWal::default());
    let mut product_stock = CountingProductStockStore::new(stock());

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 713),
            &context(&contract, 7013),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("commit append failure"));
    assert_eq!(product_stock.prepare_count, 1);
    assert_eq!(product_stock.publish_count, 0);
    assert_eq!(product_stock.abort_count, 1);
    assert_eq!(product_stock.inner.visible_stock().unwrap(), stock());
    assert_eq!(product_stock.inner.active_heap_slot_count(), 1);
    assert!(product_stock.inner.prepared_intent().is_none());
    assert!(product_stock.inner.published_commit().is_none());
    assert!(product_stock.inner.last_committed_insert().is_none());
    assert!(product_stock.inner.last_redo_payload().is_none());
    assert_eq!(product_stock.inner.page_lsn(), Lsn::ZERO);
    assert_eq!(
        runtime
            .wal()
            .records
            .iter()
            .map(|(kind, _, _)| *kind)
            .collect::<Vec<_>>(),
        vec![WalRecordKind::TxBegin, WalRecordKind::RowInsert]
    );
}

#[test]
fn v0_inventory_product_stock_adapter_is_not_touched_for_contract_rejection() {
    #[derive(Debug)]
    struct CountingProductStockStore {
        inner: ObservedInventoryProductStockStore,
        prepare_count: usize,
        publish_count: usize,
        abort_count: usize,
    }

    impl CountingProductStockStore {
        fn new(stock: InventoryStock) -> Self {
            Self {
                inner: ObservedInventoryProductStockStore::new(stock).unwrap(),
                prepare_count: 0,
                publish_count: 0,
                abort_count: 0,
            }
        }
    }

    impl InventoryProductStockStore for CountingProductStockStore {
        fn prepare_reserve_stock(
            &mut self,
            command: andromeda_exec::ReserveStockCommand,
        ) -> AndromedaResult<InventoryProductStockReservationIntent> {
            self.prepare_count += 1;
            self.inner.prepare_reserve_stock(command)
        }

        fn publish_committed_reserve_stock(
            &mut self,
            intent: &InventoryProductStockReservationIntent,
            commit: InventoryProductStockCommitEvidence,
        ) -> AndromedaResult<()> {
            self.publish_count += 1;
            self.inner.publish_committed_reserve_stock(intent, commit)
        }

        fn abort_prepared_reserve_stock(
            &mut self,
            intent: &InventoryProductStockReservationIntent,
            reason: &str,
        ) -> AndromedaResult<()> {
            self.abort_count += 1;
            self.inner.abort_prepared_reserve_stock(intent, reason)
        }
    }

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut stale_request = request(&contract, 712);
    stale_request.expected_contract_hash = ContractHash::test_vector(0xBA);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = CountingProductStockStore::new(stock());

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            stale_request,
            &context(&contract, 7012),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());
    assert_eq!(product_stock.prepare_count, 0);
    assert_eq!(product_stock.publish_count, 0);
    assert_eq!(product_stock.abort_count, 0);
    assert_eq!(product_stock.inner.visible_stock(), stock());
}

#[test]
fn v0_inventory_rejects_contract_mismatch_before_wal_append() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut stale_request = request(&contract, 701);
    stale_request.expected_contract_hash = ContractHash::test_vector(0xBA);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());

    let err = runtime
        .execute_encoded_inventory_reserve_stock(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            stale_request,
            &context(&contract, 7001),
            stock(),
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());
}

#[test]
fn v0_inventory_rejects_missing_permission_before_wal_append() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());

    let err = runtime
        .execute_encoded_inventory_reserve_stock(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 702),
            &InvocationContext::new(TraceId::new(7002), Vec::new()),
            stock(),
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(runtime.wal().is_empty());
}

#[test]
fn v0_inventory_file_wal_recovers_only_committed_redo_after_sync() {
    let path = std::env::temp_dir().join(format!(
        "andromeda-v0-exec-{}-{}.wal",
        std::process::id(),
        704
    ));
    std::fs::remove_file(&path).ok();

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    {
        let wal = FileWal::open(&path).unwrap();
        let mut runtime = V0InventoryRecoverableRuntime::new(wal);
        runtime
            .execute_encoded_inventory_reserve_stock(
                &encoded_execute_frame(),
                inventory_reserve_stock_v0_pdf_srpl_source(),
                &catalog,
                &contract,
                request(&contract, 704),
                &context(&contract, 7004),
                stock(),
            )
            .unwrap();
    }

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 1,
    };
    let redo = recover_from_file_wal(&manifest, StartupMode::SafeStart, &path).unwrap();
    let replay = redo.committed_replay_lsns().collect::<Vec<_>>();

    assert_eq!(redo.durable_lsn, Lsn::new(3));
    assert_eq!(replay, vec![Lsn::new(2)]);
    assert_eq!(
        redo.records
            .iter()
            .find(|record| record.lsn == Lsn::new(1))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipNonRedoRecord
    );
    assert_eq!(
        redo.records
            .iter()
            .find(|record| record.lsn == Lsn::new(3))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipNonRedoRecord
    );

    std::fs::remove_file(&path).ok();
}

#[test]
fn v0_inventory_observed_path_emits_required_audit_envelopes() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();

    runtime
        .execute_encoded_inventory_reserve_stock_observed(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 705),
            &context(&contract, 7005),
            stock(),
            &mut sink,
        )
        .unwrap();

    assert_eq!(sink.events().len(), 4);
    assert!(sink.events()[0].correlation.has_contract_catalog());
    assert!(sink.events()[0].correlation.has_no_transaction_evidence());
    assert_eq!(sink.events()[0].event_id.get(), 1);
    assert_eq!(sink.events()[3].event_id.get(), 4);
    assert!(matches!(
        &sink.events()[0].event,
        TraceEvent::Decision(decision)
            if decision.decision == CriticalDecisionKind::ResourceGovernance
    ));
    assert!(matches!(
        &sink.events()[1].event,
        TraceEvent::Decision(decision)
            if decision.decision == CriticalDecisionKind::ContractValidation
    ));
    assert!(matches!(
        &sink.events()[2].event,
        TraceEvent::Decision(decision)
            if decision.decision == CriticalDecisionKind::SecurityAuthorization
    ));
    assert_eq!(sink.events()[3].correlation.durable_lsn, Some(3));
}

#[test]
fn v0_inventory_observed_contract_rejection_emits_pre_transaction_evidence() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut stale_request = request(&contract, 707);
    stale_request.expected_contract_hash = ContractHash::test_vector(0xBA);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            stale_request,
            &context(&contract, 7007),
            stock(),
            &mut sink,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());
    assert_eq!(sink.events().len(), 2);
    assert!(
        sink.events()
            .iter()
            .all(|event| event.correlation.has_no_transaction_evidence())
    );
    assert!(
        sink.events()
            .iter()
            .all(|event| event.correlation.has_request_session())
    );
    assert!(matches!(
        &sink.events()[0].event,
        TraceEvent::ContractRejected(trace)
            if trace.has_reason() && trace.has_contract_evidence()
    ));
    assert!(matches!(
        &sink.events()[1].event,
        TraceEvent::ExecutionTransition(trace)
            if trace.reason_code == andromeda_observe::TransitionReasonCode::PRE_TRANSACTION_REJECTION
                && trace.transaction_id.is_none()
                && trace.durable_lsn.is_none()
    ));
}

#[test]
fn v0_inventory_observed_authorization_denial_emits_pre_transaction_evidence() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 708),
            &InvocationContext::new(TraceId::new(7008), Vec::new()),
            stock(),
            &mut sink,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(runtime.wal().is_empty());
    assert_eq!(sink.events().len(), 2);
    assert!(
        sink.events()
            .iter()
            .all(|event| event.correlation.has_no_transaction_evidence())
    );
    assert!(matches!(
        &sink.events()[0].event,
        TraceEvent::AuthorizationDenied(trace)
            if trace.has_reason()
                && trace.has_permission_evidence()
                && trace.denied_permission == "Inventory.ReserveStock.Execute"
    ));
    assert!(matches!(
        &sink.events()[1].event,
        TraceEvent::ExecutionTransition(trace)
            if trace.reason_code == andromeda_observe::TransitionReasonCode::PERMISSION_DENIED
                && trace.transaction_id.is_none()
                && trace.durable_lsn.is_none()
    ));
}

#[test]
fn v0_inventory_observed_admission_rejection_emits_pre_transaction_evidence() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut zero_invocation = request(&contract, 709);
    zero_invocation.invocation_id = InvocationId::new(0);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            zero_invocation,
            &context(&contract, 7009),
            stock(),
            &mut sink,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());
    assert_eq!(sink.events().len(), 1);
    assert!(sink.events()[0].correlation.has_no_transaction_evidence());
    assert!(matches!(
        &sink.events()[0].event,
        TraceEvent::ContractRejected(trace)
            if trace.has_reason()
                && trace.has_contract_evidence()
                && trace.reason.contains("InvocationId")
    ));
}

#[test]
fn v0_inventory_observed_path_surfaces_and_counts_emitter_failure() {
    #[derive(Debug)]
    struct FailingAfterAccepted {
        accepted_before_failure: usize,
        attempts: usize,
    }

    impl EventSink for FailingAfterAccepted {
        fn emit(&mut self, _event: EventEnvelope) -> AndromedaResult<()> {
            self.attempts += 1;
            if self.attempts > self.accepted_before_failure {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Internal,
                    "injected observe sink failure",
                ));
            }
            Ok(())
        }
    }

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut emitter = EventEmitter::new(FailingAfterAccepted {
        accepted_before_failure: 2,
        attempts: 0,
    });

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed_with_emitter(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            request(&contract, 706),
            &context(&contract, 7006),
            stock(),
            &mut emitter,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Internal);
    assert!(err.message().contains("injected observe sink failure"));
    assert_eq!(emitter.accepted_count(), 2);
    assert_eq!(emitter.rejected_count(), 1);
    assert_eq!(emitter.sink().attempts, 3);
    assert_eq!(
        emitter.peek_next_event_id().map(|event_id| event_id.get()),
        Some(3)
    );
    assert_eq!(runtime.wal().replay_durable().len(), 3);
}

#[test]
fn v0_inventory_observed_rejection_surfaces_and_counts_emitter_failure_without_wal() {
    #[derive(Debug)]
    struct FailingImmediately {
        attempts: usize,
    }

    impl EventSink for FailingImmediately {
        fn emit(&mut self, _event: EventEnvelope) -> AndromedaResult<()> {
            self.attempts += 1;
            Err(AndromedaError::new(
                AndromedaErrorKind::Internal,
                "injected pre-transaction observe sink failure",
            ))
        }
    }

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let mut stale_request = request(&contract, 710);
    stale_request.expected_contract_hash = ContractHash::test_vector(0xBA);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut emitter = EventEmitter::new(FailingImmediately { attempts: 0 });

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed_with_emitter(
            &encoded_execute_frame(),
            inventory_reserve_stock_v0_pdf_srpl_source(),
            &catalog,
            &contract,
            stale_request,
            &context(&contract, 7010),
            stock(),
            &mut emitter,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Internal);
    assert!(
        err.message()
            .contains("injected pre-transaction observe sink failure")
    );
    assert_eq!(emitter.accepted_count(), 0);
    assert_eq!(emitter.rejected_count(), 1);
    assert_eq!(emitter.sink().attempts, 1);
    assert!(runtime.wal().is_empty());
}
