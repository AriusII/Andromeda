use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};

/// Strict Two-Phase Locking (2PL) Disciplined Transaction States.
///
/// # 2PL State Machine Overview
///
/// Andromeda enforces strict 2PL discipline to ensure serializability. This state
/// machine models two distinct phases of transaction execution:
///
/// ## Growing Phase (Lock Acquisition)
/// During the growing phase, the transaction acquires locks and performs reads/writes:
/// - `Created` → `Active` (transaction begins, no locks yet)
/// - `Active` (locks acquired, reads/writes performed)
///
/// ## Shrinking Phase (Lock Release)
/// During the shrinking phase, the transaction releases all locks before becoming
/// visible to other transactions:
/// - `Active` → `Committing` (commit requested, shrinking phase begins)
/// - `Committing` (locks released, no new acquisitions allowed)
///
/// Once ANY lock is released, the transaction enters the shrinking phase and may
/// not acquire additional locks. This is the core 2PL invariant.
///
/// ## Terminal States
/// After either commit or rollback completes durably, the transaction enters a
/// terminal state and all lock records must be released:
/// - `Committing` → `Committed` (all locks released, commit durable)
/// - `Committed` → `Disposed` (final cleanup, no operations allowed)
/// - (Rollback path): `Active` → `RollingBack` → `RolledBack` → `Disposed`
///
/// ## Invariants
/// 1. **Lock acquisition only in Growing Phase**: Active or Committing
/// 2. **Lock release only in Shrinking Phase**: Committing or RollingBack
/// 3. **No acquire after release**: Once shrinking begins, only releases are allowed
/// 4. **release_all only terminal**: Only after Committed or RolledBack
/// 5. **No operations after Disposed**: Disposed transactions cannot acquire, release, or operate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    Created,
    Active,
    Committing,
    Committed,
    Failed,
    RollingBack,
    RolledBack,
    Poisoned,
    Disposed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionEvent {
    Begin,
    CommitRequested,
    DurableWalFlushed,
    Fail,
    Poison,
    RollbackRequested,
    RollbackComplete,
    Dispose,
}

impl TransactionState {
    pub fn apply(self, event: TransactionEvent) -> AndromedaResult<Self> {
        match (self, event) {
            (Self::Created, TransactionEvent::Begin) => Ok(Self::Active),
            (Self::Active, TransactionEvent::CommitRequested) => Ok(Self::Committing),
            (Self::Committing, TransactionEvent::DurableWalFlushed) => Ok(Self::Committed),
            (Self::Committed, TransactionEvent::Dispose) => Ok(Self::Disposed),
            (Self::Active, TransactionEvent::RollbackRequested) => Ok(Self::RollingBack),
            (Self::Active, TransactionEvent::Fail) => Ok(Self::Failed),
            (Self::Failed, TransactionEvent::RollbackRequested) => Ok(Self::RollingBack),
            (Self::Active, TransactionEvent::Poison) => Ok(Self::Poisoned),
            (Self::Poisoned, TransactionEvent::RollbackRequested) => Ok(Self::RollingBack),
            (Self::RollingBack, TransactionEvent::RollbackComplete) => Ok(Self::RolledBack),
            (Self::RolledBack, TransactionEvent::Dispose) => Ok(Self::Disposed),
            _ => Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "illegal transaction state transition",
            )),
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::RolledBack | Self::Disposed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionStateMachine {
    pub transaction_id: TransactionId,
    pub state: TransactionState,
    pub durable_commit_lsn: Option<u64>,
    pub durable_rollback_lsn: Option<u64>,
}

impl TransactionStateMachine {
    pub const fn new(transaction_id: TransactionId) -> Self {
        Self {
            transaction_id,
            state: TransactionState::Created,
            durable_commit_lsn: None,
            durable_rollback_lsn: None,
        }
    }

