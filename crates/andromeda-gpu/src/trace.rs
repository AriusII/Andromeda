//! GPU execution trace for observability and post-mortem analysis.
//!
//! [`GpuExecutionTrace`] is the record of a single GPU job execution. It
//! captures the job class, a caller-supplied trace identifier, timestamps
//! (caller-supplied so that this module has no clock dependency), the final
//! execution result, the validation state, and — when applicable — the reason
//! a CPU fallback was used.
//!
//! # Builder pattern
//!
//! Construct a trace with [`GpuExecutionTrace::new`], then finalise it with
//! [`GpuExecutionTrace::finish`] and optionally augment it with
//! [`with_validation_state`][GpuExecutionTrace::with_validation_state] and
//! [`with_fallback_reason`][GpuExecutionTrace::with_fallback_reason].

use crate::{
    budget::{GpuBudget, GpuBudgetRequest},
    fallback::ValidationState,
    job::{FallbackReason, GpuJobClass},
};

const TRACE_REASON_MAX_BYTES: usize = 256;
const TRACE_REASON_TRUNCATION_SUFFIX: &str = "...";

fn truncate_trace_reason(mut reason: String) -> String {
    if reason.len() <= TRACE_REASON_MAX_BYTES {
        return reason;
    }

    let keep_bytes = TRACE_REASON_MAX_BYTES.saturating_sub(TRACE_REASON_TRUNCATION_SUFFIX.len());
    let mut end = keep_bytes.min(reason.len());
    while end > 0 && !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason.truncate(end);
    reason.push_str(TRACE_REASON_TRUNCATION_SUFFIX);
    reason
}

fn sanitize_execution_result(result: GpuExecutionResult) -> GpuExecutionResult {
    match result {
        GpuExecutionResult::Failed(reason) => {
            GpuExecutionResult::Failed(truncate_trace_reason(reason))
        },
        other => other,
    }
}

fn sanitize_validation_state(state: ValidationState) -> ValidationState {
    match state {
        ValidationState::Rejected(reason) => {
            ValidationState::Rejected(truncate_trace_reason(reason))
        },
        other => other,
    }
}

/// The outcome of a GPU job execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuExecutionResult {
    /// The job has been started but has not yet finished.
    Pending,
    /// The job completed successfully and the result passed validation.
    Success,
    /// The job failed with the enclosed diagnostic message.
    Failed(String),
    /// The job was cancelled by the kill switch before completion.
    Cancelled,
}

/// Bounded resource-budget evidence captured for a GPU execution attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuBudgetEvidence {
    /// The requested GPU resource envelope.
    pub requested: GpuBudgetRequest,
    /// The configured budget limits used for admission.
    pub budget: GpuBudget,
}

impl GpuBudgetEvidence {
    /// Creates budget evidence from a request and configured budget.
    #[must_use]
    pub const fn new(requested: GpuBudgetRequest, budget: GpuBudget) -> Self {
        Self { requested, budget }
    }
}

/// Observability record for a single GPU job execution.
///
/// Timestamps are supplied by the caller so that this type has no dependency
/// on any clock abstraction. In v0, both timestamps are set to `0`; callers
/// that require real timestamps should wrap this struct in a higher-level
/// context that provides wall-clock time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuExecutionTrace {
    /// The domain class of this GPU job.
    pub job_class: GpuJobClass,
    /// An opaque 128-bit identifier for correlation with external observability
    /// systems. Callers should supply a unique value per invocation.
    pub trace_id: u128,
    /// Unix millisecond timestamp at which execution started (caller-supplied).
    pub started_at_unix_ms: u64,
    /// Unix millisecond timestamp at which execution finished, or `None` if the
    /// job is still [`Pending`][GpuExecutionResult::Pending].
    pub finished_at_unix_ms: Option<u64>,
    /// Final execution result.
    pub result: GpuExecutionResult,
    /// Whether the GPU result was validated against a CPU shadow.
    pub validation_state: ValidationState,
    /// Why the CPU fallback path was taken, if applicable.
    pub fallback_reason: Option<FallbackReason>,
    /// Resource-budget request/limit evidence when known.
    pub budget_evidence: Option<GpuBudgetEvidence>,
}

impl GpuExecutionTrace {
    /// Maximum byte length retained for trace failure/rejection reasons.
    pub const MAX_REASON_BYTES: usize = TRACE_REASON_MAX_BYTES;

    /// Creates a new trace in the [`Pending`][GpuExecutionResult::Pending] state.
    ///
    /// Pass `trace_id = 0` and `started_at_unix_ms = 0` in v0 contexts where
    /// no real identifiers or clocks are available.
    #[must_use]
    pub fn new(job_class: GpuJobClass, trace_id: u128, started_at_unix_ms: u64) -> Self {
        Self {
            job_class,
            trace_id,
            started_at_unix_ms,
            finished_at_unix_ms: None,
            result: GpuExecutionResult::Pending,
            validation_state: ValidationState::NotValidated,
            fallback_reason: None,
            budget_evidence: None,
        }
    }

