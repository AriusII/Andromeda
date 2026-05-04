use andromeda_catalog::{
    inventory_domain_definition_batch, inventory_reserve_stock_contract, CatalogSnapshot,
    ProcedureContract, INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID,
};
use andromeda_core::{AndromedaErrorKind, ContractHash, InvocationId, RequestId, SessionId};
use andromeda_exec::{
    encode_inventory_reserve_stock_v0_execute_frame, inventory_reserve_stock_v0_pdf_srpl_source,
    CompletionStatus, InventoryStock, InvocationContext, InvocationRequest,
    V0InventoryRecoverableRuntime, V0InventoryReserveStockRpcPayload,
};
use andromeda_observe::{InMemoryEventSink, TraceId};
use andromeda_quic::{
    validate_result_stream_sequence, validate_single_frame_on_stream, FrameType, StreamRole,
};
use andromeda_storage::{
    recover_from_file_wal, DatabaseManifest, FileWal, InMemoryWal, Lsn, RedoRecordDecision,
    StartupMode,
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
        outcome.vertical.completion.status,
        CompletionStatus::Committed
    );
    assert_eq!(outcome.vertical.completion.rows_affected, Some(2));
    assert_eq!(outcome.vertical.completion.durable_lsn, Some(Lsn::new(3)));
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

    assert_eq!(sink.events().len(), 3);
    assert!(sink.events()[0].correlation.has_contract_catalog());
    assert!(sink.events()[0].correlation.has_no_transaction_evidence());
    assert_eq!(sink.events()[2].correlation.durable_lsn, Some(3));
}