    pub fn apply(&mut self, event: TransactionEvent) -> AndromedaResult<()> {
        let next = self.state.apply(event)?;
        if matches!(next, TransactionState::Committed) && self.durable_commit_lsn.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "commit requires durable WAL LSN before visibility",
            ));
        }

        if matches!(next, TransactionState::RolledBack) && self.durable_rollback_lsn.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "rollback requires durable WAL LSN before completion",
            ));
        }

        self.state = next;
        Ok(())
    }

    pub fn begin(&mut self) -> AndromedaResult<()> {
        self.apply(TransactionEvent::Begin)
    }

    pub fn request_commit(&mut self) -> AndromedaResult<()> {
        self.apply(TransactionEvent::CommitRequested)
    }

    pub fn publish_visible_commit_after_durable_flush(&mut self, lsn: u64) -> AndromedaResult<()> {
        self.mark_durable_commit_lsn(lsn)?;
        self.apply(TransactionEvent::DurableWalFlushed)
    }

    pub fn request_rollback(&mut self) -> AndromedaResult<()> {
        self.apply(TransactionEvent::RollbackRequested)
    }

    pub fn complete_rollback_after_durable_flush(&mut self, lsn: u64) -> AndromedaResult<()> {
        self.mark_durable_rollback_lsn(lsn)?;
        self.apply(TransactionEvent::RollbackComplete)
    }

    pub fn mark_durable_commit_lsn(&mut self, lsn: u64) -> AndromedaResult<()> {
        if !matches!(self.state, TransactionState::Committing) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "durable commit LSN can only be recorded while committing",
            ));
        }

        if lsn == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "durable commit LSN must not be zero",
            ));
        }

        self.durable_commit_lsn = Some(lsn);
        Ok(())
    }

    pub fn mark_durable_rollback_lsn(&mut self, lsn: u64) -> AndromedaResult<()> {
        if !matches!(self.state, TransactionState::RollingBack) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "durable rollback LSN can only be recorded while rolling back",
            ));
        }

        if lsn == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "durable rollback LSN must not be zero",
            ));
        }

        self.durable_rollback_lsn = Some(lsn);
        Ok(())
    }

    pub const fn is_visible_committed(self) -> bool {
        matches!(self.state, TransactionState::Committed) && self.durable_commit_lsn.is_some()
    }

    pub const fn is_durable_rolled_back(self) -> bool {
        matches!(self.state, TransactionState::RolledBack) && self.durable_rollback_lsn.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_commit_path_requires_durable_wal_before_committed() {
        let mut tx = TransactionStateMachine::new(TransactionId::new(1));

        tx.apply(TransactionEvent::Begin).unwrap();
        tx.apply(TransactionEvent::CommitRequested).unwrap();
        assert_eq!(tx.state, TransactionState::Committing);
        assert_eq!(
            tx.apply(TransactionEvent::DurableWalFlushed)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Transaction
        );

        tx.mark_durable_commit_lsn(42).unwrap();
        tx.apply(TransactionEvent::DurableWalFlushed).unwrap();

        assert!(tx.is_visible_committed());
    }

    #[test]
    fn illegal_commit_visibility_is_rejected_before_wal_flush() {
        let state = TransactionState::Active;
        assert_eq!(
            state
                .apply(TransactionEvent::DurableWalFlushed)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Transaction
        );
    }

    #[test]
    fn poison_path_rolls_back_before_disposal() {
        let rolled_back = TransactionState::Active
            .apply(TransactionEvent::Poison)
            .unwrap()
            .apply(TransactionEvent::RollbackRequested)
            .unwrap()
            .apply(TransactionEvent::RollbackComplete)
            .unwrap();

        assert_eq!(rolled_back, TransactionState::RolledBack);
    }

    #[test]
    fn rollback_completion_requires_durable_wal() {
        let mut tx = TransactionStateMachine::new(TransactionId::new(3));
        tx.apply(TransactionEvent::Begin).unwrap();
        tx.apply(TransactionEvent::RollbackRequested).unwrap();

        assert_eq!(
            tx.apply(TransactionEvent::RollbackComplete)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Transaction
        );

        tx.mark_durable_rollback_lsn(24).unwrap();
        tx.apply(TransactionEvent::RollbackComplete).unwrap();

        assert!(tx.is_durable_rolled_back());
    }
}
