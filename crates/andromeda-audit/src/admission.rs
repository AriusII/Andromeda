//! Admission audit event contracts.
//!
//! Covers contract validation, admission decisions, permission failures,
//! throttling, and dispatch authorization.

use std::time::SystemTime;

use andromeda_observability::TraceId;
use andromeda_security_contract::{
    Permission as SecurityContractPermission, SecurityAdmissionEvidenceCodeV0,
    SecurityAdmissionOutcomeV0, SecurityAdmissionReasonCodeV0, SecurityAdmissionStepV0,
    SecurityAdmissionV0, SecuritySurface, SurfaceClass,
};

fn has_text(value: &str) -> bool {
    !value.trim().is_empty()
}

pub const SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID: &str =
    "andromeda.audit.security_admission.v0";
pub const SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION: u16 = 0;

/// Admission procedure identity.
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

/// Contract validation outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContractValidationResult {
    Valid,
    Invalid,
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

/// Admission gate outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdmissionDecisionKind {
    Accepted,
    RejectedQuotaExceeded,
    RejectedHealthState,
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

/// Runtime-free audit projection for a security admission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecurityAdmissionAuditEventV0 {
    admission: SecurityAdmissionV0,
    surface: SecuritySurface,
    class: SurfaceClass,
    permission: SecurityContractPermission,
    policy_evidence_present: bool,
}

impl SecurityAdmissionAuditEventV0 {
    pub const fn new(
        admission: SecurityAdmissionV0,
        surface: SecuritySurface,
        class: SurfaceClass,
        permission: SecurityContractPermission,
        policy_evidence_present: bool,
    ) -> Self {
        Self {
            admission,
            surface,
            class,
            permission,
            policy_evidence_present,
        }
    }

    pub const fn schema_id(self) -> &'static str {
        SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID
    }

    pub const fn schema_version(self) -> u16 {
        SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION
    }

    pub const fn family(self) -> &'static str {
        "security_admission"
    }

    pub const fn phase(self) -> &'static str {
        "pre_transaction_admission"
    }

    pub const fn admission(self) -> SecurityAdmissionV0 {
        self.admission
    }

    pub const fn step(self) -> SecurityAdmissionStepV0 {
        self.admission.step()
    }

    pub const fn evidence(self) -> SecurityAdmissionEvidenceCodeV0 {
        self.admission.evidence()
    }

    pub const fn outcome(self) -> SecurityAdmissionOutcomeV0 {
        self.admission.outcome()
    }

    pub const fn reason_code(self) -> SecurityAdmissionReasonCodeV0 {
        self.admission.reason_code()
    }

    pub const fn surface(self) -> SecuritySurface {
        self.surface
    }

    pub const fn class(self) -> SurfaceClass {
        self.class
    }

    pub const fn permission(self) -> SecurityContractPermission {
        self.permission
    }

    pub const fn policy_evidence_present(self) -> bool {
        self.policy_evidence_present
    }

    pub const fn is_allowed(self) -> bool {
        self.admission.is_allowed()
    }

    pub const fn is_denied(self) -> bool {
        !self.is_allowed()
    }
}

/// Backpressure reason code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackpressureReason {
    BufferPoolFull,
    NetworkCongestion,
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

/// Principal bound at event emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedPrincipal {
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
    ContractValidated {
        trace_id: TraceId,
        procedure_id: ProcedureId,
        input_row_count: u64,
        result: ContractValidationResult,
        validation_details: String,
        affected_principal: AffectedPrincipal,
        event_timestamp: SystemTime,
    },

    /// Admission decision reached.
    AdmissionDecision {
        trace_id: TraceId,
        procedure_id: ProcedureId,
        decision: AdmissionDecisionKind,
        reason: String,
        affected_principal: AffectedPrincipal,
        event_timestamp: SystemTime,
    },

    /// Permission check failed during authorization.
    PermissionCheckFailed {
        trace_id: TraceId,
        procedure_id: ProcedureId,
        affected_principal: AffectedPrincipal,
        required_permission: String,
        actual_permission_set: String,
        event_timestamp: SystemTime,
    },

    /// Request throttled or rate-limited.
    RequestThrottled {
        trace_id: TraceId,
        procedure_id: ProcedureId,
        backpressure_reason: BackpressureReason,
        retry_after_ms: u64,
        event_timestamp: SystemTime,
    },

    /// Procedure dispatch authorized.
    ProcedureDispatchAuthorized {
        trace_id: TraceId,
        procedure_id: ProcedureId,
        surface_plane: String,
        certificate_identity: String,
        affected_principal: AffectedPrincipal,
        dispatch_timestamp: SystemTime,
    },
}

