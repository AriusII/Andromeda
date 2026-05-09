use andromeda_core::TransactionId;
use andromeda_mvcc::{
    ActiveSnapshotRegistry, MvccGarbageCollector, SnapshotHandle, TransactionStatusTable,
};
use andromeda_transaction_log::Lsn;
use std::sync::Arc;

pub(crate) fn durable_status_lsn() -> Lsn {
    Lsn::new(1)
}

pub(crate) fn setup_gc_system() -> (
    Arc<ActiveSnapshotRegistry>,
    Arc<TransactionStatusTable>,
    Arc<MvccGarbageCollector>,
) {
    let registry = Arc::new(ActiveSnapshotRegistry::new());
    let status_table = Arc::new(TransactionStatusTable::new());
    let collector = Arc::new(MvccGarbageCollector::new(
        registry.clone(),
        status_table.clone(),
    ));
    (registry, status_table, collector)
}

pub(crate) fn record_committed(status_table: &TransactionStatusTable, tx_id: TransactionId) {
    status_table
        .record_committed_after_durable_wal(tx_id, durable_status_lsn(), durable_status_lsn())
        .expect("commit");
}

pub(crate) fn record_rolled_back(status_table: &TransactionStatusTable, tx_id: TransactionId) {
    status_table
        .record_rolled_back_after_durable_wal(tx_id, durable_status_lsn(), durable_status_lsn())
        .expect("rollback");
}

pub(crate) fn register_snapshot(
    registry: &ActiveSnapshotRegistry,
    timestamp: u64,
    tx_id: TransactionId,
) -> SnapshotHandle {
    let snapshot = SnapshotHandle::new(timestamp, tx_id).expect("snapshot");
    registry
        .register_snapshot(snapshot)
        .expect("register snapshot");
    snapshot
}
