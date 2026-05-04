use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId, TransactionId,
};
use andromeda_observe::TraceId;
use andromeda_storage::{
    summarize_transactions_from_records, DurableTransactionState, Lsn, WalRecord,
};
use andromeda_tx::TransactionState;
use std::collections::BTreeMap;

use crate::{CompletionStatus, InvocationCompletion};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompletionMappingService;

impl CompletionMappingService {
    pub fn committed(
        invocation_id: InvocationId,
        rows_affected: u64,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        if transaction_state != TransactionState::Committed {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "committed completion requires committed transaction state",
            ));
        }

        if durable_lsn.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "committed completion requires nonzero durable LSN evidence",
            ));
        }

        Ok(InvocationCompletion {
            invocation_id,
            status: CompletionStatus::Committed,
            rows_affected: Some(rows_affected),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        })
    }

    pub fn rolled_back(
        invocation_id: InvocationId,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        if transaction_state != TransactionState::RolledBack {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "rolled-back completion requires rolled-back transaction state",
            ));
        }

        if durable_lsn.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "rolled-back completion requires nonzero durable LSN evidence",
            ));
        }

        Ok(InvocationCompletion {
            invocation_id,
            status: CompletionStatus::RolledBack,
            rows_affected: Some(0),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        })
    }

    pub fn rejected(
        invocation_id: InvocationId,
        status: CompletionStatus,
        trace_id: TraceId,
    ) -> InvocationCompletion {
        InvocationCompletion {
            invocation_id,
            status,
            rows_affected: None,
            transaction_state: None,
            durable_lsn: None,
            trace_id,
        }
    }

    /// Build a `Poisoned` completion that has been routed through a durable
    /// rollback. Mirrors the executor invariant that poison failures must
    /// transit `TransactionState::Poisoned` before reaching durable
    /// `RolledBack`. Callers MUST supply the rolled-back transaction state
    /// and the durable WAL LSN proving the rollback is durable. Use
    /// `rejected` instead for poison failures detected before any
    /// transaction begin.
    pub fn poisoned_after_rollback(
        invocation_id: InvocationId,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        trace_id: TraceId,
    ) -> AndromedaResult<InvocationCompletion> {
        if transaction_state != TransactionState::RolledBack {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "poisoned-after-rollback completion requires rolled-back transaction state",
            ));
        }
        if durable_lsn.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "poisoned-after-rollback completion requires nonzero durable LSN evidence",
            ));
        }
        Ok(InvocationCompletion {
            invocation_id,
            status: CompletionStatus::Poisoned,
            rows_affected: Some(0),
            transaction_state: Some(transaction_state),
            durable_lsn: Some(durable_lsn),
            trace_id,
        })
    }

    /// Build a `FailedBeforeTransaction` completion with no transaction
    /// evidence. This is the only legal projection of a runtime failure that
    /// occurred prior to `Begin`; failures observed after `Begin` MUST be
    /// routed through the rolled-back path so the transaction terminal
    /// invariant is preserved.
    pub fn failed_before_transaction(
        invocation_id: InvocationId,
        trace_id: TraceId,
    ) -> InvocationCompletion {
        InvocationCompletion {
            invocation_id,
            status: CompletionStatus::FailedBeforeTransaction,
            rows_affected: None,
            transaction_state: None,
            durable_lsn: None,
            trace_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionJournalRecord {
    pub invocation_id: InvocationId,
    pub transaction_id: TransactionId,
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
            transaction_id,
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
            transaction_id,
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

        if self.transaction_id.get() == 0 {
            return Err(completion_journal_error(
                "completion journal transaction id must not be zero",
            ));
        }

        match self.status {
            CompletionStatus::Committed => {
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
                if self.transaction_state.is_some()
                    || self.rows_affected.is_some()
                    || self.terminal_lsn.is_some()
                    || self.durable_lsn.is_some()
                {
                    return Err(completion_journal_error(
                        "non-transactional journal record must not carry transaction evidence",
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

        if self.by_invocation.contains_key(&record.invocation_id) {
            return Err(completion_journal_error(
                "completion journal already contains invocation completion",
            ));
        }

        if self
            .invocation_by_transaction
            .contains_key(&record.transaction_id)
        {
            return Err(completion_journal_error(
                "completion journal already contains transaction completion",
            ));
        }

        self.invocation_by_transaction
            .insert(record.transaction_id, record.invocation_id);
        self.by_invocation.insert(record.invocation_id, record);
        Ok(self
            .by_invocation
            .get(&record.invocation_id)
            .expect("inserted completion journal record must be visible"))
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

    fn validate(self) -> AndromedaResult<()> {
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

fn completion_journal_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Execution, message)
}

#[cfg(test)]
#[allow(deprecated)]
// The tests below intentionally exercise the deprecated
// `transaction_id_for_invocation` shim and the
// `CompletionRecoveryExpectation::for_invocation` legacy constructor: they
// hand-craft WAL records keyed by InvocationId-derived TransactionIds to
// exercise reconciliation without spinning up a TransactionManager. These
// suites stay pinned to that shape so the recovery proofs remain
// byte-stable; production paths must continue to use
// `TransactionManager::begin` and
// `CompletionRecoveryExpectation::for_invocation_with_transaction`.
mod tests {
    use super::*;
    use andromeda_core::InvocationId;
    use andromeda_storage::{InMemoryWal, WalRecordKind};

    fn append_committed_invocation(
        wal: &mut InMemoryWal,
        invocation_id: InvocationId,
        rows_payload: &[u8],
    ) -> (TransactionId, Lsn) {
        let transaction_id = crate::transaction_id_for_invocation(invocation_id);
        wal.append_tx_begin(transaction_id).unwrap();
        if !rows_payload.is_empty() {
            wal.append_payload(WalRecordKind::RowUpdate, Some(transaction_id), rows_payload)
                .unwrap();
        }
        let commit_lsn = wal.append_tx_commit(transaction_id).unwrap();
        wal.flush_through(commit_lsn).unwrap();
        (transaction_id, commit_lsn)
    }

    #[test]
    fn completion_journal_rejects_duplicate_completion_without_overwrite() {
        let invocation_id = InvocationId::new(40);
        let transaction_id = crate::transaction_id_for_invocation(invocation_id);
        let completion = CompletionMappingService::committed(
            invocation_id,
            1,
            TransactionState::Committed,
            Lsn::new(3),
            TraceId::new(400),
        )
        .unwrap();
        let record =
            CompletionJournalRecord::committed(completion, transaction_id, Lsn::new(3), Some(1))
                .unwrap();
        let mut journal = InvocationCompletionJournal::new();

        journal.record(record).unwrap();
        let err = journal.record(record).unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Execution);
        assert_eq!(journal.len(), 1);
        assert_eq!(journal.get(invocation_id), Some(&record));
    }

    #[test]
    fn completion_recovery_marks_crash_after_mutation_as_incomplete() {
        let invocation_id = InvocationId::new(41);
        let transaction_id = crate::transaction_id_for_invocation(invocation_id);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(transaction_id).unwrap();
        let mutation_lsn = wal
            .append_payload(WalRecordKind::RowUpdate, Some(transaction_id), b"mutated")
            .unwrap();
        wal.flush_through(mutation_lsn).unwrap();

        let report = reconcile_completion_recovery_from_wal(
            &wal.replay_durable(),
            &[CompletionRecoveryExpectation::for_invocation(invocation_id)
                .with_expected_metadata(Some(1), Some(1))],
        )
        .unwrap();

        assert_eq!(report.records.len(), 1);
        assert_eq!(
            report.records[0].status,
            CompletionRecoveryStatus::Incomplete
        );
        assert_eq!(
            report.records[0].transaction_state,
            Some(DurableTransactionState::Open)
        );
        assert!(!report.has_ambiguity());
    }

    #[test]
    fn completion_recovery_reconstructs_crash_after_commit_before_completion() {
        let invocation_id = InvocationId::new(42);
        let mut wal = InMemoryWal::new();
        let (_transaction_id, commit_lsn) =
            append_committed_invocation(&mut wal, invocation_id, b"committed-before-completion");

        let report = reconcile_completion_recovery_from_wal(
            &wal.replay_durable(),
            &[CompletionRecoveryExpectation::for_invocation(invocation_id)
                .with_expected_metadata(Some(1), Some(1))],
        )
        .unwrap();

        assert_eq!(
            report.records[0].status,
            CompletionRecoveryStatus::Completed
        );
        assert_eq!(
            report.records[0].transaction_state,
            Some(DurableTransactionState::Committed)
        );
        assert_eq!(report.records[0].terminal_lsn, Some(commit_lsn));
        assert_eq!(report.records[0].rows_affected, Some(1));
        assert_eq!(report.records[0].result_row_count_exact, Some(1));
    }

    #[test]
    fn completion_recovery_validates_committed_journal_metadata() {
        let invocation_id = InvocationId::new(43);
        let mut wal = InMemoryWal::new();
        let (transaction_id, commit_lsn) =
            append_committed_invocation(&mut wal, invocation_id, b"committed");
        let completion = CompletionMappingService::committed(
            invocation_id,
            2,
            TransactionState::Committed,
            commit_lsn,
            TraceId::new(430),
        )
        .unwrap();
        let journal_record =
            CompletionJournalRecord::committed(completion, transaction_id, commit_lsn, Some(1))
                .unwrap();

        let report = reconcile_completion_recovery_from_wal(
            &wal.replay_durable(),
            &[CompletionRecoveryExpectation::for_invocation(invocation_id)
                .with_expected_metadata(Some(2), Some(1))
                .with_journal_record(journal_record)],
        )
        .unwrap();

        assert_eq!(
            report.records[0].status,
            CompletionRecoveryStatus::Completed
        );
        assert_eq!(report.records[0].rows_affected, Some(2));
        assert_eq!(report.records[0].result_row_count_exact, Some(1));
    }

    #[test]
    fn completion_recovery_surfaces_journal_metadata_mismatch_as_ambiguous() {
        let invocation_id = InvocationId::new(44);
        let mut wal = InMemoryWal::new();
        let (transaction_id, commit_lsn) =
            append_committed_invocation(&mut wal, invocation_id, b"committed");
        let completion = CompletionMappingService::committed(
            invocation_id,
            1,
            TransactionState::Committed,
            commit_lsn,
            TraceId::new(440),
        )
        .unwrap();
        let journal_record =
            CompletionJournalRecord::committed(completion, transaction_id, commit_lsn, Some(1))
                .unwrap();

        let report = reconcile_completion_recovery_from_wal(
            &wal.replay_durable(),
            &[CompletionRecoveryExpectation::for_invocation(invocation_id)
                .with_expected_metadata(Some(2), Some(1))
                .with_journal_record(journal_record)],
        )
        .unwrap();

        assert_eq!(
            report.records[0].status,
            CompletionRecoveryStatus::Ambiguous
        );
        assert_eq!(
            report.records[0].ambiguity,
            Some(CompletionRecoveryAmbiguity::RowsAffectedMismatch)
        );
    }

    #[test]
    fn completion_recovery_maps_rollback_completion() {
        let invocation_id = InvocationId::new(45);
        let transaction_id = crate::transaction_id_for_invocation(invocation_id);
        let mut wal = InMemoryWal::new();
        wal.append_tx_begin(transaction_id).unwrap();
        let rollback_lsn = wal.append_tx_rollback(transaction_id).unwrap();
        wal.flush_through(rollback_lsn).unwrap();

        let completion = CompletionMappingService::rolled_back(
            invocation_id,
            TransactionState::RolledBack,
            rollback_lsn,
            TraceId::new(450),
        )
        .unwrap();
        let journal_record =
            CompletionJournalRecord::rolled_back(completion, transaction_id, rollback_lsn).unwrap();

        let report = reconcile_completion_recovery_from_wal(
            &wal.replay_durable(),
            &[CompletionRecoveryExpectation::for_invocation(invocation_id)
                .with_journal_record(journal_record)],
        )
        .unwrap();

        assert_eq!(
            report.records[0].status,
            CompletionRecoveryStatus::RolledBack
        );
        assert_eq!(report.records[0].rows_affected, Some(0));
        assert_eq!(report.records[0].terminal_lsn, Some(rollback_lsn));
    }

    #[test]
    fn completion_recovery_reconstructs_committed_only_wal_evidence() {
        let invocation_id = InvocationId::new(46);
        let mut wal = InMemoryWal::new();
        let (_transaction_id, commit_lsn) =
            append_committed_invocation(&mut wal, invocation_id, b"");

        let report = reconcile_completion_recovery_from_wal(
            &wal.replay_durable(),
            &[CompletionRecoveryExpectation::for_invocation(invocation_id)
                .with_expected_metadata(Some(0), Some(0))],
        )
        .unwrap();

        assert_eq!(
            report.records[0].status,
            CompletionRecoveryStatus::Completed
        );
        assert_eq!(report.records[0].terminal_lsn, Some(commit_lsn));
        assert_eq!(report.records[0].rows_affected, Some(0));
        assert_eq!(report.completed().count(), 1);
    }

    #[test]
    fn poisoned_after_rollback_requires_rolled_back_state_and_durable_lsn() {
        let invocation_id = InvocationId::new(50);
        let trace_id = TraceId::new(500);

        // Active transaction state cannot back a poisoned completion.
        let err = CompletionMappingService::poisoned_after_rollback(
            invocation_id,
            TransactionState::Active,
            Lsn::new(7),
            trace_id,
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

        // Committed state cannot back a poisoned completion either.
        let err = CompletionMappingService::poisoned_after_rollback(
            invocation_id,
            TransactionState::Committed,
            Lsn::new(7),
            trace_id,
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);

        // Zero LSN must be rejected.
        let err = CompletionMappingService::poisoned_after_rollback(
            invocation_id,
            TransactionState::RolledBack,
            Lsn::ZERO,
            trace_id,
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Storage);

        // Valid combination: rolled-back + nonzero durable LSN.
        let completion = CompletionMappingService::poisoned_after_rollback(
            invocation_id,
            TransactionState::RolledBack,
            Lsn::new(11),
            trace_id,
        )
        .unwrap();
        assert_eq!(completion.status, CompletionStatus::Poisoned);
        assert_eq!(completion.rows_affected, Some(0));
        assert_eq!(
            completion.transaction_state,
            Some(TransactionState::RolledBack)
        );
        assert_eq!(completion.durable_lsn, Some(Lsn::new(11)));
        // Stable terminal code is wire-aligned with proto STATUS_POISONED = 5.
        assert_eq!(completion.status.terminal_code(), 5);
    }

    #[test]
    fn failed_before_transaction_carries_no_transaction_evidence() {
        let completion = CompletionMappingService::failed_before_transaction(
            InvocationId::new(51),
            TraceId::new(510),
        );
        assert_eq!(completion.status, CompletionStatus::FailedBeforeTransaction);
        assert!(completion.transaction_state.is_none());
        assert!(completion.durable_lsn.is_none());
        assert!(completion.rows_affected.is_none());
        assert_eq!(completion.status.terminal_code(), 3);
    }

    #[test]
    fn completion_recovery_does_not_fallback_to_success_when_wal_is_missing() {
        let report = reconcile_completion_recovery_from_wal(
            &[],
            &[
                CompletionRecoveryExpectation::for_invocation(InvocationId::new(47))
                    .with_expected_metadata(Some(1), Some(1)),
            ],
        )
        .unwrap();

        assert_eq!(
            report.records[0].status,
            CompletionRecoveryStatus::Ambiguous
        );
        assert_eq!(
            report.records[0].ambiguity,
            Some(CompletionRecoveryAmbiguity::MissingWalEvidence)
        );
    }
}
