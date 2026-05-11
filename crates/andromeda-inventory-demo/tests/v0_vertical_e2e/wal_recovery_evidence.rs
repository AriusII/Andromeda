use crate::support::{
    context, encoded_execute_frame, executable_procedure, inventory_catalog_snapshot, request,
    stock,
};
use andromeda_inventory_demo::V0InventoryRecoverableRuntime;
use andromeda_inventory_demo::inventory_reserve_stock_contract;
use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{
    RedoRecordDecision, ReplayContext, StartupMode, execute_redo_plan_into_context,
    recover_from_file_wal,
};
use andromeda_storage_heap::{HeapRowRedoPayloadV1, ProductStockRow};
use andromeda_wal::{DurableTransactionState, FileWal, Lsn, WalRecordKind};

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
    let procedure = executable_procedure(&catalog, &contract);
    {
        let wal = FileWal::open(&path).unwrap();
        let mut runtime = V0InventoryRecoverableRuntime::new(wal);
        runtime
            .execute_encoded_inventory_reserve_stock(
                &encoded_execute_frame(),
                &procedure,
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
        segment_index_file_id: 0,
        btree_root_page_id: 0,
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
fn v0_inventory_file_wal_replays_exec_product_stock_hredov1_into_context() {
    let path = std::env::temp_dir().join(format!(
        "andromeda-v0-exec-{}-{}.wal",
        std::process::id(),
        705
    ));
    std::fs::remove_file(&path).ok();

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let outcome = {
        let wal = FileWal::open(&path).unwrap();
        let mut runtime = V0InventoryRecoverableRuntime::new(wal);
        runtime
            .execute_encoded_inventory_reserve_stock(
                &encoded_execute_frame(),
                &procedure,
                request(&contract, 705),
                &context(&contract, 7005),
                stock(),
            )
            .unwrap()
    };

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 1,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    };
    let redo = recover_from_file_wal(&manifest, StartupMode::SafeStart, &path).unwrap();
    let disk_wal = FileWal::open(&path).unwrap();
    let durable_records = disk_wal.replay_durable();
    assert_eq!(disk_wal.durable_lsn(), Lsn::new(3));
    drop(disk_wal);

    let durable_redo_record = durable_records
        .iter()
        .find(|record| record.header.lsn == outcome.product_stock_redo.redo_record_lsn)
        .unwrap();
    let durable_hredov1 = HeapRowRedoPayloadV1::decode(
        &durable_redo_record.payload,
        durable_redo_record.header.kind,
    )
    .unwrap();
    assert_eq!(durable_hredov1, outcome.product_stock_redo.redo_payload);
    let expected_row = ProductStockRow::new(42, 7).unwrap();
    assert_eq!(
        ProductStockRow::decode(durable_hredov1.tuple()).unwrap(),
        expected_row
    );

    // WAL-only proof: strict cold snapshot hydration is left out until the
    // page_lsn contract is available in this E2E harness. This proves the
    // durable exec-emitted HREDOV1 record can reconstruct the typed row.
    let mut replay = ReplayContext::new();
    let report = execute_redo_plan_into_context(&redo, &durable_records, &mut replay).unwrap();

    assert_eq!(
        redo.committed_replay_lsns().collect::<Vec<_>>(),
        vec![Lsn::new(2)]
    );
    assert_eq!(report.applied_count, 1);
    assert_eq!(report.plan_skipped_count, 2);
    assert_eq!(report.committed_transaction_count, 1);
    assert_eq!(report.incomplete_transaction_count, 0);
    assert_eq!(
        report.replay_end_lsn,
        Some(outcome.product_stock_redo.redo_record_lsn)
    );
    assert!(!report.has_replay_errors);

    let recovered_page = replay.heap_redo_page(durable_hredov1.page_id()).unwrap();
    assert_eq!(
        replay.heap_redo_page_origin(durable_hredov1.page_id()),
        Some("redo-created")
    );
    assert_eq!(replay.heap_redo_page_count(), 1);
    assert_eq!(
        recovered_page.page_lsn(),
        outcome.product_stock_redo.redo_record_lsn
    );
    assert_eq!(
        recovered_page.decode_product_stock_rows().unwrap(),
        vec![(durable_hredov1.after_slot_id(), expected_row)]
    );
    assert_eq!(
        recovered_page.decode_product_stock_recovery_rows().unwrap(),
        vec![(
            durable_hredov1.after_slot_id(),
            outcome.product_stock_redo.redo_record_lsn,
            expected_row
        )]
    );
    assert!(!replay.has_errors());

    std::fs::remove_file(&path).ok();
}

#[test]
fn v0_inventory_file_wal_recovers_product_stock_after_crash_before_client_ack() {
    let path = std::env::temp_dir().join(format!(
        "andromeda-v0-exec-{}-{}.wal",
        std::process::id(),
        706
    ));
    std::fs::remove_file(&path).ok();

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let (
        transaction_id,
        redo_record_lsn,
        expected_redo_payload,
        volatile_result_stream_frame_count,
    ) = {
        let wal = FileWal::open(&path).unwrap();
        let mut runtime = V0InventoryRecoverableRuntime::new(wal);
        let outcome = runtime
            .execute_encoded_inventory_reserve_stock(
                &encoded_execute_frame(),
                &procedure,
                request(&contract, 706),
                &context(&contract, 7006),
                stock(),
            )
            .unwrap();

        assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
        assert_eq!(runtime.wal().replay_durable().len(), 3);
        assert_eq!(outcome.durable_lsn(), Some(Lsn::new(3)));
        assert_eq!(outcome.product_stock_redo.redo_record_lsn, Lsn::new(2));
        assert_eq!(outcome.product_stock_redo.durable_commit_lsn, Lsn::new(3));

        (
            outcome.vertical.transaction_id,
            outcome.product_stock_redo.redo_record_lsn,
            outcome.product_stock_redo.redo_payload,
            outcome.result_frames.len(),
        )
        // Crash cut: the runtime, in-memory ProductStock publication, and
        // ResultStream frames are dropped before any client ACK is observed.
        // Recovery below intentionally reopens only the durable FileWal.
    };

    let disk_scan = FileWal::scan_path(&path).unwrap();
    assert_eq!(disk_scan.header.durable_lsn, Lsn::new(3));
    assert_eq!(disk_scan.durable_lsn, Lsn::new(3));
    assert_eq!(disk_scan.scan.records.len(), 3);

    let disk_wal = FileWal::open(&path).unwrap();
    let durable_records = disk_wal.replay_durable();
    assert_eq!(disk_wal.durable_lsn(), Lsn::new(3));
    drop(disk_wal);

    assert_eq!(
        durable_records
            .iter()
            .map(|record| record.header.kind)
            .collect::<Vec<_>>(),
        vec![
            WalRecordKind::TxBegin,
            WalRecordKind::RowInsert,
            WalRecordKind::TxCommit,
        ]
    );
    assert_eq!(durable_records[0].header.lsn, Lsn::new(1));
    assert_eq!(durable_records[0].header.previous_lsn, None);
    assert_eq!(durable_records[1].header.lsn, redo_record_lsn);
    assert_eq!(durable_records[1].header.previous_lsn, Some(Lsn::new(1)));
    assert_eq!(durable_records[2].header.lsn, Lsn::new(3));
    assert_eq!(
        durable_records[2].header.previous_lsn,
        Some(redo_record_lsn)
    );
    assert!(
        durable_records
            .iter()
            .all(|record| record.header.transaction_id == Some(transaction_id))
    );

    let durable_hredov1 =
        HeapRowRedoPayloadV1::decode(&durable_records[1].payload, durable_records[1].header.kind)
            .unwrap();
    assert_eq!(durable_hredov1, expected_redo_payload);
    let expected_row = ProductStockRow::new(42, 7).unwrap();
    assert_eq!(
        ProductStockRow::decode(durable_hredov1.tuple()).unwrap(),
        expected_row
    );

    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 1,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    };
    let redo = recover_from_file_wal(&manifest, StartupMode::SafeStart, &path).unwrap();

    assert_eq!(redo.durable_lsn, Lsn::new(3));
    assert_eq!(
        redo.committed_replay_lsns().collect::<Vec<_>>(),
        vec![redo_record_lsn]
    );
    let transaction = redo
        .transaction_evidence
        .iter()
        .find(|transaction| transaction.transaction_id == transaction_id)
        .unwrap();
    assert_eq!(transaction.state, DurableTransactionState::Committed);
    assert_eq!(transaction.begin_lsn, Some(Lsn::new(1)));
    assert_eq!(transaction.commit_lsn, Some(Lsn::new(3)));
    assert_eq!(transaction.record_count, 3);
    assert!(redo.incomplete_transactions.is_empty());
    assert_eq!(
        redo.records
            .iter()
            .map(|record| (record.kind, record.decision))
            .collect::<Vec<_>>(),
        vec![
            (
                WalRecordKind::TxBegin,
                RedoRecordDecision::SkipNonRedoRecord
            ),
            (WalRecordKind::RowInsert, RedoRecordDecision::Replay),
            (
                WalRecordKind::TxCommit,
                RedoRecordDecision::SkipNonRedoRecord
            ),
        ]
    );

    let mut replay = ReplayContext::new();
    let report = execute_redo_plan_into_context(&redo, &durable_records, &mut replay).unwrap();

    assert_eq!(report.applied_count, 1);
    assert_eq!(report.plan_skipped_count, 2);
    assert_eq!(report.committed_transaction_count, 1);
    assert_eq!(report.incomplete_transaction_count, 0);
    assert_eq!(report.replay_end_lsn, Some(redo_record_lsn));
    assert!(!report.has_replay_errors);
    assert_eq!(
        volatile_result_stream_frame_count, 3,
        "ResultStream/ACK evidence is intentionally not a recovery input"
    );

    let recovered_page = replay.heap_redo_page(durable_hredov1.page_id()).unwrap();
    assert_eq!(replay.heap_redo_page_count(), 1);
    assert_eq!(recovered_page.page_lsn(), redo_record_lsn);
    assert_eq!(
        recovered_page.decode_product_stock_rows().unwrap(),
        vec![(durable_hredov1.after_slot_id(), expected_row)]
    );
    assert_eq!(
        recovered_page.decode_product_stock_recovery_rows().unwrap(),
        vec![(
            durable_hredov1.after_slot_id(),
            redo_record_lsn,
            expected_row
        )]
    );
    assert!(!replay.has_errors());

    std::fs::remove_file(&path).ok();
}
