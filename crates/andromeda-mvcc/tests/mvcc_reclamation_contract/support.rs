use andromeda_mvcc::{ReclamationMarkCandidate, TransactionStatusTable};
use andromeda_transaction_log::Lsn;
use andromeda_types::TransactionId;

pub(crate) const DEFAULT_END_TS: u64 = 50;
pub(crate) const DEFAULT_MARKED_AT: u64 = 20;
pub(crate) const DEFAULT_GC_EPOCH: u64 = 5;
pub(crate) const DEFAULT_MIN_VISIBLE_TS: u64 = 100;
pub(crate) const DEFAULT_CURRENT_GC_EPOCH: u64 = 10;

pub(crate) fn tx_id(id: u64) -> TransactionId {
    TransactionId::new(id)
}

pub(crate) fn candidate(
    version_id: u64,
    creator_tx_id: TransactionId,
    end_ts: u64,
    marked_at: u64,
    gc_epoch: u64,
) -> ReclamationMarkCandidate {
    ReclamationMarkCandidate::new(version_id, creator_tx_id, end_ts, marked_at, gc_epoch)
}

pub(crate) fn default_candidate(
    version_id: u64,
    creator_tx_id: TransactionId,
) -> ReclamationMarkCandidate {
    candidate(
        version_id,
        creator_tx_id,
        DEFAULT_END_TS,
        DEFAULT_MARKED_AT,
        DEFAULT_GC_EPOCH,
    )
}

pub(crate) fn committed_creator(id: u64) -> (TransactionStatusTable, TransactionId) {
    let status_table = TransactionStatusTable::new();
    let tx_id = tx_id(id);
    record_committed(&status_table, tx_id);
    (status_table, tx_id)
}

pub(crate) fn record_committed(status_table: &TransactionStatusTable, tx_id: TransactionId) {
    status_table
        .record_committed_after_durable_wal(tx_id, Lsn::new(1), Lsn::new(1))
        .unwrap();
}
