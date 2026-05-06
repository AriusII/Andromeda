use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId, TransactionId,
};
use andromeda_observe::TraceId;
use andromeda_storage::Lsn;
use andromeda_tx::TransactionState;
use std::collections::{BTreeMap, btree_map::Entry};

use crate::{CompletionStatus, InvocationCompletion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionJournalRecord {
    pub invocation_id: InvocationId,
    pub transaction_id: Option<TransactionId>,
    pub status: CompletionStatus,
    pub transaction_state: Option<TransactionState>,
    pub rows_affected: Option<u64>,
    pub result_row_count_exact: Option<u64>,
    pub terminal_lsn: Option<Lsn>,
    pub durable_lsn: Option<Lsn>,
    pub trace_id: TraceId,
}

impl CompletionJournalRecord {
    pub fn committed(
        completion: InvocationCompletion,
        transaction_id: TransactionId,
        terminal_lsn: Lsn,
        result_row_count_exact: Option<u64>,
    ) -> AndromedaResult<Self> {
        if completion.status != CompletionStatus::Committed {
            return Err(completion_journal_error(
                "committed journal record requires committed completion status",
            ));
        }

        let record = Self {
            invocation_id: completion.invocation_id,
            transaction_id: Some(transaction_id),
            status: completion.status,
            transaction_state: completion.transaction_state,
            rows_affected: completion.rows_affected,
            result_row_count_exact,
            terminal_lsn: Some(terminal_lsn),
            durable_lsn: completion.durable_lsn,
            trace_id: completion.trace_id,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn rolled_back(
        completion: InvocationCompletion,
        transaction_id: TransactionId,
        terminal_lsn: Lsn,
    ) -> AndromedaResult<Self> {
        if completion.status != CompletionStatus::RolledBack {
            return Err(completion_journal_error(
                "rolled-back journal record requires rolled-back completion status",
            ));
        }

        let record = Self {
            invocation_id: completion.invocation_id,
            transaction_id: Some(transaction_id),
            status: completion.status,
            transaction_state: completion.transaction_state,
            rows_affected: completion.rows_affected,
            result_row_count_exact: Some(0),
            terminal_lsn: Some(terminal_lsn),
            durable_lsn: completion.durable_lsn,
            trace_id: completion.trace_id,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.invocation_id.get() == 0 {
            return Err(completion_journal_error(
                "completion journal invocation id must not be zero",
            ));
        }

        if let Some(transaction_id) = self.transaction_id
            && transaction_id.get() == 0
        {
            return Err(completion_journal_error(
                "completion journal transaction id must not be zero when present",
            ));
        }

        match self.status {
            CompletionStatus::Committed => {
                if self.transaction_id.is_none() {
                    return Err(completion_journal_error(
                        "committed journal record requires transaction id",
                    ));
                }
                if self.transaction_state != Some(TransactionState::Committed) {
                    return Err(completion_journal_error(
                        "committed journal record requires committed transaction state",
                    ));
                }
                if self.rows_affected.is_none() {
                    return Err(completion_journal_error(
                        "committed journal record requires rows affected metadata",
                    ));
                }
                self.validate_terminal_lsn("committed journal record")?;
            }
            CompletionStatus::RolledBack => {
                if self.transaction_id.is_none() {
                    return Err(completion_journal_error(
                        "rolled-back journal record requires transaction id",
                    ));
                }
                if self.transaction_state != Some(TransactionState::RolledBack) {
                    return Err(completion_journal_error(
                        "rolled-back journal record requires rolled-back transaction state",
                    ));
                }
                if self.rows_affected != Some(0) {
                    return Err(completion_journal_error(
                        "rolled-back journal record must report zero rows affected",
                    ));
                }
                self.validate_terminal_lsn("rolled-back journal record")?;
            }
            CompletionStatus::FailedBeforeTransaction
            | CompletionStatus::Cancelled
            | CompletionStatus::Poisoned
            | CompletionStatus::PermissionDenied
            | CompletionStatus::ContractRejected
            | CompletionStatus::SystemUnavailable => {
                if self.transaction_id.is_some()
                    || self.transaction_state.is_some()
                    || self.rows_affected.is_some()
                    || self.result_row_count_exact.is_some()
                    || self.terminal_lsn.is_some()
                    || self.durable_lsn.is_some()
                {
                    return Err(completion_journal_error(
                        "non-transactional journal record must not carry transaction or result evidence",
                    ));
                }
            }
        }

        Ok(())
    }

    fn validate_terminal_lsn(self, label: &str) -> AndromedaResult<()> {
        let Some(terminal_lsn) = self.terminal_lsn else {
            return Err(completion_journal_error(format!(
                "{label} requires terminal WAL LSN evidence"
            )));
        };
        let Some(durable_lsn) = self.durable_lsn else {
            return Err(completion_journal_error(format!(
                "{label} requires durable WAL LSN evidence"
            )));
        };

        if terminal_lsn.is_zero() || durable_lsn.is_zero() {
            return Err(completion_journal_error(format!(
                "{label} requires nonzero WAL LSN evidence"
            )));
        }

        if durable_lsn < terminal_lsn {
            return Err(completion_journal_error(format!(
                "{label} durable LSN must cover terminal WAL LSN"
            )));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct InvocationCompletionJournal {
    by_invocation: BTreeMap<InvocationId, CompletionJournalRecord>,
    invocation_by_transaction: BTreeMap<TransactionId, InvocationId>,
}

impl InvocationCompletionJournal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(
        &mut self,
        record: CompletionJournalRecord,
    ) -> AndromedaResult<&CompletionJournalRecord> {
        record.validate()?;
        let invocation_id = record.invocation_id;
        let transaction_id = record.transaction_id;

        if self.by_invocation.contains_key(&invocation_id) {
            return Err(completion_journal_error(
                "completion journal already contains invocation completion",
            ));
        }

        if transaction_id.is_some_and(|transaction_id| {
            self.invocation_by_transaction.contains_key(&transaction_id)
        }) {
            return Err(completion_journal_error(
                "completion journal already contains transaction completion",
            ));
        }

        match self.by_invocation.entry(invocation_id) {
            Entry::Occupied(_) => Err(completion_journal_error(
                "completion journal already contains invocation completion",
            )),
            Entry::Vacant(entry) => {
                if let Some(transaction_id) = transaction_id {
                    self.invocation_by_transaction
                        .insert(transaction_id, invocation_id);
                }
                Ok(entry.insert(record))
            }
        }
    }

    pub fn get(&self, invocation_id: InvocationId) -> Option<&CompletionJournalRecord> {
        self.by_invocation.get(&invocation_id)
    }

    pub fn get_by_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Option<&CompletionJournalRecord> {
        self.invocation_by_transaction
            .get(&transaction_id)
            .and_then(|invocation_id| self.by_invocation.get(invocation_id))
    }

    pub fn len(&self) -> usize {
        self.by_invocation.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_invocation.is_empty()
    }
}

pub(super) fn completion_journal_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Execution, message)
}
