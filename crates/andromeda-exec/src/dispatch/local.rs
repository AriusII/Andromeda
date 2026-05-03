use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_storage::{Lsn, WalRecordKind};
use andromeda_tx::{TransactionEvent, TransactionState, TransactionStateMachine};

use crate::InvocationWal;

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

        Ok(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalRollbackReceipt {
    pub transaction_id: TransactionId,
    pub transaction_state: TransactionState,
    pub durable_lsn: Lsn,
    pub wal_evidence: RollbackWalDurabilityEvidence,
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
            Some(self.wal.append(
                WalRecordKind::RowUpdate,
                Some(plan.transaction_id),
                &plan.mutation_payload,
            )?)
        } else {
            None
        };

        tx.apply(TransactionEvent::CommitRequested)?;
        let commit_lsn = self.wal.append(
            WalRecordKind::TxCommit,
            Some(plan.transaction_id),
            b"tx-commit",
        )?;
        let durable_lsn = self.wal.flush_through(commit_lsn)?;
        let wal_evidence = WalDurabilityEvidence {
            begin_lsn,
            mutation_lsn,
            commit_lsn,
            durable_lsn,
        };
        wal_evidence.validate()?;
        tx.mark_durable_commit_lsn(durable_lsn.get())?;
        tx.apply(TransactionEvent::DurableWalFlushed)?;

        Ok(LocalDispatchReceipt {
            transaction_id: plan.transaction_id,
            transaction_state: tx.state,
            durable_lsn,
            rows_affected: plan.rows_affected,
            wal_evidence,
        })
    }

    pub fn dispatch_rollback(
        &mut self,
        plan: LocalRollbackPlan,
    ) -> AndromedaResult<LocalRollbackReceipt> {
        plan.validate()?;

        let mut tx = TransactionStateMachine::new(plan.transaction_id);
        tx.apply(TransactionEvent::Begin)?;

        let begin_lsn = self.wal.append(
            WalRecordKind::TxBegin,
            Some(plan.transaction_id),
            b"tx-begin",
        )?;

        tx.apply(TransactionEvent::RollbackRequested)?;
        let rollback_lsn = self.wal.append(
            WalRecordKind::TxRollback,
            Some(plan.transaction_id),
            &plan.rollback_payload,
        )?;
        let durable_lsn = self.wal.flush_through(rollback_lsn)?;
        let wal_evidence = RollbackWalDurabilityEvidence {
            begin_lsn,
            rollback_lsn,
            durable_lsn,
        };
        wal_evidence.validate()?;
        tx.mark_durable_rollback_lsn(durable_lsn.get())?;
        tx.apply(TransactionEvent::RollbackComplete)?;

        Ok(LocalRollbackReceipt {
            transaction_id: plan.transaction_id,
            transaction_state: tx.state,
            durable_lsn,
            wal_evidence,
        })
    }
}
