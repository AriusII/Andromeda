use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_storage::{Lsn, WalRecordKind};
use andromeda_tx::{IsolationLevel, TransactionEvent, TransactionState, TransactionStateMachine};

use crate::{
    InvocationWal, LocalHeapRowInsertRedoTemplate, encode_exec_tx_commit_payload,
    encode_exec_tx_rollback_payload,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDispatchPlan {
    pub transaction_id: TransactionId,
    pub mutation_payload: Vec<u8>,
    pub rows_affected: u64,
}

impl LocalDispatchPlan {
    pub fn has_mutation(&self) -> bool {
        self.rows_affected > 0
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.transaction_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "dispatch transaction id must not be zero",
            ));
        }

        if self.has_mutation() && self.mutation_payload.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "mutation payload must exist when rows are affected",
            ));
        }

        if self.has_mutation() {
            LocalHeapRowInsertRedoTemplate::try_decode_template(&self.mutation_payload)?;
        }

        Ok(())
    }

    fn mutation_record_for_lsn(
        &self,
        mutation_lsn: Lsn,
    ) -> AndromedaResult<(WalRecordKind, Vec<u8>, bool)> {
        if let Some(template) =
            LocalHeapRowInsertRedoTemplate::try_decode_template(&self.mutation_payload)?
        {
            return Ok((
                WalRecordKind::RowInsert,
                template.materialize_wal_payload(mutation_lsn)?,
                true,
            ));
        }

        Ok((
            WalRecordKind::RowUpdate,
            self.mutation_payload.clone(),
            false,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRollbackPlan {
    pub transaction_id: TransactionId,
    pub rollback_payload: Vec<u8>,
}

impl LocalRollbackPlan {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.transaction_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "rollback transaction id must not be zero",
            ));
        }

        if self.rollback_payload.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "rollback payload must explain rollback cause",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalDurabilityEvidence {
    pub begin_lsn: Lsn,
    pub mutation_lsn: Option<Lsn>,
    pub commit_lsn: Lsn,
    pub durable_lsn: Lsn,
}

impl WalDurabilityEvidence {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.durable_lsn < self.commit_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "commit WAL flush did not reach commit LSN",
            ));
        }

        if let Some(mutation_lsn) = self.mutation_lsn
            && (mutation_lsn <= self.begin_lsn || mutation_lsn >= self.commit_lsn)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "mutation WAL LSN must be between begin and commit",
            ));
        }

        if self.begin_lsn >= self.commit_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "begin WAL LSN must precede commit LSN",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollbackWalDurabilityEvidence {
    pub begin_lsn: Lsn,
    pub rollback_lsn: Lsn,
    pub durable_lsn: Lsn,
}

impl RollbackWalDurabilityEvidence {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.durable_lsn < self.rollback_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "rollback WAL flush did not reach rollback LSN",
            ));
        }

        if self.begin_lsn >= self.rollback_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "begin WAL LSN must precede rollback LSN",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDispatchReceipt {
    pub transaction_id: TransactionId,
    pub transaction_state: TransactionState,
    pub durable_lsn: Lsn,
    pub rows_affected: u64,
    pub wal_evidence: WalDurabilityEvidence,
}

/// Reason a transaction is being rolled back.
///
/// Routes the dispatcher through the correct intermediate state on the
/// transaction state machine before the durable rollback record is appended.
///
/// * `Direct` — caller-driven rollback: `Active -> RollingBack`.
/// * `BusinessFailure` — runtime business/procedure failure that did not
///   poison the transaction: `Active -> Failed -> RollingBack`.
/// * `Poison` — the transaction must be quarantined before rollback because
///   continued use would violate engine invariants: `Active -> Poisoned ->
///   RollingBack`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackCause {
    Direct,
    BusinessFailure,
    Poison,
}

