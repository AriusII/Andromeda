use andromeda_core::{AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_exec::{
    CommitLogInvocationWal, LocalDispatchPlan, LocalDispatcher, LocalRollbackPlan,
    encode_exec_tx_commit_payload, encode_exec_tx_rollback_payload,
    map_exec_wal_evidence_to_tx_replay,
};
use andromeda_storage::{FileWal, InMemoryWal, Lsn as StorageLsn, WalRecord, WalRecordKind};
use andromeda_tx::{
    CommitLogManager, IsolationLevel, Lsn, TransactionStatus, TransactionStatusTable,
    TxWalReplayRecord,
};
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_wal_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "andromeda-exec-{test_name}-{}-{nanos}.wal",
        std::process::id()
    ))
}

fn commit_payload(
    isolation_level: IsolationLevel,
    row_count_affected: u64,
    parameter_hash: u64,
) -> Vec<u8> {
    encode_exec_tx_commit_payload(isolation_level, row_count_affected, parameter_hash)
}

fn rollback_payload(parameter_hash: u64) -> Vec<u8> {
    encode_exec_tx_rollback_payload(parameter_hash)
}

fn record(
    kind: WalRecordKind,
    lsn: u64,
    previous_lsn: Option<u64>,
    tx_id: Option<TransactionId>,
    payload: impl Into<Vec<u8>>,
) -> WalRecord {
    WalRecord::from_parts(
        kind,
        StorageLsn::new(lsn),
        previous_lsn.map(StorageLsn::new),
        tx_id,
        payload,
    )
    .expect("test WAL record should be valid")
}

fn replay_wal() -> Arc<dyn andromeda_tx::commit_log::InvocationWal> {
    Arc::new(CommitLogInvocationWal::new(InMemoryWal::new()))
}

#[test]
fn local_dispatch_terminal_payloads_replay_through_tx_commit_log() -> AndromedaResult<()> {
    let committed = TransactionId::new(301);
    let rolled_back = TransactionId::new(302);
    let mut wal = InMemoryWal::new();

    let commit_receipt = LocalDispatcher::new(&mut wal).dispatch_commit(LocalDispatchPlan {
        transaction_id: committed,
        mutation_payload: Vec::new(),
        rows_affected: 0,
    })?;
    assert_eq!(commit_receipt.transaction_id, committed);

    let rollback_receipt = LocalDispatcher::new(&mut wal).dispatch_rollback(LocalRollbackPlan {
        transaction_id: rolled_back,
        rollback_payload: rollback_payload(0xBEEF),
    })?;
    assert_eq!(rollback_receipt.transaction_id, rolled_back);

    let bridged = map_exec_wal_evidence_to_tx_replay(wal.durable_records())?;
    assert_eq!(bridged.evidence.commits, 1);
    assert_eq!(bridged.evidence.rollbacks, 1);
    assert_eq!(bridged.evidence.incomplete_transactions, 0);

    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(replay_wal(), status_table.clone());
    let summary = commit_log.reconstruct_from_tx_wal_replay(bridged.replay_records)?;

    assert_eq!(summary.commits_restored, 1);
    assert_eq!(summary.rollbacks_restored, 1);
    assert_eq!(
        status_table.status(committed),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );
    Ok(())
}

