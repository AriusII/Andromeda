use crate::support::{manifest, record};
use andromeda_recovery::{RecoveryPlan, RedoRecordDecision, StartupMode};
use andromeda_types::TransactionId;
use andromeda_wal::WalRecordKind;
use andromeda_wal::{DurableTransactionState, InMemoryWal, Lsn, classify_durable_transactions};

#[test]
fn recovery_replays_only_committed_transaction_records() {
    let committed = 101;
    let rolled_back = 102;
    let incomplete = 103;
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(committed), b""),
        record(
            WalRecordKind::RowInsert,
            2,
            Some(1),
            Some(committed),
            b"commit-row",
        ),
        record(WalRecordKind::TxCommit, 3, Some(2), Some(committed), b""),
        record(WalRecordKind::TxBegin, 4, Some(3), Some(rolled_back), b""),
        record(
            WalRecordKind::RowInsert,
            5,
            Some(4),
            Some(rolled_back),
            b"rollback-row",
        ),
        record(
            WalRecordKind::TxRollback,
            6,
            Some(5),
            Some(rolled_back),
            b"",
        ),
        record(WalRecordKind::TxBegin, 7, Some(6), Some(incomplete), b""),
        record(
            WalRecordKind::RowInsert,
            8,
            Some(7),
            Some(incomplete),
            b"incomplete-row",
        ),
    ];

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    )
    .unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
    assert_eq!(
        plan.committed_replay_lsns().collect::<Vec<_>>(),
        vec![Lsn::new(2)]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(2))
            .unwrap()
            .transaction_state,
        Some(DurableTransactionState::Committed)
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(5))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipRolledBackTransaction
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(8))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn committed_redo_records_exclude_incomplete_and_rollback_terminals() {
    let committed = 111;
    let rolled_back = 112;
    let incomplete = 113;
    let records = vec![
        record(WalRecordKind::PageAllocate, 1, None, None, b"page"),
        record(WalRecordKind::TxBegin, 2, Some(1), Some(committed), b""),
        record(
            WalRecordKind::RowInsert,
            3,
            Some(2),
            Some(committed),
            b"committed-row",
        ),
        record(WalRecordKind::TxCommit, 4, Some(3), Some(committed), b""),
        record(WalRecordKind::TxBegin, 5, Some(4), Some(rolled_back), b""),
        record(
            WalRecordKind::RowUpdate,
            6,
            Some(5),
            Some(rolled_back),
            b"rolled-back-row",
        ),
        record(
            WalRecordKind::TxRollback,
            7,
            Some(6),
            Some(rolled_back),
            b"",
        ),
        record(WalRecordKind::TxBegin, 8, Some(7), Some(incomplete), b""),
        record(
            WalRecordKind::RowDelete,
            9,
            Some(8),
            Some(incomplete),
            b"incomplete-row",
        ),
    ];

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    )
    .unwrap();

    assert_eq!(
        plan.committed_redo_records()
            .map(|record| (record.lsn, record.transaction_state))
            .collect::<Vec<_>>(),
        vec![
            (Lsn::new(1), None),
            (Lsn::new(3), Some(DurableTransactionState::Committed)),
        ]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(6))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipRolledBackTransaction
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(9))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );
}