    /// Finalises the trace with a finish timestamp and result.
    #[must_use]
    pub fn finish(mut self, finished_at_unix_ms: u64, result: GpuExecutionResult) -> Self {
        self.finished_at_unix_ms = Some(finished_at_unix_ms);
        self.result = sanitize_execution_result(result);
        self
    }

    /// Sets the validation state on this trace.
    #[must_use]
    pub fn with_validation_state(mut self, state: ValidationState) -> Self {
        self.validation_state = sanitize_validation_state(state);
        self
    }

    /// Sets the fallback reason on this trace.
    #[must_use]
    pub fn with_fallback_reason(mut self, reason: FallbackReason) -> Self {
        self.fallback_reason = Some(reason);
        self
    }

    /// Attaches resource-budget evidence to this trace.
    #[must_use]
    pub fn with_budget_evidence(mut self, evidence: GpuBudgetEvidence) -> Self {
        self.budget_evidence = Some(evidence);
        self
    }

    /// Typed cancellation trace contract for kill-switch short-circuits.
    #[must_use]
    pub fn cancelled_by_kill_switch(
        job_class: GpuJobClass,
        trace_id: u128,
        started_at_unix_ms: u64,
        finished_at_unix_ms: u64,
    ) -> Self {
        Self::new(job_class, trace_id, started_at_unix_ms)
            .finish(finished_at_unix_ms, GpuExecutionResult::Cancelled)
            .with_validation_state(ValidationState::NotValidated)
            .with_fallback_reason(FallbackReason::KillSwitchActive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_trace_is_pending() {
        let trace = GpuExecutionTrace::new(GpuJobClass::Statistics, 42, 1_000);
        assert_eq!(trace.result, GpuExecutionResult::Pending);
        assert_eq!(trace.validation_state, ValidationState::NotValidated);
        assert_eq!(trace.fallback_reason, None);
        assert_eq!(trace.budget_evidence, None);
        assert_eq!(trace.finished_at_unix_ms, None);
    }

    #[test]
    fn finish_sets_result_and_timestamp() {
        let trace = GpuExecutionTrace::new(GpuJobClass::Analytics, 0, 0)
            .finish(100, GpuExecutionResult::Success);
        assert_eq!(trace.result, GpuExecutionResult::Success);
        assert_eq!(trace.finished_at_unix_ms, Some(100));
    }

    #[test]
    fn cancelled_by_kill_switch_contract_sets_typed_fields() {
        let trace = GpuExecutionTrace::cancelled_by_kill_switch(GpuJobClass::Statistics, 9, 10, 20);
        assert_eq!(trace.result, GpuExecutionResult::Cancelled);
        assert_eq!(
            trace.fallback_reason,
            Some(FallbackReason::KillSwitchActive)
        );
        assert_eq!(trace.validation_state, ValidationState::NotValidated);
        assert_eq!(trace.finished_at_unix_ms, Some(20));
    }

    #[test]
    fn builder_chain_sets_all_fields() {
        let trace = GpuExecutionTrace::new(GpuJobClass::Statistics, 1, 50)
            .finish(150, GpuExecutionResult::Failed("test".to_owned()))
            .with_validation_state(ValidationState::Rejected("diverged".to_owned()))
            .with_fallback_reason(FallbackReason::CpuValidationFailed);
        assert_eq!(
            trace.validation_state,
            ValidationState::Rejected("diverged".to_owned())
        );
        assert_eq!(
            trace.fallback_reason,
            Some(FallbackReason::CpuValidationFailed)
        );
    }

    #[test]
    fn finish_truncates_failed_reason_to_bounded_size() {
        let long_reason = "x".repeat(1_024);
        let trace = GpuExecutionTrace::new(GpuJobClass::Analytics, 1, 0)
            .finish(1, GpuExecutionResult::Failed(long_reason));

        match trace.result {
            GpuExecutionResult::Failed(reason) => {
                assert_eq!(reason.len(), GpuExecutionTrace::MAX_REASON_BYTES);
                assert!(reason.ends_with(TRACE_REASON_TRUNCATION_SUFFIX));
            },
            other => panic!("expected Failed reason, got {other:?}"),
        }
    }

    #[test]
    fn validation_state_truncates_rejected_reason_to_bounded_size() {
        let long_reason = "x".repeat(1_024);
        let trace = GpuExecutionTrace::new(GpuJobClass::Analytics, 1, 0)
            .with_validation_state(ValidationState::Rejected(long_reason));

        match trace.validation_state {
            ValidationState::Rejected(reason) => {
                assert_eq!(reason.len(), GpuExecutionTrace::MAX_REASON_BYTES);
                assert!(reason.ends_with(TRACE_REASON_TRUNCATION_SUFFIX));
            },
            other => panic!("expected Rejected reason, got {other:?}"),
        }
    }
}