#[tokio::test]
async fn commit_log_manager_uses_exec_file_wal_bridge_for_terminal_records() -> AndromedaResult<()>
{
    let path = unique_wal_path("tx-commit-log-wal-bridge");
    let bridge = Arc::new(CommitLogInvocationWal::new(FileWal::open(&path)?));
    let status_table = Arc::new(TransactionStatusTable::new());
    let wal_manager: Arc<dyn andromeda_tx::commit_log::InvocationWal> = bridge.clone();
    let commit_log = CommitLogManager::new(wal_manager.clone(), status_table.clone());

    let committed = TransactionId::new(101);
    let rolled_back = TransactionId::new(102);
    let begin_lsn = bridge.append_tx_begin(committed)?;

    let commit_entry = commit_log
        .record_commit(committed, IsolationLevel::Serializable, 7, 0xC0FFEE)
        .await?;
    let rollback_begin_lsn = bridge.append_tx_begin(rolled_back)?;
    let rollback_entry = commit_log.record_rollback(rolled_back, 0xBAD).await?;

    assert_eq!(begin_lsn, Lsn::new(1));
    assert_eq!(commit_entry.commit_lsn, Lsn::new(2));
    assert_eq!(rollback_begin_lsn, Lsn::new(3));
    assert_eq!(rollback_entry.rollback_lsn, Lsn::new(4));
    assert_eq!(
        status_table.status(committed),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );

    drop(commit_log);
    drop(wal_manager);
    let bridge = match Arc::try_unwrap(bridge) {
        Ok(bridge) => bridge,
        Err(_) => panic!("bridge should have no remaining strong references"),
    };
    drop(bridge.into_inner()?);

    let reopened = FileWal::open(&path)?;
    assert_eq!(reopened.durable_lsn().get(), 4);
    let durable_records: Vec<_> = reopened.durable_records().collect();
    assert_eq!(durable_records.len(), 4);
    assert_eq!(durable_records[0].header.kind, WalRecordKind::TxBegin);
    assert_eq!(durable_records[0].header.transaction_id, Some(committed));
    assert_eq!(durable_records[1].header.kind, WalRecordKind::TxCommit);
    assert_eq!(durable_records[1].header.transaction_id, Some(committed));
    assert_eq!(durable_records[2].header.kind, WalRecordKind::TxBegin);
    assert_eq!(durable_records[2].header.transaction_id, Some(rolled_back));
    assert_eq!(durable_records[3].header.kind, WalRecordKind::TxRollback);
    assert_eq!(durable_records[3].header.transaction_id, Some(rolled_back));

    let bridged = map_exec_wal_evidence_to_tx_replay(reopened.durable_records())?;
    assert_eq!(bridged.evidence.source_records, 4);
    assert_eq!(bridged.evidence.adapter_records, 4);
    assert_eq!(bridged.evidence.commits, 1);
    assert_eq!(bridged.evidence.rollbacks, 1);
    assert_eq!(bridged.evidence.incomplete_transactions, 0);

    let recovered_status_table = Arc::new(TransactionStatusTable::new());
    let recovered_commit_log = CommitLogManager::new(replay_wal(), recovered_status_table.clone());
    let summary = recovered_commit_log.reconstruct_from_tx_wal_replay(bridged.replay_records)?;
    assert_eq!(summary.commits_restored, 1);
    assert_eq!(summary.rollbacks_restored, 1);
    assert_eq!(
        recovered_status_table.status(committed),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        recovered_status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );

    drop(reopened);
    let _ = fs::remove_file(path);
    Ok(())
}

#[test]
fn exec_wal_bridge_preserves_duplicate_terminal_idempotency() -> AndromedaResult<()> {
    let committed = TransactionId::new(201);
    let rolled_back = TransactionId::new(202);

    let bridged = map_exec_wal_evidence_to_tx_replay([
        record(WalRecordKind::TxBegin, 1, None, Some(committed), []),
        record(
            WalRecordKind::TxCommit,
            2,
            Some(1),
            Some(committed),
            commit_payload(IsolationLevel::Snapshot, 3, 0xAAAA),
        ),
        record(WalRecordKind::TxBegin, 4, Some(2), Some(rolled_back), []),
        record(
            WalRecordKind::TxRollback,
            5,
            Some(4),
            Some(rolled_back),
            rollback_payload(0xCAFE),
        ),
    ])?;

    assert_eq!(bridged.evidence.source_records, 4);
    assert_eq!(bridged.evidence.adapter_records, 4);
    assert_eq!(bridged.evidence.commits, 1);
    assert_eq!(bridged.evidence.rollbacks, 1);
    assert_eq!(
        bridged.replay_records,
        vec![
            TxWalReplayRecord::commit(
                committed,
                Lsn::new(2),
                andromeda_core::EngineTimestamp::ZERO,
                3,
                IsolationLevel::Snapshot,
            ),
            TxWalReplayRecord::rollback(
                rolled_back,
                Lsn::new(5),
                andromeda_core::EngineTimestamp::ZERO,
                0xCAFE,
            ),
        ]
    );

    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(replay_wal(), status_table.clone());
    let summary = commit_log.reconstruct_from_tx_wal_replay(bridged.replay_records)?;
    assert_eq!(summary.commits_restored, 1);
    assert_eq!(summary.rollbacks_restored, 1);
    assert_eq!(summary.duplicate_commits, 0);
    assert_eq!(summary.duplicate_rollbacks, 0);
    assert_eq!(
        status_table.status(committed),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );

    let duplicate_summary = commit_log.reconstruct_from_tx_wal_replay([
        TxWalReplayRecord::commit(
            committed,
            Lsn::new(2),
            andromeda_core::EngineTimestamp::ZERO,
            3,
            IsolationLevel::Snapshot,
        ),
        TxWalReplayRecord::rollback(
            rolled_back,
            Lsn::new(5),
            andromeda_core::EngineTimestamp::ZERO,
            0xCAFE,
        ),
    ])?;
    assert_eq!(duplicate_summary.commits_restored, 0);
    assert_eq!(duplicate_summary.rollbacks_restored, 0);
    assert_eq!(duplicate_summary.duplicate_commits, 1);
    assert_eq!(duplicate_summary.duplicate_rollbacks, 1);

    Ok(())
}

