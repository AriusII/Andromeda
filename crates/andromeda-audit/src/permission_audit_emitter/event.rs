use super::denial::DenialAuditReason;
use super::evidence::{
    AuditEmissionEvidence, AuditEmissionKind, AuditEmissionOutcome, AuditSinkAvailability,
    PermissionAuditEvidence,
};
use super::policy::AuditEmissionPolicy;
use andromeda_core::{AndromedaResult, Permission, PrincipalId};
use andromeda_observability::TraceId;

const UNKNOWN_PRINCIPAL_ID: PrincipalId = PrincipalId::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionAuditEvent {
    pub trace_id: TraceId,
    pub principal_id: PrincipalId,
    pub required_permission: Permission,
    pub decision: PermissionDecisionAudit,
    pub timestamp: std::time::SystemTime,
}

impl PermissionAuditEvent {
    pub fn allowed(
        trace_id: TraceId,
        principal_id: PrincipalId,
        required_permission: Permission,
    ) -> Self {
        Self {
            trace_id,
            principal_id,
            required_permission,
            decision: PermissionDecisionAudit::Allowed,
            timestamp: std::time::SystemTime::now(),
        }
    }

    pub fn denied(
        trace_id: TraceId,
        principal_id: PrincipalId,
        required_permission: Permission,
        reason: DenialAuditReason,
    ) -> Self {
        Self {
            trace_id,
            principal_id,
            required_permission,
            decision: PermissionDecisionAudit::Denied(reason),
            timestamp: std::time::SystemTime::now(),
        }
    }

    pub fn denied_unknown_principal(trace_id: TraceId, required_permission: Permission) -> Self {
        Self {
            trace_id,
            principal_id: UNKNOWN_PRINCIPAL_ID,
            required_permission,
            decision: PermissionDecisionAudit::DeniedUnknownPrincipal,
            timestamp: std::time::SystemTime::now(),
        }
    }

    pub fn to_decision_trace(&self) -> PermissionAuditDecisionTrace {
        PermissionAuditDecisionTrace {
            trace_id: self.trace_id,
            reason: self.decision_reason(),
        }
    }

    pub fn decision_reason(&self) -> String {
        match &self.decision {
            PermissionDecisionAudit::Allowed => {
                format!(
                    "permission allowed: principal {} granted permission {}",
                    self.principal_id, self.required_permission
                )
            }
            PermissionDecisionAudit::Denied(reason) => {
                format!(
                    "permission denied: principal {} - {}",
                    self.principal_id,
                    reason.explanation()
                )
            }
            PermissionDecisionAudit::DeniedUnknownPrincipal => {
                format!(
                    "permission denied: unknown principal - permission required: {}",
                    self.required_permission
                )
            }
        }
    }

    pub fn is_allowed(&self) -> bool {
        matches!(self.decision, PermissionDecisionAudit::Allowed)
    }

    pub fn is_denied(&self) -> bool {
        matches!(
            self.decision,
            PermissionDecisionAudit::Denied(_) | PermissionDecisionAudit::DeniedUnknownPrincipal
        )
    }

    pub fn audit_evidence(
        &self,
        policy: AuditEmissionPolicy,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<PermissionAuditEvidence> {
        let outcome = if self.is_allowed() {
            AuditEmissionOutcome::Allowed
        } else {
            AuditEmissionOutcome::Denied
        };
        AuditEmissionEvidence::new(
            policy,
            self.trace_id,
            AuditEmissionKind::PermissionDecision,
            outcome,
            self.decision_reason(),
            sink,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionAuditDecisionTrace {
    pub trace_id: TraceId,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecisionAudit {
    Allowed,
    Denied(DenialAuditReason),
    DeniedUnknownPrincipal,
}
