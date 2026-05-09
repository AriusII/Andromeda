use super::support::manifest_for_replay_from;
use andromeda_storage::{
    InMemoryWal, RecoveryPlan, RedoRecordDecision, StartupMode, UndoChainsBuilder, UndoOperation,
    WalRecordKind,
};
use andromeda_types::TransactionId;

#[test]
fn redo_plan_and_undo_builder_preserve_opposite_lsn_ordering() {
    let tx_id = TransactionId::new(44);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(tx_id)
        .expect("begin record should append");
    let insert_lsn = wal
        .append_payload(WalRecordKind::RowInsert, Some(tx_id), b"insert")
        .expect("insert record should append");
    let update_lsn = wal
        .append_payload(WalRecordKind::RowUpdate, Some(tx_id), b"update")
        .expect("update record should append");
    let delete_lsn = wal
        .append_payload(WalRecordKind::RowDelete, Some(tx_id), b"delete")
        .expect("delete record should append");
    wal.append_tx_commit(tx_id)
        .expect("commit record should append");
    wal.flush_all().expect("test WAL should flush");

    let durable_records = wal.replay_durable();
    let manifest = manifest_for_replay_from(insert_lsn);
    let plan =
        RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &durable_records)
            .expect("complete committed WAL should produce a redo plan");

    let replay_lsns = plan.replay_lsns().collect::<Vec<_>>();
    assert_eq!(replay_lsns, vec![insert_lsn, update_lsn, delete_lsn]);
    assert!(replay_lsns.windows(2).all(|pair| pair[0] < pair[1]));

    let mut undo_builder = UndoChainsBuilder::new();
    for redo_record in plan
        .records
        .iter()
        .filter(|record| record.decision == RedoRecordDecision::Replay)
    {
        undo_builder
            .add_redo_record(
                redo_record.lsn,
                redo_record.kind,
                redo_record
                    .transaction_id
                    .expect("row redo records should carry transaction ids"),
            )
            .expect("redo record should be accepted into undo builder");
    }

    let chains = undo_builder
        .build()
        .expect("undo builder should create a valid undo chain");
    assert_eq!(chains.len(), 1);
    let chain = &chains[0];
    assert_eq!(chain.transaction_id, tx_id);
    assert_eq!(
        chain
            .records
            .iter()
            .map(|record| record.original_redo_lsn)
            .collect::<Vec<_>>(),
        vec![delete_lsn, update_lsn, insert_lsn]
    );
    assert_eq!(
        chain
            .records
            .iter()
            .map(|record| record.operation)
            .collect::<Vec<_>>(),
        vec![
            UndoOperation::UndoRowDelete,
            UndoOperation::UndoRowUpdate,
            UndoOperation::UndoRowInsert,
        ]
    );
    assert!(
        chain
            .records
            .windows(2)
            .all(|pair| pair[0].original_redo_lsn > pair[1].original_redo_lsn)
    );
}
