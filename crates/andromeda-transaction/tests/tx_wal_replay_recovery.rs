use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_mvcc::{
    MvccIsolationPolicy, MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable,
};
use andromeda_time::EngineTimestamp;
use andromeda_transaction::CommitLogManager;
use andromeda_transaction_log::{IsolationLevel, Lsn, TxWalReplayRecord, WalRecordKind};
use andromeda_types::{CatalogVersion, TransactionId};
use std::sync::Arc;

struct NoopWal;

#[async_trait::async_trait]
impl andromeda_transaction_log::InvocationWal for NoopWal {
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
fn public_status_table_rejects_terminal_status_without_durable_evidence() {
    let status_table = TransactionStatusTable::new();
    let committed = TransactionId::new(8);
    let rolled_back = TransactionId::new(9);

    let commit_error = status_table
        .record(committed, TransactionStatus::Committed)
        .expect_err("direct committed status requires durable terminal evidence");
    assert_eq!(commit_error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(committed), None);

    let set_committed_error = status_table
        .set_committed(committed)
        .expect_err("set_committed remains a rejected compatibility shim");
    assert_eq!(set_committed_error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(committed), None);

    let rollback_error = status_table
        .record(rolled_back, TransactionStatus::RolledBack)
        .expect_err("direct rolled-back status requires durable terminal evidence");
    assert_eq!(rollback_error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(rolled_back), None);
}

#[test]
fn validated_replay_can_restore_terminal_statuses() {
    let (commit_log, status_table) = recovered_commit_log();
    let committed = TransactionId::new(13);
    let rolled_back = TransactionId::new(14);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(committed, Lsn::new(15), ts(50), 1, IsolationLevel::Snapshot),
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(16), ts(51), 2),
        ])
        .unwrap();

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
    assert_eq!(commit_log.get_commit_lsn(committed), Some(Lsn::new(15)));
    assert_eq!(commit_log.get_rollback_lsn(rolled_back), Some(Lsn::new(16)));
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
fn exact_duplicate_terminal_records_are_idempotent_and_preserve_first_evidence() {
    let (commit_log, status_table) = recovered_commit_log();
    let committed = TransactionId::new(20);
    let rolled_back = TransactionId::new(21);
    let commit =
        TxWalReplayRecord::commit(committed, Lsn::new(8), ts(200), 1, IsolationLevel::Snapshot);
    let rollback = TxWalReplayRecord::rollback(rolled_back, Lsn::new(10), ts(202), 1);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([commit.clone(), commit, rollback.clone(), rollback])
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
fn divergent_duplicate_terminal_records_are_rejected() {
    let (commit_log, status_table) = recovered_commit_log();
    let committed = TransactionId::new(22);

    let error = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(committed, Lsn::new(8), ts(200), 1, IsolationLevel::Snapshot),
            TxWalReplayRecord::commit(
                committed,
                Lsn::new(9),
                ts(201),
                99,
                IsolationLevel::Serializable,
            ),
        ])
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(committed), None);
    assert_eq!(commit_log.get_commit_lsn(committed), None);
}

#[test]
fn conflicting_terminal_records_are_rejected() {
    let (commit_log, status_table) = recovered_commit_log();
    let tx_id = TransactionId::new(30);

    let error = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(tx_id, Lsn::new(12), ts(300), 1, IsolationLevel::Snapshot),
            TxWalReplayRecord::rollback(tx_id, Lsn::new(13), ts(301), 7),
        ])
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert_eq!(status_table.status(tx_id), None);
    assert_eq!(commit_log.get_commit_lsn(tx_id), None);
    assert_eq!(commit_log.get_rollback_lsn(tx_id), None);
}

#[test]
fn replay_rejects_terminal_durable_lsn_below_record_lsn() {
    let (commit_log, status_table) = recovered_commit_log();
    let committed = TransactionId::new(34);

    let commit_error = commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::commit_with_durable_lsn(
            committed,
            Lsn::new(20),
            Lsn::new(19),
            ts(400),
            1,
            IsolationLevel::Snapshot,
        )])
        .unwrap_err();

    assert_eq!(commit_error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(committed), None);
    assert_eq!(commit_log.get_commit_lsn(committed), None);

    let rolled_back = TransactionId::new(35);
    let rollback_error = commit_log
        .reconstruct_from_tx_wal_replay([TxWalReplayRecord::rollback_with_durable_lsn(
            rolled_back,
            Lsn::new(21),
            Lsn::new(20),
            ts(401),
            0xAA,
        )])
        .unwrap_err();

    assert_eq!(rollback_error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(status_table.status(rolled_back), None);
    assert_eq!(commit_log.get_rollback_lsn(rolled_back), None);
}

#[test]
fn replay_rejects_out_of_order_lsn_stream() {
    let (commit_log, status_table) = recovered_commit_log();
    let committed = TransactionId::new(36);
    let rolled_back = TransactionId::new(37);

    let error = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(
                committed,
                Lsn::new(30),
                ts(500),
                1,
                IsolationLevel::Snapshot,
            ),
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(29), ts(501), 0),
        ])
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert!(error.message().contains("LSN order regressed"));
    assert_eq!(status_table.status(committed), None);
    assert_eq!(status_table.status(rolled_back), None);
    assert_eq!(commit_log.get_commit_lsn(committed), None);
    assert_eq!(commit_log.get_rollback_lsn(rolled_back), None);
}

#[test]
fn replay_rejects_equal_lsn_for_distinct_terminal_records() {
    let (commit_log, status_table) = recovered_commit_log();
    let committed = TransactionId::new(38);
    let rolled_back = TransactionId::new(39);

    let error = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::commit(
                committed,
                Lsn::new(33),
                ts(520),
                1,
                IsolationLevel::Snapshot,
            ),
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(33), ts(521), 0),
        ])
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transaction);
    assert!(error.message().contains("LSN reused by distinct records"));
    assert_eq!(status_table.status(committed), None);
    assert_eq!(status_table.status(rolled_back), None);
    assert_eq!(commit_log.get_commit_lsn(committed), None);
    assert_eq!(commit_log.get_rollback_lsn(rolled_back), None);
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

#[test]
fn aborted_and_incomplete_replay_records_do_not_make_mvcc_rows_visible() {
    let (commit_log, status_table) = recovered_commit_log();
    let rolled_back = TransactionId::new(41);
    let incomplete = TransactionId::new(42);

    let summary = commit_log
        .reconstruct_from_tx_wal_replay([
            TxWalReplayRecord::rollback(rolled_back, Lsn::new(40), ts(600), 0xCAFE),
            TxWalReplayRecord::incomplete(incomplete, Lsn::new(41)),
        ])
        .unwrap();

    assert_eq!(summary.rollbacks_restored, 1);
    assert_eq!(summary.incomplete_transactions, 1);

    let snapshot = Snapshot::with_context(
        100,
        CatalogVersion::new(1),
        MvccIsolationPolicy::ReadCommitted,
        None,
        Vec::<TransactionId>::new(),
    )
    .unwrap();
    let rolled_back_row = MvccRowHeader::open_version(10, rolled_back, None).unwrap();
    let incomplete_row = MvccRowHeader::open_version(10, incomplete, None).unwrap();

    assert!(
        !rolled_back_row
            .visible_in_snapshot(&snapshot, &status_table)
            .unwrap()
    );
    assert!(
        !incomplete_row
            .visible_in_snapshot(&snapshot, &status_table)
            .unwrap()
    );
}
