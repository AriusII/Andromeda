use andromeda_core::{AndromedaResult, TransactionId};
use andromeda_observe::DurableAuditSinkReport;
use andromeda_storage::Lsn;

use crate::{CompletionStatus, InvocationCompletion};

use crate::services::permission_audit_emitter::{
    AuditEmissionEvidence, AuditEmissionKind, AuditEmissionOutcome, AuditEmissionPolicy,
    AuditSinkAvailability,
};

use super::journal::{
    CompletionJournalRecord, InvocationCompletionJournal, completion_journal_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionEmission {
    kind: CompletionEmissionKind,
    completion: InvocationCompletion,
    transaction_id: Option<TransactionId>,
    terminal_lsn: Option<Lsn>,
    result_row_count_exact: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionEmissionKind {
    Committed,
    RolledBack,
    PreTransaction,
}

impl CompletionEmission {
    pub const fn committed(
        completion: InvocationCompletion,
        transaction_id: TransactionId,
        terminal_lsn: Lsn,
        result_row_count_exact: Option<u64>,
    ) -> Self {
        Self {
            kind: CompletionEmissionKind::Committed,
            completion,
            transaction_id: Some(transaction_id),
            terminal_lsn: Some(terminal_lsn),
            result_row_count_exact,
        }
    }

    pub const fn rolled_back(
        completion: InvocationCompletion,
        transaction_id: TransactionId,
        terminal_lsn: Lsn,
    ) -> Self {
        Self {
            kind: CompletionEmissionKind::RolledBack,
            completion,
            transaction_id: Some(transaction_id),
            terminal_lsn: Some(terminal_lsn),
            result_row_count_exact: Some(0),
        }
    }

    pub const fn pre_transaction(completion: InvocationCompletion) -> Self {
        Self {
            kind: CompletionEmissionKind::PreTransaction,
            completion,
            transaction_id: None,
            terminal_lsn: None,
            result_row_count_exact: None,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        completion_record_from_emission(*self).map(|_| ())
    }

    pub fn audit_evidence(
        &self,
        policy: AuditEmissionPolicy,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<AuditEmissionEvidence> {
        self.validate()?;
        AuditEmissionEvidence::completion_emitted(
            policy,
            self.completion.trace_id,
            format!(
                "completion emitted with terminal code {}",
                self.completion.status.terminal_code()
            ),
            sink,
        )
    }

    pub fn audit_evidence_from_durable_report(
        &self,
        policy: AuditEmissionPolicy,
        report: DurableAuditSinkReport,
    ) -> AndromedaResult<AuditEmissionEvidence> {
        self.audit_evidence(
            policy,
            AuditSinkAvailability::durable_for_policy(policy, report)?,
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct InvocationCompletionEmitter {
    journal: InvocationCompletionJournal,
}

impl InvocationCompletionEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_journal(journal: InvocationCompletionJournal) -> Self {
        Self { journal }
    }

    pub fn emit(
        &mut self,
        emission: CompletionEmission,
    ) -> AndromedaResult<&CompletionJournalRecord> {
        if self
            .journal
            .get(emission.completion.invocation_id)
            .is_some()
        {
            return Err(completion_journal_error(
                "invocation completion already emitted for invocation",
            ));
        }

        let record = completion_record_from_emission(emission)?;
        self.journal.record(record)
    }

    pub fn emit_with_audit(
        &mut self,
        emission: CompletionEmission,
        audit_evidence: AuditEmissionEvidence,
    ) -> AndromedaResult<&CompletionJournalRecord> {
        validate_completion_audit_evidence(&emission, &audit_evidence)?;
        self.emit(emission)
    }

    pub fn emit_with_audit_policy(
        &mut self,
        emission: CompletionEmission,
        policy: AuditEmissionPolicy,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<&CompletionJournalRecord> {
        let audit_evidence = emission.audit_evidence(policy, sink)?;
        self.emit_with_audit(emission, audit_evidence)
    }

    pub fn emit_with_durable_audit_report(
        &mut self,
        emission: CompletionEmission,
        policy: AuditEmissionPolicy,
        report: DurableAuditSinkReport,
    ) -> AndromedaResult<&CompletionJournalRecord> {
        let audit_evidence = emission.audit_evidence_from_durable_report(policy, report)?;
        self.emit_with_audit(emission, audit_evidence)
    }

    pub fn get(
        &self,
        invocation_id: andromeda_core::InvocationId,
    ) -> Option<&CompletionJournalRecord> {
        self.journal.get(invocation_id)
    }

    pub fn journal(&self) -> &InvocationCompletionJournal {
        &self.journal
    }

    pub fn into_journal(self) -> InvocationCompletionJournal {
        self.journal
    }
}

fn validate_completion_audit_evidence(
    emission: &CompletionEmission,
    audit_evidence: &AuditEmissionEvidence,
) -> AndromedaResult<()> {
    audit_evidence.validate()?;
    if audit_evidence.kind != AuditEmissionKind::Completion {
        return Err(completion_journal_error(
            "completion emission requires completion audit evidence",
        ));
    }
    if audit_evidence.outcome != AuditEmissionOutcome::Emitted {
        return Err(completion_journal_error(
            "completion audit evidence requires emitted outcome",
        ));
    }
    if audit_evidence.trace_id != emission.completion.trace_id {
        return Err(completion_journal_error(
            "completion audit evidence trace id must match emitted completion",
        ));
    }
    Ok(())
}

fn completion_record_from_emission(
    emission: CompletionEmission,
) -> AndromedaResult<CompletionJournalRecord> {
    let completion = emission.completion;

    let record = match emission.kind {
        CompletionEmissionKind::Committed => {
            if completion.status != CompletionStatus::Committed {
                return Err(completion_journal_error(
                    "committed completion emission requires committed completion status",
                ));
            }
            CompletionJournalRecord {
                invocation_id: completion.invocation_id,
                transaction_id: emission.transaction_id,
                status: completion.status,
                transaction_state: completion.transaction_state,
                rows_affected: completion.rows_affected,
                result_row_count_exact: emission.result_row_count_exact,
                terminal_lsn: emission.terminal_lsn,
                durable_lsn: completion.durable_lsn,
                trace_id: completion.trace_id,
            }
        }
        CompletionEmissionKind::RolledBack => {
            if completion.status != CompletionStatus::RolledBack {
                return Err(completion_journal_error(
                    "rolled-back completion emission requires rolled-back completion status",
                ));
            }
            CompletionJournalRecord {
                invocation_id: completion.invocation_id,
                transaction_id: emission.transaction_id,
                status: completion.status,
                transaction_state: completion.transaction_state,
                rows_affected: completion.rows_affected,
                result_row_count_exact: Some(0),
                terminal_lsn: emission.terminal_lsn,
                durable_lsn: completion.durable_lsn,
                trace_id: completion.trace_id,
            }
        }
        CompletionEmissionKind::PreTransaction => {
            if completion.status.is_transactional_terminal() {
                return Err(completion_journal_error(
                    "pre-transaction completion emission cannot use transactional terminal status",
                ));
            }
            if emission.transaction_id.is_some()
                || emission.terminal_lsn.is_some()
                || emission.result_row_count_exact.is_some()
                || completion.transaction_state.is_some()
                || completion.rows_affected.is_some()
                || completion.durable_lsn.is_some()
            {
                return Err(completion_journal_error(
                    "non-transactional completion emission must not carry transaction or result evidence",
                ));
            }
            CompletionJournalRecord {
                invocation_id: completion.invocation_id,
                transaction_id: None,
                status: completion.status,
                transaction_state: None,
                rows_affected: None,
                result_row_count_exact: None,
                terminal_lsn: None,
                durable_lsn: None,
                trace_id: completion.trace_id,
            }
        }
    };

    record.validate()?;
    Ok(record)
}