#[test]
fn recovery_continuity_replays_only_committed_inventory_update() {
    let committed = TransactionId::new(121);
    let rolled_back = TransactionId::new(122);
    let incomplete = TransactionId::new(123);
    let records = vec![
        record(WalRecordKind::TxBegin, 1, None, Some(committed.get()), b""),
        record(
            WalRecordKind::RowUpdate,
            2,
            Some(1),
            Some(committed.get()),
            b"Inventory:item=sku-7,delta=-1",
        ),
        record(
            WalRecordKind::TxCommit,
            3,
            Some(2),
            Some(committed.get()),
            b"",
        ),
        record(
            WalRecordKind::TxBegin,
            4,
            Some(3),
            Some(rolled_back.get()),
            b"",
        ),
        record(
            WalRecordKind::RowUpdate,
            5,
            Some(4),
            Some(rolled_back.get()),
            b"Inventory:item=sku-8,delta=-1",
        ),
        record(
            WalRecordKind::TxRollback,
            6,
            Some(5),
            Some(rolled_back.get()),
            b"",
        ),
        record(
            WalRecordKind::TxBegin,
            7,
            Some(6),
            Some(incomplete.get()),
            b"",
        ),
        record(
            WalRecordKind::RowUpdate,
            8,
            Some(7),
            Some(incomplete.get()),
            b"Inventory:item=sku-9,delta=-1",
        ),
    ];

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &records,
    )
    .unwrap();

    assert_eq!(plan.replay_lsns().collect::<Vec<_>>(), vec![Lsn::new(2)]);
    assert_eq!(
        plan.committed_redo_records()
            .map(|record| (record.lsn, record.kind, record.transaction_id))
            .collect::<Vec<_>>(),
        vec![(Lsn::new(2), WalRecordKind::RowUpdate, Some(committed))]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(5))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipRolledBackTransaction
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == Lsn::new(8))
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );

    let classifications = classify_durable_transactions(records.iter());
    assert_eq!(
        classifications
            .committed_transaction_ids()
            .collect::<Vec<_>>(),
        vec![committed]
    );
    assert_eq!(
        classifications
            .rolled_back_transaction_ids()
            .collect::<Vec<_>>(),
        vec![rolled_back]
    );
    assert_eq!(
        classifications
            .incomplete_transaction_ids()
            .collect::<Vec<_>>(),
        vec![incomplete]
    );
}

#[test]
fn recovery_continuity_respects_in_memory_flush_boundary() {
    let committed = TransactionId::new(131);
    let rollback_tail = TransactionId::new(132);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(committed).unwrap();
    let committed_update_lsn = wal
        .append_payload(
            WalRecordKind::RowUpdate,
            Some(committed),
            b"Inventory:item=sku-10,delta=-1",
        )
        .unwrap();
    wal.append_tx_commit(committed).unwrap();
    wal.append_tx_begin(rollback_tail).unwrap();
    let rollback_row_lsn = wal
        .append_payload(
            WalRecordKind::RowUpdate,
            Some(rollback_tail),
            b"Inventory:item=sku-11,delta=-1",
        )
        .unwrap();
    let unflushed_rollback_terminal = wal.append_tx_rollback(rollback_tail).unwrap();

    wal.flush_through(rollback_row_lsn).unwrap();
    assert_eq!(wal.durable_lsn(), rollback_row_lsn);
    assert!(unflushed_rollback_terminal > wal.durable_lsn());

    let durable_records = wal.replay_durable();
    assert_eq!(
        durable_records.last().map(|record| record.header.lsn),
        Some(rollback_row_lsn)
    );
    assert!(
        durable_records
            .iter()
            .all(|record| record.header.lsn < unflushed_rollback_terminal)
    );

    let plan = RecoveryPlan::from_manifest_and_wal(
        &manifest(Lsn::new(1)),
        StartupMode::SafeStart,
        &durable_records,
    )
    .unwrap();

    assert_eq!(
        plan.replay_lsns().collect::<Vec<_>>(),
        vec![committed_update_lsn]
    );
    assert_eq!(
        plan.records
            .iter()
            .find(|record| record.lsn == rollback_row_lsn)
            .unwrap()
            .decision,
        RedoRecordDecision::SkipIncompleteTransaction
    );

    let classifications = wal.classify_durable_transactions();
    assert_eq!(
        classifications
            .committed_transaction_ids()
            .collect::<Vec<_>>(),
        vec![committed]
    );
    assert_eq!(
        classifications
            .rolled_back_transaction_ids()
            .collect::<Vec<_>>(),
        Vec::<TransactionId>::new()
    );
    assert_eq!(
        classifications
            .incomplete_transaction_ids()
            .collect::<Vec<_>>(),
        vec![rollback_tail]
    );
}
