use andromeda_error::AndromedaResult;
use andromeda_types::TransactionId;
use std::sync::Arc;

use crate::Lsn;

/// Minimal transaction status contract needed by the durable commit-log owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionLogStatus {
    InFlight,
    Committed,
    RolledBack,
}

impl TransactionLogStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::RolledBack)
    }
}

/// Status-table boundary used by [`crate::CommitLogManager`].
///
/// The owner crate stays independent of MVCC by depending only on this small
/// contract. Facade crates can adapt their concrete status tables to it.
pub trait TransactionStatusStore: Send + Sync {
    fn status(&self, tx_id: TransactionId) -> Option<TransactionLogStatus>;

    fn record_commit_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        commit_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()>;

    fn record_rollback_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()>;

    fn restore_terminal_from_validated_replay(
        &self,
        tx_id: TransactionId,
        status: TransactionLogStatus,
    ) -> AndromedaResult<()>;
}

impl<T: TransactionStatusStore + ?Sized> TransactionStatusStore for Arc<T> {
    fn status(&self, tx_id: TransactionId) -> Option<TransactionLogStatus> {
        self.as_ref().status(tx_id)
    }

    fn record_commit_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        commit_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.as_ref()
            .record_commit_from_durable_evidence(tx_id, commit_lsn, durable_lsn)
    }

    fn record_rollback_from_durable_evidence(
        &self,
        tx_id: TransactionId,
        rollback_lsn: Lsn,
        durable_lsn: Lsn,
    ) -> AndromedaResult<()> {
        self.as_ref()
            .record_rollback_from_durable_evidence(tx_id, rollback_lsn, durable_lsn)
    }

    fn restore_terminal_from_validated_replay(
        &self,
        tx_id: TransactionId,
        status: TransactionLogStatus,
    ) -> AndromedaResult<()> {
        self.as_ref()
            .restore_terminal_from_validated_replay(tx_id, status)
    }
}
