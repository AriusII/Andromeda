use andromeda_core::{AndromedaResult, InvocationId, TransactionId};
use andromeda_storage::{
    summarize_transactions_from_records, DurableTransactionState, Lsn, WalRecord,
};

use crate::CompletionStatus;

use super::journal::{completion_journal_error, CompletionJournalRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionRecoveryExpectation {
    pub invocation_id: InvocationId,
    pub transaction_id: TransactionId,
    pub expected_rows_affected: Option<u64>,
    pub expected_result_row_count_exact: Option<u64>,
    pub journal_record: Option<CompletionJournalRecord>,
}

impl CompletionRecoveryExpectation {
    /// Construct a recovery expectation from an explicit recovery-safe
    /// `TransactionId`. This is the preferred constructor for production
    /// recovery code: the caller is expected to feed in the id observed in
    /// the durable WAL (or allocated through
    /// [`andromeda_tx::TransactionManager`]) so the expectation matches the
    /// id the dispatcher actually stamped on each record.
    pub fn for_invocation_with_transaction(
        invocation_id: InvocationId,
        transaction_id: TransactionId,
    ) -> Self {
        Self {
            invocation_id,
            transaction_id,
            expected_rows_affected: None,
            expected_result_row_count_exact: None,
            journal_record: None,
        }
    }

    /// Legacy constructor that derives the expected `TransactionId` from the
    /// `InvocationId` via the deprecated
    /// [`crate::transaction_id_for_invocation`] shim.
    ///
    /// **Test/compatibility only.** Production recovery callers must use
    /// [`Self::for_invocation_with_transaction`]: deriving the id from the
    /// invocation namespace breaks monotonicity across restarts and is
    /// incompatible with [`andromeda_tx::TransactionManager`] allocation.
    #[allow(deprecated)]
    pub fn for_invocation(invocation_id: InvocationId) -> Self {
        Self::for_invocation_with_transaction(
            invocation_id,
            crate::transaction_id_for_invocation(invocation_id),
        )
    }

    pub fn with_expected_metadata(
        mut self,
        rows_affected: Option<u64>,
        result_row_count_exact: Option<u64>,
    ) -> Self {
        self.expected_rows_affected = rows_affected;
        self.expected_result_row_count_exact = result_row_count_exact;
        self
    }

    pub fn with_journal_record(mut self, journal_record: CompletionJournalRecord) -> Self {
        self.journal_record = Some(journal_record);
        self
    }

    pub(super) fn validate(self) -> AndromedaResult<()> {
        if self.invocation_id.get() == 0 {
            return Err(completion_journal_error(
                "completion recovery expectation invocation id must not be zero",
            ));
        }
        if self.transaction_id.get() == 0 {
            return Err(completion_journal_error(
                "completion recovery expectation transaction id must not be zero",
            ));
        }
        if let Some(journal_record) = self.journal_record {
            journal_record.validate()?;
            if journal_record.invocation_id != self.invocation_id
                || journal_record.transaction_id != self.transaction_id
            {
                return Err(completion_journal_error(
                    "completion recovery expectation journal identity mismatch",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionRecoveryStatus {
    Completed,
    RolledBack,
    Incomplete,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionRecoveryAmbiguity {
    MissingWalEvidence,
    ConflictingTerminalWalEvidence,
    JournalContradictsWal,
    RowsAffectedMismatch,
    ResultRowCountMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionRecoveryRecord {
    pub invocation_id: InvocationId,
    pub transaction_id: TransactionId,
    pub status: CompletionRecoveryStatus,
    pub transaction_state: Option<DurableTransactionState>,
    pub terminal_lsn: Option<Lsn>,
    pub durable_lsn: Lsn,
    pub rows_affected: Option<u64>,
    pub result_row_count_exact: Option<u64>,
    pub ambiguity: Option<CompletionRecoveryAmbiguity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRecoveryReport {
    pub durable_lsn: Lsn,
    pub records: Vec<CompletionRecoveryRecord>,
}

impl CompletionRecoveryReport {
    pub fn completed(&self) -> impl Iterator<Item = &CompletionRecoveryRecord> + '_ {
        self.records
            .iter()
            .filter(|record| record.status == CompletionRecoveryStatus::Completed)
    }

    pub fn ambiguous(&self) -> impl Iterator<Item = &CompletionRecoveryRecord> + '_ {
        self.records
            .iter()
            .filter(|record| record.status == CompletionRecoveryStatus::Ambiguous)
    }

    pub fn has_ambiguity(&self) -> bool {
        self.ambiguous().next().is_some()
    }
}

pub fn reconcile_completion_recovery_from_wal(
    durable_records: &[WalRecord],
    expectations: &[CompletionRecoveryExpectation],
) -> AndromedaResult<CompletionRecoveryReport> {
    for record in durable_records {
        record.validate()?;
    }
    for expectation in expectations {
        expectation.validate()?;
    }

    let durable_lsn = durable_records
        .iter()
        .map(|record| record.header.lsn)
        .max()
        .unwrap_or_default();
    let transaction_summaries = summarize_transactions_from_records(durable_records);

    let records = expectations
        .iter()
        .map(|expectation| {
            let summary = transaction_summaries
                .iter()
                .find(|summary| summary.transaction_id == expectation.transaction_id);

            match summary {
                Some(summary) if summary.commit_lsn.is_some() && summary.rollback_lsn.is_some() => {
                    completion_recovery_ambiguous(
                        *expectation,
                        durable_lsn,
                        Some(DurableTransactionState::Incomplete),
                        summary.last_lsn,
                        CompletionRecoveryAmbiguity::ConflictingTerminalWalEvidence,
                    )
                }
                Some(summary) if summary.state == DurableTransactionState::Committed => {
                    completion_recovery_committed(*expectation, durable_lsn, summary.commit_lsn)
                }
                Some(summary) if summary.state == DurableTransactionState::RolledBack => {
                    completion_recovery_rolled_back(*expectation, durable_lsn, summary.rollback_lsn)
                }
                Some(summary) => {
                    if let Some(journal_record) = expectation.journal_record {
                        if matches!(
                            journal_record.status,
                            CompletionStatus::Committed | CompletionStatus::RolledBack
                        ) {
                            completion_recovery_ambiguous(
                                *expectation,
                                durable_lsn,
                                Some(summary.state),
                                summary.last_lsn,
                                CompletionRecoveryAmbiguity::JournalContradictsWal,
                            )
                        } else {
                            CompletionRecoveryRecord {
                                invocation_id: expectation.invocation_id,
                                transaction_id: expectation.transaction_id,
                                status: CompletionRecoveryStatus::Incomplete,
                                transaction_state: Some(summary.state),
                                terminal_lsn: None,
                                durable_lsn,
                                rows_affected: None,
                                result_row_count_exact: None,
                                ambiguity: None,
                            }
                        }
                    } else {
                        CompletionRecoveryRecord {
                            invocation_id: expectation.invocation_id,
                            transaction_id: expectation.transaction_id,
                            status: CompletionRecoveryStatus::Incomplete,
                            transaction_state: Some(summary.state),
                            terminal_lsn: None,
                            durable_lsn,
                            rows_affected: None,
                            result_row_count_exact: None,
                            ambiguity: None,
                        }
                    }
                }
                None => completion_recovery_ambiguous(
                    *expectation,
                    durable_lsn,
                    None,
                    Lsn::ZERO,
                    CompletionRecoveryAmbiguity::MissingWalEvidence,
                ),
            }
        })
        .collect();

    Ok(CompletionRecoveryReport {
        durable_lsn,
        records,
    })
}

fn completion_recovery_committed(
    expectation: CompletionRecoveryExpectation,
    durable_lsn: Lsn,
    commit_lsn: Option<Lsn>,
) -> CompletionRecoveryRecord {
    if let Some(journal_record) = expectation.journal_record {
        if journal_record.status != CompletionStatus::Committed {
            return completion_recovery_ambiguous(
                expectation,
                durable_lsn,
                Some(DurableTransactionState::Committed),
                commit_lsn.unwrap_or_default(),
                CompletionRecoveryAmbiguity::JournalContradictsWal,
            );
        }
    }
    if let Some(journal_record) = expectation.journal_record {
        if journal_record.terminal_lsn != commit_lsn
            || matches!(journal_record.durable_lsn, Some(journal_durable_lsn) if journal_durable_lsn > durable_lsn)
        {
            return completion_recovery_ambiguous(
                expectation,
                durable_lsn,
                Some(DurableTransactionState::Committed),
                commit_lsn.unwrap_or_default(),
                CompletionRecoveryAmbiguity::JournalContradictsWal,
            );
        }
    }

    let rows_affected = expectation
        .journal_record
        .and_then(|record| record.rows_affected)
        .or(expectation.expected_rows_affected);
    let result_row_count_exact = expectation
        .journal_record
        .and_then(|record| record.result_row_count_exact)
        .or(expectation.expected_result_row_count_exact);

    if let Some(journal_record) = expectation.journal_record {
        if let (Some(journal_rows), Some(expected_rows)) = (
            journal_record.rows_affected,
            expectation.expected_rows_affected,
        ) {
            if journal_rows != expected_rows {
                return completion_recovery_ambiguous(
                    expectation,
                    durable_lsn,
                    Some(DurableTransactionState::Committed),
                    commit_lsn.unwrap_or_default(),
                    CompletionRecoveryAmbiguity::RowsAffectedMismatch,
                );
            }
        }

        if let (Some(journal_rows), Some(expected_rows)) = (
            journal_record.result_row_count_exact,
            expectation.expected_result_row_count_exact,
        ) {
            if journal_rows != expected_rows {
                return completion_recovery_ambiguous(
                    expectation,
                    durable_lsn,
                    Some(DurableTransactionState::Committed),
                    commit_lsn.unwrap_or_default(),
                    CompletionRecoveryAmbiguity::ResultRowCountMismatch,
                );
            }
        }
    }

    CompletionRecoveryRecord {
        invocation_id: expectation.invocation_id,
        transaction_id: expectation.transaction_id,
        status: CompletionRecoveryStatus::Completed,
        transaction_state: Some(DurableTransactionState::Committed),
        terminal_lsn: commit_lsn,
        durable_lsn,
        rows_affected,
        result_row_count_exact,
        ambiguity: None,
    }
}

fn completion_recovery_rolled_back(
    expectation: CompletionRecoveryExpectation,
    durable_lsn: Lsn,
    rollback_lsn: Option<Lsn>,
) -> CompletionRecoveryRecord {
    if let Some(journal_record) = expectation.journal_record {
        if journal_record.status != CompletionStatus::RolledBack {
            return completion_recovery_ambiguous(
                expectation,
                durable_lsn,
                Some(DurableTransactionState::RolledBack),
                rollback_lsn.unwrap_or_default(),
                CompletionRecoveryAmbiguity::JournalContradictsWal,
            );
        }
    }
    if let Some(journal_record) = expectation.journal_record {
        if journal_record.terminal_lsn != rollback_lsn
            || matches!(journal_record.durable_lsn, Some(journal_durable_lsn) if journal_durable_lsn > durable_lsn)
        {
            return completion_recovery_ambiguous(
                expectation,
                durable_lsn,
                Some(DurableTransactionState::RolledBack),
                rollback_lsn.unwrap_or_default(),
                CompletionRecoveryAmbiguity::JournalContradictsWal,
            );
        }
    }

    CompletionRecoveryRecord {
        invocation_id: expectation.invocation_id,
        transaction_id: expectation.transaction_id,
        status: CompletionRecoveryStatus::RolledBack,
        transaction_state: Some(DurableTransactionState::RolledBack),
        terminal_lsn: rollback_lsn,
        durable_lsn,
        rows_affected: Some(0),
        result_row_count_exact: Some(0),
        ambiguity: None,
    }
}

fn completion_recovery_ambiguous(
    expectation: CompletionRecoveryExpectation,
    durable_lsn: Lsn,
    transaction_state: Option<DurableTransactionState>,
    terminal_lsn: Lsn,
    ambiguity: CompletionRecoveryAmbiguity,
) -> CompletionRecoveryRecord {
    CompletionRecoveryRecord {
        invocation_id: expectation.invocation_id,
        transaction_id: expectation.transaction_id,
        status: CompletionRecoveryStatus::Ambiguous,
        transaction_state,
        terminal_lsn: (!terminal_lsn.is_zero()).then_some(terminal_lsn),
        durable_lsn,
        rows_affected: None,
        result_row_count_exact: None,
        ambiguity: Some(ambiguity),
    }
}
