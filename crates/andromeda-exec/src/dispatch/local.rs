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
        if self.has_mutation() && self.mutation_payload.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "mutation payload must exist when rows are affected",
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

        self.wal.append(
            WalRecordKind::TxBegin,
            Some(plan.transaction_id),
            b"tx-begin",
        )?;

        if plan.has_mutation() {
            self.wal.append(
                WalRecordKind::RowUpdate,
                Some(plan.transaction_id),
                &plan.mutation_payload,
            )?;
        }

        tx.apply(TransactionEvent::CommitRequested)?;
        let commit_lsn = self.wal.append(
            WalRecordKind::TxCommit,
            Some(plan.transaction_id),
            b"tx-commit",
        )?;
        let durable_lsn = self.wal.flush_through(commit_lsn)?;
        tx.mark_durable_commit_lsn(durable_lsn.get())?;
        tx.apply(TransactionEvent::DurableWalFlushed)?;

        Ok(LocalDispatchReceipt {
            transaction_id: plan.transaction_id,
            transaction_state: tx.state,
            durable_lsn,
            rows_affected: plan.rows_affected,
        })
    }
}