#[test]
fn exec_wal_bridge_maps_begin_only_to_incomplete_and_keeps_it_invisible() -> AndromedaResult<()> {
    let tx_id = TransactionId::new(203);

    let bridged = map_exec_wal_evidence_to_tx_replay([
        record(WalRecordKind::TxBegin, 10, None, Some(tx_id), []),
        record(
            WalRecordKind::RowInsert,
            11,
            Some(10),
            Some(tx_id),
            b"row".to_vec(),
        ),
        record(
            WalRecordKind::RowUpdate,
            12,
            Some(11),
            Some(tx_id),
            b"row2".to_vec(),
        ),
    ])?;

    assert_eq!(bridged.evidence.commits, 0);
    assert_eq!(bridged.evidence.rollbacks, 0);
    assert_eq!(bridged.evidence.incomplete_transactions, 1);
    assert_eq!(
        bridged.replay_records,
        vec![TxWalReplayRecord::incomplete(tx_id, Lsn::new(12))]
    );

    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(replay_wal(), status_table.clone());
    let summary = commit_log.reconstruct_from_tx_wal_replay(bridged.replay_records)?;

    assert_eq!(summary.incomplete_transactions, 1);
    assert_eq!(status_table.status(tx_id), None);
    assert!(!commit_log.is_committed(tx_id));
    assert!(!commit_log.is_rolled_back(tx_id));
    Ok(())
}

#[test]
fn exec_wal_bridge_fails_closed_on_conflicting_terminal_evidence() {
    let tx_id = TransactionId::new(204);

    let error = map_exec_wal_evidence_to_tx_replay([
        record(WalRecordKind::TxBegin, 20, None, Some(tx_id), []),
        record(
            WalRecordKind::TxCommit,
            21,
            Some(20),
            Some(tx_id),
            commit_payload(IsolationLevel::Snapshot, 1, 0x1111),
        ),
        record(
            WalRecordKind::TxRollback,
            22,
            Some(21),
            Some(tx_id),
            rollback_payload(0x2222),
        ),
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
}

#[test]
fn exec_wal_bridge_fails_closed_on_same_kind_terminal_drift() {
    let tx_id = TransactionId::new(207);

    let error = map_exec_wal_evidence_to_tx_replay([
        record(WalRecordKind::TxBegin, 60, None, Some(tx_id), []),
        record(
            WalRecordKind::TxCommit,
            61,
            Some(60),
            Some(tx_id),
            commit_payload(IsolationLevel::Snapshot, 1, 0x3333),
        ),
        record(
            WalRecordKind::TxCommit,
            62,
            Some(61),
            Some(tx_id),
            commit_payload(IsolationLevel::Serializable, 2, 0x4444),
        ),
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert!(error.message().contains("conflicting terminal records"));
}

#[test]
fn exec_wal_bridge_fails_closed_on_duplicate_source_lsn() {
    let tx_id = TransactionId::new(206);

    let error = map_exec_wal_evidence_to_tx_replay([
        record(WalRecordKind::TxBegin, 40, None, Some(tx_id), []),
        record(
            WalRecordKind::TxCommit,
            40,
            None,
            Some(tx_id),
            commit_payload(IsolationLevel::Snapshot, 1, 0x3333),
        ),
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("duplicate source LSN"));
}

#[test]
fn exec_wal_bridge_fails_closed_on_zero_transaction_id() {
    let error = map_exec_wal_evidence_to_tx_replay([record(
        WalRecordKind::TxBegin,
        50,
        None,
        Some(TransactionId::new(0)),
        [],
    )])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert!(error.message().contains("zero transaction id"));
}

#[test]
fn exec_wal_bridge_fails_closed_on_malformed_terminal_payload() {
    let tx_id = TransactionId::new(205);

    let error = map_exec_wal_evidence_to_tx_replay([
        record(WalRecordKind::TxBegin, 30, None, Some(tx_id), []),
        record(
            WalRecordKind::TxCommit,
            31,
            Some(30),
            Some(tx_id),
            b"legacy".to_vec(),
        ),
    ])
    .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
}