impl RollbackCause {
    pub const fn intermediate_state(self) -> Option<TransactionState> {
        match self {
            Self::Direct => None,
            Self::BusinessFailure => Some(TransactionState::Failed),
            Self::Poison => Some(TransactionState::Poisoned),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalRollbackReceipt {
    pub transaction_id: TransactionId,
    pub transaction_state: TransactionState,
    pub durable_lsn: Lsn,
    pub wal_evidence: RollbackWalDurabilityEvidence,
    /// State machine transition that preceded `RollingBack`, when the
    /// rollback was driven by an explicit `Failed`/`Poisoned` route. `None`
    /// for direct rollbacks that move straight from `Active` into
    /// `RollingBack`.
    pub intermediate_state: Option<TransactionState>,
    pub cause: RollbackCause,
}

pub struct LocalDispatcher<'a, W> {
    wal: &'a mut W,
}

impl<'a, W> LocalDispatcher<'a, W>
where
    W: InvocationWal,
{
    pub fn new(wal: &'a mut W) -> Self {
        Self { wal }
    }

    pub fn dispatch_commit(
        &mut self,
        plan: LocalDispatchPlan,
    ) -> AndromedaResult<LocalDispatchReceipt> {
        plan.validate()?;

        let mut tx = TransactionStateMachine::new(plan.transaction_id);
        tx.apply(TransactionEvent::Begin)?;

        let begin_lsn = self.wal.append(
            WalRecordKind::TxBegin,
            Some(plan.transaction_id),
            b"tx-begin",
        )?;

        let mutation_lsn = if plan.has_mutation() {
            let expected_mutation_lsn = begin_lsn.try_next()?;
            let (mutation_kind, mutation_payload, requires_exact_lsn) =
                plan.mutation_record_for_lsn(expected_mutation_lsn)?;
            let mutation_lsn =
                self.wal
                    .append(mutation_kind, Some(plan.transaction_id), &mutation_payload)?;
            if requires_exact_lsn && mutation_lsn != expected_mutation_lsn {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!(
                        "local storage redo payload was materialized for mutation LSN {} but WAL appended it at {}",
                        expected_mutation_lsn.get(),
                        mutation_lsn.get()
                    ),
                ));
            }
            Some(mutation_lsn)
        } else {
            None
        };

        tx.apply(TransactionEvent::CommitRequested)?;
        let commit_payload =
            encode_exec_tx_commit_payload(IsolationLevel::Serializable, plan.rows_affected, 0);
        let commit_lsn = self.wal.append(
            WalRecordKind::TxCommit,
            Some(plan.transaction_id),
            &commit_payload,
        )?;
        let durable_lsn = self.wal.flush_through(commit_lsn)?;
        let wal_evidence = WalDurabilityEvidence {
            begin_lsn,
            mutation_lsn,
            commit_lsn,
            durable_lsn,
        };
        wal_evidence.validate()?;
        tx.publish_visible_commit_with_durable_evidence(commit_lsn.get(), durable_lsn.get())?;

        Ok(LocalDispatchReceipt {
            transaction_id: plan.transaction_id,
            transaction_state: tx.state(),
            durable_lsn,
            rows_affected: plan.rows_affected,
            wal_evidence,
        })
    }

    pub fn dispatch_rollback(
        &mut self,
        plan: LocalRollbackPlan,
    ) -> AndromedaResult<LocalRollbackReceipt> {
        self.dispatch_rollback_with_cause(plan, RollbackCause::Direct)
    }

    /// Drive a durable rollback while routing the transaction state machine
    /// through the failure/poison intermediate state implied by `cause`.
    ///
    /// The same WAL record sequence is appended for every cause
    /// (`TxBegin` followed by `TxRollback`); the difference is purely the
    /// transaction state machine path so that downstream evidence can prove
    /// runtime failures did not skip the failed/poisoned states before a
    /// `RolledBack` completion is emitted.
    pub fn dispatch_rollback_with_cause(
        &mut self,
        plan: LocalRollbackPlan,
        cause: RollbackCause,
    ) -> AndromedaResult<LocalRollbackReceipt> {
        plan.validate()?;

        let mut tx = TransactionStateMachine::new(plan.transaction_id);
        tx.apply(TransactionEvent::Begin)?;

        let begin_lsn = self.wal.append(
            WalRecordKind::TxBegin,
            Some(plan.transaction_id),
            b"tx-begin",
        )?;

        let intermediate_state = match cause {
            RollbackCause::Direct => None,
            RollbackCause::BusinessFailure => {
                tx.apply(TransactionEvent::Fail)?;
                Some(TransactionState::Failed)
            }
            RollbackCause::Poison => {
                tx.apply(TransactionEvent::Poison)?;
                Some(TransactionState::Poisoned)
            }
        };

        tx.apply(TransactionEvent::RollbackRequested)?;
        let rollback_payload = encode_exec_tx_rollback_payload(0);
        let rollback_lsn = self.wal.append(
            WalRecordKind::TxRollback,
            Some(plan.transaction_id),
            &rollback_payload,
        )?;
        let durable_lsn = self.wal.flush_through(rollback_lsn)?;
        let wal_evidence = RollbackWalDurabilityEvidence {
            begin_lsn,
            rollback_lsn,
            durable_lsn,
        };
        wal_evidence.validate()?;
        tx.complete_rollback_with_durable_evidence(rollback_lsn.get(), durable_lsn.get())?;

        Ok(LocalRollbackReceipt {
            transaction_id: plan.transaction_id,
            transaction_state: tx.state(),
            durable_lsn,
            wal_evidence,
            intermediate_state,
            cause,
        })
    }
}
