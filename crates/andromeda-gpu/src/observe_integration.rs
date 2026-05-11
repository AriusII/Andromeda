//! Optional `andromeda-observe` integration for GPU execution traces.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observability::{EventCorrelation, EventId, TraceId};
use andromeda_observe::{
    EventEnvelope, GpuBudgetTraceEvidence, GpuExecutionFallbackReason, GpuExecutionJobClass,
    GpuExecutionOutcome, GpuExecutionTraceEvent, GpuValidationOutcome, TraceEvent,
};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, RequestId, SessionId};

use crate::{
    fallback::ValidationState,
    job::{FallbackReason, GpuJobClass},
    trace::{GpuBudgetEvidence, GpuExecutionResult, GpuExecutionTrace},
};

/// Optional non-authoritative correlation for GPU execution trace evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuTraceCorrelation {
    /// Request ID, paired with `session_id` when present.
    pub request_id: Option<RequestId>,
    /// Session ID, paired with `request_id` when present.
    pub session_id: Option<SessionId>,
    /// Procedure contract hash, paired with `catalog_version` when present.
    pub contract_hash: Option<ContractHash>,
    /// Catalog version, paired with `contract_hash` when present.
    pub catalog_version: Option<CatalogVersion>,
    /// Optional catalog object ID for analytical procedure context.
    pub catalog_object_id: Option<CatalogObjectId>,
}

impl GpuTraceCorrelation {
    /// Empty evidence-only correlation.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            request_id: None,
            session_id: None,
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
        }
    }

    const fn into_event_correlation(self) -> EventCorrelation {
        EventCorrelation {
            request_id: self.request_id,
            session_id: self.session_id,
            contract_hash: self.contract_hash,
            catalog_version: self.catalog_version,
            catalog_object_id: self.catalog_object_id,
            transaction_id: None,
            durable_lsn: None,
            protocol: None,
        }
    }
}

/// Creates a validated observe event envelope from a GPU execution trace.
///
/// # Errors
///
/// Returns an error if the GPU trace is unfinished, lacks budget evidence, uses
/// a zero trace ID, carries invalid optional correlation, or contains sensitive
/// markers in failure/rejection text.
pub fn gpu_execution_envelope(
    event_id: EventId,
    correlation: GpuTraceCorrelation,
    trace: &GpuExecutionTrace,
) -> AndromedaResult<EventEnvelope> {
    EventEnvelope::new(
        event_id,
        correlation.into_event_correlation(),
        TraceEvent::GpuExecution(to_observe_event(trace)?),
    )
}

fn to_observe_event(trace: &GpuExecutionTrace) -> AndromedaResult<GpuExecutionTraceEvent> {
    let budget_evidence = trace.budget_evidence.ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Internal,
            "GPU observe event requires budget evidence",
        )
    })?;

    let finished_at_unix_ms = trace.finished_at_unix_ms.ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Internal,
            "GPU observe event requires a finished execution trace",
        )
    })?;

    Ok(GpuExecutionTraceEvent {
        trace_id: TraceId::new(trace.trace_id),
        job_class: map_job_class(trace.job_class),
        started_at_unix_ms: trace.started_at_unix_ms,
        finished_at_unix_ms,
        outcome: map_result(&trace.result)?,
        validation: map_validation(&trace.validation_state),
        fallback_reason: trace.fallback_reason.map(map_fallback_reason),
        budget_evidence: map_budget_evidence(budget_evidence),
    })
}

fn map_job_class(job_class: GpuJobClass) -> GpuExecutionJobClass {
    match job_class {
        GpuJobClass::Statistics => GpuExecutionJobClass::Statistics,
        GpuJobClass::Analytics => GpuExecutionJobClass::Analytics,
        GpuJobClass::Benchmark => GpuExecutionJobClass::Benchmark,
    }
}

fn map_result(result: &GpuExecutionResult) -> AndromedaResult<GpuExecutionOutcome> {
    match result {
        GpuExecutionResult::Pending => Err(AndromedaError::new(
            AndromedaErrorKind::Internal,
            "GPU observe event cannot be emitted for a pending trace",
        )),
        GpuExecutionResult::Success => Ok(GpuExecutionOutcome::Success),
        GpuExecutionResult::Failed(reason) => Ok(GpuExecutionOutcome::Failed(reason.clone())),
        GpuExecutionResult::Cancelled => Ok(GpuExecutionOutcome::Cancelled),
    }
}

fn map_validation(validation: &ValidationState) -> GpuValidationOutcome {
    match validation {
        ValidationState::NotValidated => GpuValidationOutcome::NotValidated,
        ValidationState::Validated => GpuValidationOutcome::Validated,
        ValidationState::Rejected(reason) => GpuValidationOutcome::Rejected(reason.clone()),
    }
}

fn map_fallback_reason(reason: FallbackReason) -> GpuExecutionFallbackReason {
    match reason {
        FallbackReason::Disabled => GpuExecutionFallbackReason::Disabled,
        FallbackReason::BudgetExceeded => GpuExecutionFallbackReason::BudgetExceeded,
        FallbackReason::KillSwitchActive => GpuExecutionFallbackReason::KillSwitchActive,
        FallbackReason::CpuValidationFailed => GpuExecutionFallbackReason::CpuValidationFailed,
        FallbackReason::GpuComputationFailed => GpuExecutionFallbackReason::GpuComputationFailed,
        FallbackReason::DeviceUnavailable => GpuExecutionFallbackReason::DeviceUnavailable,
        FallbackReason::Cancelled => GpuExecutionFallbackReason::Cancelled,
    }
}

fn map_budget_evidence(evidence: GpuBudgetEvidence) -> GpuBudgetTraceEvidence {
    GpuBudgetTraceEvidence {
        requested_memory_bytes: evidence.requested.memory_bytes,
        requested_time_ms: evidence.requested.time_ms,
        requested_transfer_bytes: evidence.requested.transfer_bytes,
        budget_memory_bytes: evidence.budget.memory_bytes(),
        budget_time_ms: evidence.budget.time_ms(),
        budget_transfer_bytes: evidence.budget.transfer_bytes(),
    }
}
