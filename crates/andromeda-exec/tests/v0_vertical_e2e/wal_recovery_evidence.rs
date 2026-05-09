use crate::support::{
    context, encoded_execute_frame, executable_procedure, inventory_catalog_snapshot, request,
    stock,
};
use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_inventory_demo::V0InventoryRecoverableRuntime;
use andromeda_storage::{
    DatabaseManifest, ProductStockRow, RedoRecordDecision, ReplayContext, StartupMode,
    execute_redo_plan_into_context, recover_from_file_wal,
};
use andromeda_storage_heap::HeapRowRedoPayloadV1;
use andromeda_wal::{FileWal, Lsn};

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
