//! Admission control and contract validation audit event types.
//!
//! This module defines the audit event taxonomy for admission control operations,
//! including contract validation, admission decisions, permission checks, throttling,
//! and procedure dispatch authorization.
//!
//! Each event captures a critical decision point in the request admission lifecycle,
//! enabling forensic analysis of quota violations, permission denials, policy breaches,
//! and authorization failures.
//!
//! ## Audit Principles
//!
//! - **Immutability**: Once emitted, an audit event is immutable and append-only.
//! - **Principal Binding**: All events carry an `affected_principal` bound at emission time.
//!   This binding is immutable and cannot be re-bound after event creation.
//! - **Completeness**: Every admission decision (pass/fail) is traced with machine-parseable reasons.
//! - **Traceability**: All events carry a `procedure_id` for correlation with execution traces.
//! - **No Optional Fields**: All events are fully specified; no Optional fields in the event enum.
//! - **Machine-Parseable Classification**: Event types and reasons are deterministic, not free-form strings.

use std::time::SystemTime;

use crate::TraceId;

/// Unique procedure identifier for admission tracing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcedureId(u64);

impl ProcedureId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// Contract validation result: Valid, Invalid, or Deprecated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContractValidationResult {
    /// Contract passed validation against current catalog.
    Valid,
    /// Contract failed validation; incompatible or corrupted.
    Invalid,
    /// Contract was valid but is now deprecated; procedure can still be invoked.
    Deprecated,
}

impl ContractValidationResult {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Deprecated => "deprecated",
        }
    }

    pub const fn is_valid(self) -> bool {
        matches!(self, Self::Valid | Self::Deprecated)
    }
}

/// Admission decision: whether request is accepted or rejected with reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdmissionDecisionKind {
    /// Request accepted; will be admitted to execution.
    Accepted,
    /// Request rejected: quota exceeded or throttling limit reached.
    RejectedQuotaExceeded,
    /// Request rejected: system health state does not permit execution.
    RejectedHealthState,
    /// Request rejected: procedure is not recognized in current catalog.
    RejectedUnknownProcedure,
}

impl AdmissionDecisionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::RejectedQuotaExceeded => "rejected_quota_exceeded",
            Self::RejectedHealthState => "rejected_health_state",
            Self::RejectedUnknownProcedure => "rejected_unknown_procedure",
        }
    }

    pub const fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub const fn is_rejected(self) -> bool {
        !self.is_accepted()
    }
}

/// Backpressure/throttling reason code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackpressureReason {
    /// Connection buffer pool exhausted.
    BufferPoolFull,
    /// Network congestion detected; retry recommended.
    NetworkCongestion,
    /// Request queue overloaded.
    QueueOverload,
}

impl BackpressureReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BufferPoolFull => "buffer_pool_full",
            Self::NetworkCongestion => "network_congestion",
            Self::QueueOverload => "queue_overload",
        }
    }
}

/// Principal identifier bound immutably at event emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedPrincipal {
    /// Immutable principal identity (mTLS subject CN or service account ID).
    pub principal_id: String,
}

