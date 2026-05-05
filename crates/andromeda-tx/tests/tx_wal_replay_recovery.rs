use andromeda_core::{AndromedaErrorKind, AndromedaResult, EngineTimestamp, TransactionId};
use andromeda_tx::{
    CommitLogManager, IsolationLevel, Lsn, TransactionStatus, TransactionStatusTable,
    TxWalReplayRecord, WalRecordKind,
};
use std::sync::Arc;

struct NoopWal;

#[async_trait::async_trait]
impl andromeda_tx::commit_log::InvocationWal for NoopWal {
    async fn append(
        &self,
        _kind: WalRecordKind,
        _transaction_id: Option<TransactionId>,
        _payload: &[u8],
    ) -> AndromedaResult<Lsn> {
        Ok(Lsn::new(1))
    }

    async fn flush_through(&self, lsn: Lsn) -> AndromedaResult<Lsn> {
        Ok(lsn)
    }
}

fn recovered_commit_log() -> (CommitLogManager, Arc<TransactionStatusTable>) {
    let status_table = Arc::new(TransactionStatusTable::new());
    let commit_log = CommitLogManager::new(Arc::new(NoopWal), status_table.clone());
    (commit_log, status_table)
}

fn ts(value: u64) -> EngineTimestamp {
    EngineTimestamp::from_unix_millis(value)
}

#[test]
fn reconstructs_terminal_status_from_transaction_local_replay_records() {
    let (commit_log, status_table) = recovered_commit_log();
    let committed = TransactionId::new(10);
    let rolled_back = TransactionId::new(11);
    let incomplete = TransactionId::new(12);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(
                committed,
                Lsn::new(5),
                ts(100),
                3,
                IsolationLevel::Serializable,
            ),
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(6), ts(101), 0xCAFE),
            TxWalReplayRecord::incomplete(incomplete, Lsn::new(7)),
        ])
        .unwrap();

    assert_eq!(summary.commits_restored, 1);
    assert_eq!(summary.rollbacks_restored, 1);
    assert_eq!(summary.incomplete_transactions, 1);
    assert_eq!(
        status_table.status(committed),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );
    assert_eq!(status_table.status(incomplete), None);
    assert_eq!(commit_log.get_commit_lsn(committed), Some(Lsn::new(5)));
    assert_eq!(commit_log.get_rollback_lsn(rolled_back), Some(Lsn::new(6)));
}

#[test]
fn duplicate_terminal_records_are_idempotent_and_preserve_first_evidence() {
    let (commit_log, status_table) = recovered_commit_log();
    let committed = TransactionId::new(20);
    let rolled_back = TransactionId::new(21);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(committed, Lsn::new(8), ts(200), 1, IsolationLevel::Snapshot),
            TxWalReplayRecord::commit(
                committed,
                Lsn::new(9),
                ts(201),
                99,
                IsolationLevel::Serializable,
            ),
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(10), ts(202), 1),
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(11), ts(203), 2),
        ])
        .unwrap();

    assert_eq!(summary.commits_restored, 1);
    assert_eq!(summary.duplicate_commits, 1);
    assert_eq!(summary.rollbacks_restored, 1);
    assert_eq!(summary.duplicate_rollbacks, 1);
    assert_eq!(commit_log.get_commit_lsn(committed), Some(Lsn::new(8)));
    assert_eq!(commit_log.get_rollback_lsn(rolled_back), Some(Lsn::new(10)));
    assert_eq!(
        status_table.status(committed),
        Some(TransactionStatus::Committed)
    );
    assert_eq!(
        status_table.status(rolled_back),
        Some(TransactionStatus::RolledBack)
    );
}

#[test]
fn conflicting_terminal_records_are_rejected() {
    let (commit_log, _status_table) = recovered_commit_log();
    let tx_id = TransactionId::new(30);

    let error = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(tx_id, Lsn::new(12), ts(300), 1, IsolationLevel::Snapshot),
            TxWalReplayRecord::rollback(tx_id, Lsn::new(13), ts(301), 7),
        ])
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(commit_log.get_commit_lsn(tx_id), Some(Lsn::new(12)));
    assert_eq!(commit_log.get_rollback_lsn(tx_id), None);
}

#[test]
fn incomplete_transactions_remain_invisible_after_replay() {
    let (commit_log, status_table) = recovered_commit_log();
    let incomplete = TransactionId::new(40);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::incomplete(incomplete, Lsn::new(14))])
        .unwrap();

    assert_eq!(summary.incomplete_transactions, 1);
    assert_eq!(status_table.status(incomplete), None);
    assert!(!commit_log.is_committed(incomplete));
    assert!(!commit_log.is_rolled_back(incomplete));
}
