//! Complete lifecycle record for a single procedure invocation.
//!
//! A [`ProcedureInvocationTrace`] is created when an invocation is admitted
//! to the runtime and finalized once the terminal outcome is known.  The
//! record is stored in the [`crate::AuditLedger`] so forensic tooling can
//! reconstruct the full dispatch history from admitted → committed/rolled-back
//! without replaying the WAL.
//!
//! ## Idempotent finalization
//!
//! [`ProcedureInvocationTrace::complete`] is idempotent: the first call sets
//! `completed_at` and `completion_status`; subsequent calls return the struct
//! unchanged.  This guards against accidental double-completion in
//! error-recovery and retry code paths.

use std::time::SystemTime;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observability::TraceId;
use andromeda_principal::PrincipalId;
use andromeda_procedure_contract::{IdempotencyKey, ProcedureDeadline};
use andromeda_result_stream::CompletionStatus;
use andromeda_types::InvocationId;

/// A complete durable record of a single procedure invocation lifecycle.
///
/// Instances are created via [`ProcedureInvocationTrace::started`] at
/// admission time and finalized via [`ProcedureInvocationTrace::complete`]
/// once a terminal outcome (`Committed`, `RolledBack`, …) is known.
///
/// All optional fields are forward-compatible placeholders for information
/// that is not yet propagated through the runtime (see field doc-comments).
#[derive(Debug, Clone)]
pub struct ProcedureInvocationTrace {
    /// Trace context id shared across all events belonging to this invocation.
    /// Must be non-zero; checked by [`validate`][Self::validate].
    pub trace_id: TraceId,

    /// Unique, stable identifier for this invocation attempt.
    /// Must be non-zero; checked by [`validate`][Self::validate].
    pub invocation_id: InvocationId,

    /// Authenticated caller identity.
    ///
    /// `None` until `PrincipalId` is propagated through `InvocationContext`
    /// (planned for P05).  Stored as `Option` so records written before that
    /// migration are still valid.
    pub principal: Option<PrincipalId>,

    /// Caller-supplied deadline for the entire invocation.
    ///
    /// `None` until `ProcedureDeadline` is propagated through
    /// `InvocationRequest` (planned for P05).
    pub deadline: Option<ProcedureDeadline>,

    /// Caller-supplied idempotency key.  `None` when not provided.
    pub idempotency_key: Option<IdempotencyKey>,

    /// Wall-clock time the invocation was admitted to the runtime.
    pub started_at: SystemTime,

    /// Terminal completion status.
    ///
    /// `None` on a started-only record; set by [`complete`][Self::complete].
    pub completion_status: Option<CompletionStatus>,

    /// Wall-clock time the terminal outcome was recorded.
    ///
    /// `None` on a started-only record; set by [`complete`][Self::complete].
    pub completed_at: Option<SystemTime>,
}

impl ProcedureInvocationTrace {
    /// Build a started-only (incomplete) trace record.
    ///
    /// Both `trace_id` and `invocation_id` must be non-zero; call
    /// [`validate`][Self::validate] if you need a hard structural check.
    pub fn started(trace_id: TraceId, invocation_id: InvocationId) -> Self {
        Self {
            trace_id,
            invocation_id,
            principal: None,
            deadline: None,
            idempotency_key: None,
            started_at: SystemTime::now(),
            completion_status: None,
            completed_at: None,
        }
    }

    /// Finalize the trace with a terminal completion status.
    ///
    /// Idempotent: if `completed_at` is already set, this returns `self`
    /// unchanged so double-completion from retry/recovery paths is harmless.
    #[must_use]
    pub fn complete(mut self, completion_status: CompletionStatus) -> Self {
        if self.completed_at.is_some() {
            // First completion wins; ignore subsequent calls.
            return self;
        }
        self.completion_status = Some(completion_status);
        self.completed_at = Some(SystemTime::now());
        self
    }

    /// Returns `true` if [`complete`][Self::complete] has been called at
    /// least once (i.e. `completed_at` is set).
    pub fn is_completed(&self) -> bool {
        self.completed_at.is_some()
    }

    /// Returns `true` if the invocation completed with a
    /// [`CompletionStatus::Committed`] outcome.
    pub fn is_success(&self) -> bool {
        matches!(self.completion_status, Some(CompletionStatus::Committed))
    }

    /// Validate the structural invariants of this record.
    ///
    /// # Errors
    ///
    /// Returns [`AndromedaErrorKind::Contract`] if:
    /// - `trace_id` is zero
    /// - `invocation_id` is zero
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "ProcedureInvocationTrace: trace_id must be non-zero",
            ));
        }
        if self.invocation_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "ProcedureInvocationTrace: invocation_id must be non-zero",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_started(trace: u128, inv: u64) -> ProcedureInvocationTrace {
        ProcedureInvocationTrace::started(TraceId::new(trace), InvocationId::new(inv))
    }

    #[test]
    fn started_record_is_not_completed() {
        let trace = make_started(1, 10);
        assert!(!trace.is_completed());
        assert!(!trace.is_success());
        assert!(trace.completion_status.is_none());
        assert!(trace.completed_at.is_none());
    }

    #[test]
    fn complete_sets_status_and_timestamp() {
        let trace = make_started(2, 20).complete(CompletionStatus::Committed);
        assert!(trace.is_completed());
        assert!(trace.is_success());
        assert_eq!(trace.completion_status, Some(CompletionStatus::Committed));
        assert!(trace.completed_at.is_some());
    }

    #[test]
    fn complete_is_idempotent_first_completion_wins() {
        // Complete with RolledBack first, then Committed.
        let first = make_started(3, 30)
            .complete(CompletionStatus::RolledBack)
            .complete(CompletionStatus::Committed);
        // First completion (RolledBack) must be retained.
        assert_eq!(first.completion_status, Some(CompletionStatus::RolledBack));
    }

    #[test]
    fn rolled_back_outcome_is_not_success() {
        let trace = make_started(4, 40).complete(CompletionStatus::RolledBack);
        assert!(trace.is_completed());
        assert!(!trace.is_success());
    }

    #[test]
    fn validate_rejects_zero_trace_id() {
        let trace = ProcedureInvocationTrace::started(TraceId::new(0), InvocationId::new(1));
        let err = trace.validate().unwrap_err();
        assert!(err.message().contains("trace_id must be non-zero"));
    }

    #[test]
    fn validate_rejects_zero_invocation_id() {
        let trace = ProcedureInvocationTrace::started(TraceId::new(1), InvocationId::new(0));
        let err = trace.validate().unwrap_err();
        assert!(err.message().contains("invocation_id must be non-zero"));
    }

    #[test]
    fn validate_accepts_valid_started_record() {
        let trace = make_started(5, 50);
        assert!(trace.validate().is_ok());
    }

    #[test]
    fn validate_accepts_completed_record() {
        let trace = make_started(6, 60).complete(CompletionStatus::PermissionDenied);
        assert!(trace.validate().is_ok());
        assert!(!trace.is_success());
    }
}
