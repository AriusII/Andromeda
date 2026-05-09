use andromeda_audit::DurableAuditSinkReport;
use andromeda_error::AndromedaResult;
use andromeda_observability::TraceId;
use andromeda_result_stream::{CompletionStatus, InvocationCompletion};
use andromeda_types::TransactionId;
use andromeda_wal::Lsn;

use super::journal::{
    CompletionJournalRecord, InvocationCompletionJournal, completion_journal_error,
};

pub trait CompletionAuditEvidence {
    fn validate_completion_audit(&self, expected_trace_id: TraceId) -> AndromedaResult<()>;
}

pub trait CompletionAuditPolicy<Sink> {
    type Evidence: CompletionAuditEvidence;

    fn completion_emitted(
        &self,
        trace_id: TraceId,
        reason: String,
        sink: Sink,
    ) -> AndromedaResult<Self::Evidence>;

    fn sink_from_durable_report(&self, report: DurableAuditSinkReport) -> AndromedaResult<Sink>;
}

impl CompletionAuditEvidence for andromeda_audit::AuditEmissionEvidence {
    fn validate_completion_audit(&self, expected_trace_id: TraceId) -> AndromedaResult<()> {
        self.validate()?;
        if self.kind != andromeda_audit::AuditEmissionKind::Completion {
            return Err(completion_journal_error(
                "completion emission requires completion audit evidence",
            ));
        }
        if self.outcome != andromeda_audit::AuditEmissionOutcome::Emitted {
            return Err(completion_journal_error(
                "completion audit evidence requires emitted outcome",
            ));
        }
        if self.trace_id != expected_trace_id {
            return Err(completion_journal_error(
                "completion audit evidence trace id must match emitted completion",
            ));
        }
        Ok(())
    }
}

impl CompletionAuditPolicy<andromeda_audit::AuditSinkAvailability>
    for andromeda_audit::AuditEmissionPolicy
{
    type Evidence = andromeda_audit::AuditEmissionEvidence;

    fn completion_emitted(
        &self,
        trace_id: TraceId,
        reason: String,
        sink: andromeda_audit::AuditSinkAvailability,
    ) -> AndromedaResult<Self::Evidence> {
        andromeda_audit::AuditEmissionEvidence::completion_emitted(*self, trace_id, reason, sink)
    }

    fn sink_from_durable_report(
        &self,
        report: DurableAuditSinkReport,
    ) -> AndromedaResult<andromeda_audit::AuditSinkAvailability> {
        andromeda_audit::AuditSinkAvailability::durable_for_policy(*self, report)
    }
}

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

    pub fn audit_evidence<Policy, Sink>(
        &self,
        policy: Policy,
        sink: Sink,
    ) -> AndromedaResult<Policy::Evidence>
    where
        Policy: CompletionAuditPolicy<Sink>,
    {
        self.validate()?;
        policy.completion_emitted(
            self.completion.trace_id,
            format!(
                "completion emitted with terminal code {}",
                self.completion.status.terminal_code()
            ),
            sink,
        )
    }

    pub fn audit_evidence_from_durable_report<Policy, Sink>(
        &self,
        policy: Policy,
        report: DurableAuditSinkReport,
    ) -> AndromedaResult<Policy::Evidence>
    where
        Policy: CompletionAuditPolicy<Sink>,
    {
        let sink = policy.sink_from_durable_report(report)?;
        self.audit_evidence(policy, sink)
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

    pub fn emit_with_audit<Evidence>(
        &mut self,
        emission: CompletionEmission,
        audit_evidence: Evidence,
    ) -> AndromedaResult<&CompletionJournalRecord>
    where
        Evidence: CompletionAuditEvidence,
    {
        audit_evidence.validate_completion_audit(emission.completion.trace_id)?;
        self.emit(emission)
    }

    pub fn emit_with_audit_policy<Policy, Sink>(
        &mut self,
        emission: CompletionEmission,
        policy: Policy,
        sink: Sink,
    ) -> AndromedaResult<&CompletionJournalRecord>
    where
        Policy: CompletionAuditPolicy<Sink>,
    {
        let audit_evidence = emission.audit_evidence(policy, sink)?;
        self.emit_with_audit(emission, audit_evidence)
    }

    pub fn emit_with_durable_audit_report<Policy, Sink>(
        &mut self,
        emission: CompletionEmission,
        policy: Policy,
        report: DurableAuditSinkReport,
    ) -> AndromedaResult<&CompletionJournalRecord>
    where
        Policy: CompletionAuditPolicy<Sink>,
    {
        let audit_evidence = emission.audit_evidence_from_durable_report(policy, report)?;
        self.emit_with_audit(emission, audit_evidence)
    }

    pub fn get(
        &self,
        invocation_id: andromeda_types::InvocationId,
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
        },
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
        },
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
        },
    };

    record.validate()?;
    Ok(record)
}