impl AffectedPrincipal {
    pub fn new(principal_id: impl Into<String>) -> Self {
        Self {
            principal_id: principal_id.into(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.principal_id.trim().is_empty()
    }
}

/// Admission control audit event types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionAuditEvent {
    /// Contract validation completed.
    ///
    /// Emitted by: `andromeda-exec::services::admission` after contract hash validation
    /// Triggering condition: Procedure invocation request carries contract; catalog lookup succeeds or fails
    /// Invariant: Emitted before admission decision gate.
    ContractValidated {
        /// Unique trace ID for correlation.
        trace_id: TraceId,
        /// Procedure being validated.
        procedure_id: ProcedureId,
        /// Row count of input contract batch.
        input_row_count: u64,
        /// Validation result: Valid/Invalid/Deprecated.
        result: ContractValidationResult,
        /// Machine-parseable validation details (e.g., "hash mismatch: expected <hash>, got <hash>").
        validation_details: String,
        /// Principal immutably bound at emission.
        affected_principal: AffectedPrincipal,
        /// Timestamp of validation event.
        event_timestamp: SystemTime,
    },

    /// Admission decision reached.
    ///
    /// Emitted by: `andromeda-exec::services::admission` after policy evaluation
    /// Triggering condition: Request reaches admission gate; decision made to accept or reject
    /// Invariant: No transaction created if decision is rejected.
    AdmissionDecision {
        /// Unique trace ID for correlation.
        trace_id: TraceId,
        /// Procedure undergoing admission.
        procedure_id: ProcedureId,
        /// Acceptance or rejection decision.
        decision: AdmissionDecisionKind,
        /// Machine-parseable reason (e.g., "principal quota limit exceeded: 1000 active requests").
        reason: String,
        /// Principal immutably bound at emission.
        affected_principal: AffectedPrincipal,
        /// Timestamp of decision event.
        event_timestamp: SystemTime,
    },

    /// Permission check failed during authorization.
    ///
    /// Emitted by: `andromeda-exec::services::admission` when principal lacks required permission
    /// Triggering condition: Permission grant lookup shows missing or insufficient permission
    /// Invariant: Emitted only on permission denial.
    PermissionCheckFailed {
        /// Unique trace ID for correlation.
        trace_id: TraceId,
        /// Procedure requiring permission.
        procedure_id: ProcedureId,
        /// Principal attempting access.
        affected_principal: AffectedPrincipal,
        /// Required permission (e.g., "execute_procedure").
        required_permission: String,
        /// Actual permissions held (comma-separated; empty if none).
        actual_permission_set: String,
        /// Timestamp of failure event.
        event_timestamp: SystemTime,
    },

    /// Request throttled or rate-limited.
    ///
    /// Emitted by: `andromeda-exec::services::admission` when request denied due to backpressure
    /// Triggering condition: Buffer, queue, or network resource exhausted
    /// Invariant: Emitted with retry_after_ms guidance for client.
    RequestThrottled {
        /// Unique trace ID for correlation.
        trace_id: TraceId,
        /// Procedure that triggered throttling.
        procedure_id: ProcedureId,
        /// Backpressure reason: BufferPoolFull, NetworkCongestion, or QueueOverload.
        backpressure_reason: BackpressureReason,
        /// Recommended retry delay in milliseconds.
        retry_after_ms: u64,
        /// Timestamp of throttling event.
        event_timestamp: SystemTime,
    },

    /// Procedure dispatch authorized.
    ///
    /// Emitted by: `andromeda-exec::services::admission` after all gates passed
    /// Triggering condition: Contract valid, principal authorized, quota available, health check passed
    /// Invariant: Emitted immediately before dispatch; proves mTLS binding and authorization.
    ProcedureDispatchAuthorized {
        /// Unique trace ID for correlation.
        trace_id: TraceId,
        /// Procedure being dispatched.
        procedure_id: ProcedureId,
        /// Surface plane (Application, Administration, Cluster, etc).
        surface_plane: String,
        /// mTLS certificate identity (subject CN).
        certificate_identity: String,
        /// Principal immutably bound at emission.
        affected_principal: AffectedPrincipal,
        /// Exact timestamp of dispatch authorization.
        dispatch_timestamp: SystemTime,
    },
}

impl AdmissionAuditEvent {
    /// Human-readable event type for logging and audit reporting.
    pub fn type_label(&self) -> &'static str {
        match self {
            Self::ContractValidated { .. } => "contract_validated",
            Self::AdmissionDecision { .. } => "admission_decision",
            Self::PermissionCheckFailed { .. } => "permission_check_failed",
            Self::RequestThrottled { .. } => "request_throttled",
            Self::ProcedureDispatchAuthorized { .. } => "procedure_dispatch_authorized",
        }
    }