impl AdmissionAuditEvent {
    /// Stable event type label.
    pub fn type_label(&self) -> &'static str {
        match self {
            Self::ContractValidated { .. } => "contract_validated",
            Self::AdmissionDecision { .. } => "admission_decision",
            Self::PermissionCheckFailed { .. } => "permission_check_failed",
            Self::RequestThrottled { .. } => "request_throttled",
            Self::ProcedureDispatchAuthorized { .. } => "procedure_dispatch_authorized",
        }
    }

    /// Affected principal when the event carries one.
    pub fn affected_principal_id(&self) -> &str {
        match self {
            Self::ContractValidated {
                affected_principal, ..
            }
            | Self::AdmissionDecision {
                affected_principal, ..
            }
            | Self::PermissionCheckFailed {
                affected_principal, ..
            }
            | Self::ProcedureDispatchAuthorized {
                affected_principal, ..
            } => affected_principal.principal_id.as_str(),
            Self::RequestThrottled { .. } => "",
        }
    }

    /// Event trace id.
    pub fn trace_id(&self) -> TraceId {
        match self {
            Self::ContractValidated { trace_id, .. }
            | Self::AdmissionDecision { trace_id, .. }
            | Self::PermissionCheckFailed { trace_id, .. }
            | Self::RequestThrottled { trace_id, .. }
            | Self::ProcedureDispatchAuthorized { trace_id, .. } => *trace_id,
        }
    }

    /// Event procedure id.
    pub fn procedure_id(&self) -> ProcedureId {
        match self {
            Self::ContractValidated { procedure_id, .. }
            | Self::AdmissionDecision { procedure_id, .. }
            | Self::PermissionCheckFailed { procedure_id, .. }
            | Self::RequestThrottled { procedure_id, .. }
            | Self::ProcedureDispatchAuthorized { procedure_id, .. } => *procedure_id,
        }
    }

    /// Event timestamp.
    pub fn event_timestamp(&self) -> SystemTime {
        match self {
            Self::ContractValidated {
                event_timestamp, ..
            }
            | Self::AdmissionDecision {
                event_timestamp, ..
            }
            | Self::PermissionCheckFailed {
                event_timestamp, ..
            }
            | Self::RequestThrottled {
                event_timestamp, ..
            } => *event_timestamp,
            Self::ProcedureDispatchAuthorized {
                dispatch_timestamp, ..
            } => *dispatch_timestamp,
        }
    }

    /// Validate required identity and reason evidence.
    pub fn is_valid(&self) -> bool {
        !self.trace_id().is_zero()
            && match self {
                Self::ContractValidated {
                    affected_principal,
                    validation_details,
                    ..
                } => !affected_principal.is_empty() && has_text(validation_details),
                Self::AdmissionDecision {
                    affected_principal,
                    reason,
                    ..
                } => !affected_principal.is_empty() && has_text(reason),
                Self::PermissionCheckFailed {
                    affected_principal,
                    required_permission,
                    ..
                } => !affected_principal.is_empty() && has_text(required_permission),
                Self::RequestThrottled { .. } => true,
                Self::ProcedureDispatchAuthorized {
                    affected_principal,
                    certificate_identity,
                    surface_plane,
                    ..
                } => {
                    !affected_principal.is_empty()
                        && has_text(certificate_identity)
                        && has_text(surface_plane)
                }
            }
    }
}
