use crate::support::{
    context, encoded_execute_frame, executable_procedure, inventory_catalog_snapshot, request,
    stock,
};
use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_exec::V0InventoryRecoverableRuntime;
use andromeda_storage::{
    DatabaseManifest, FileWal, Lsn, RedoRecordDecision, StartupMode, recover_from_file_wal,
};

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