    /// Extract affected principal from any admission audit event.
    pub fn affected_principal_id(&self) -> &str {
        match self {
            Self::ContractValidated {
                affected_principal, ..
            } => &affected_principal.principal_id,
            Self::AdmissionDecision {
                affected_principal, ..
            } => &affected_principal.principal_id,
            Self::PermissionCheckFailed {
                affected_principal, ..
            } => &affected_principal.principal_id,
            Self::RequestThrottled { .. } => "",
            Self::ProcedureDispatchAuthorized {
                affected_principal, ..
            } => &affected_principal.principal_id,
        }
    }

    /// Extract trace ID from the event for correlation.
    pub fn trace_id(&self) -> TraceId {
        match self {
            Self::ContractValidated { trace_id, .. } => *trace_id,
            Self::AdmissionDecision { trace_id, .. } => *trace_id,
            Self::PermissionCheckFailed { trace_id, .. } => *trace_id,
            Self::RequestThrottled { trace_id, .. } => *trace_id,
            Self::ProcedureDispatchAuthorized { trace_id, .. } => *trace_id,
        }
    }

    /// Extract procedure ID from the event.
    pub fn procedure_id(&self) -> ProcedureId {
        match self {
            Self::ContractValidated { procedure_id, .. } => *procedure_id,
            Self::AdmissionDecision { procedure_id, .. } => *procedure_id,
            Self::PermissionCheckFailed { procedure_id, .. } => *procedure_id,
            Self::RequestThrottled { procedure_id, .. } => *procedure_id,
            Self::ProcedureDispatchAuthorized { procedure_id, .. } => *procedure_id,
        }
    }

    /// Extract event timestamp for timeline reconstruction.
    pub fn event_timestamp(&self) -> SystemTime {
        match self {
            Self::ContractValidated {
                event_timestamp, ..
            } => *event_timestamp,
            Self::AdmissionDecision {
                event_timestamp, ..
            } => *event_timestamp,
            Self::PermissionCheckFailed {
                event_timestamp, ..
            } => *event_timestamp,
            Self::RequestThrottled {
                event_timestamp, ..
            } => *event_timestamp,
            Self::ProcedureDispatchAuthorized {
                dispatch_timestamp, ..
            } => *dispatch_timestamp,
        }
    }

    /// Validate that principal is not empty and trace_id is non-zero.
    pub fn is_valid(&self) -> bool {
        !self.trace_id().is_zero()
            && match self {
                Self::ContractValidated {
                    validation_details, ..
                } => {
                    !self.affected_principal_id().trim().is_empty()
                        && !validation_details.trim().is_empty()
                }
                Self::AdmissionDecision { reason, .. } => {
                    !self.affected_principal_id().trim().is_empty() && !reason.trim().is_empty()
                }
                Self::PermissionCheckFailed {
                    required_permission,
                    ..
                } => {
                    !self.affected_principal_id().trim().is_empty()
                        && !required_permission.trim().is_empty()
                }
                Self::RequestThrottled { .. } => true,
                Self::ProcedureDispatchAuthorized {
                    certificate_identity,
                    surface_plane,
                    ..
                } => {
                    !self.affected_principal_id().trim().is_empty()
                        && !certificate_identity.trim().is_empty()
                        && !surface_plane.trim().is_empty()
                }
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedure_id_is_stable_value() {
        let proc_id = ProcedureId::new(42);
        assert_eq!(proc_id.get(), 42);
        assert!(!proc_id.is_zero());
        assert!(ProcedureId::new(0).is_zero());
    }

    #[test]
    fn contract_validation_result_classification() {
        assert!(ContractValidationResult::Valid.is_valid());
        assert!(ContractValidationResult::Deprecated.is_valid());
        assert!(!ContractValidationResult::Invalid.is_valid());
    }

    #[test]
    fn admission_decision_kind_classification() {
        assert!(AdmissionDecisionKind::Accepted.is_accepted());
        assert!(!AdmissionDecisionKind::RejectedQuotaExceeded.is_accepted());
        assert!(AdmissionDecisionKind::RejectedHealthState.is_rejected());
    }
}
