//! Permission decision audit events for IAM admission.

use andromeda_core::{AndromedaResult, Permission, PrincipalId};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use std::fmt;

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

    pub fn to_decision_trace(&self) -> DecisionTrace {
        let reason = match &self.decision {
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
        };

        DecisionTrace {
            trace_id: self.trace_id,
            decision: CriticalDecisionKind::SecurityAuthorization,
            reason,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecisionAudit {
    Allowed,
    Denied(DenialAuditReason),
    DeniedUnknownPrincipal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialAuditReason {
    NoPermissionsGranted,
    PermissionNotGranted,
    ProcedureIdMismatch,
    SuperAdminOperationNotAudited,
    WildcardDeniedByPolicy,
    SessionExpired,
    CertificateRevoked,
    InternalError,
}

impl DenialAuditReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoPermissionsGranted => "no_permissions_granted",
            Self::PermissionNotGranted => "permission_not_granted",
            Self::ProcedureIdMismatch => "procedure_id_mismatch",
            Self::SuperAdminOperationNotAudited => "superadmin_operation_not_audited",
            Self::WildcardDeniedByPolicy => "wildcard_denied_by_policy",
            Self::SessionExpired => "session_expired",
            Self::CertificateRevoked => "certificate_revoked",
            Self::InternalError => "internal_error",
        }
    }

    pub fn explanation(self) -> String {
        match self {
            Self::NoPermissionsGranted => "principal has no permissions granted".to_string(),
            Self::PermissionNotGranted => {
                "required permission is not in principal's permission set".to_string()
            }
            Self::ProcedureIdMismatch => {
                "requested procedure ID does not match granted procedure".to_string()
            }
            Self::SuperAdminOperationNotAudited => {
                "super-admin operation attempted without explicit authorization audit record"
                    .to_string()
            }
            Self::WildcardDeniedByPolicy => {
                "wildcard permission was explicitly denied by policy".to_string()
            }
            Self::SessionExpired => "principal's session has expired".to_string(),
            Self::CertificateRevoked => "certificate revocation check failed".to_string(),
            Self::InternalError => "internal error during permission evaluation".to_string(),
        }
    }
}

impl fmt::Display for DenialAuditReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub trait PermissionAuditEmitter: Send + Sync {
    fn emit_permission_decision(&self, event: PermissionAuditEvent) -> AndromedaResult<()>;
}

pub struct NoOpPermissionAuditEmitter;

impl PermissionAuditEmitter for NoOpPermissionAuditEmitter {
    fn emit_permission_decision(&self, _event: PermissionAuditEvent) -> AndromedaResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_allowed() {
        let event = PermissionAuditEvent::allowed(
            TraceId::new(1),
            PrincipalId::new(100),
            Permission::ExecuteProcedure(andromeda_core::ProcedureId::new(1)),
        );

        assert!(event.is_allowed());
        assert!(!event.is_denied());
    }

    #[test]
    fn test_audit_event_denied() {
        let event = PermissionAuditEvent::denied(
            TraceId::new(1),
            PrincipalId::new(100),
            Permission::AdminShutdown,
            DenialAuditReason::PermissionNotGranted,
        );

        assert!(!event.is_allowed());
        assert!(event.is_denied());
    }

    #[test]
    fn test_audit_event_denied_unknown_principal() {
        let event = PermissionAuditEvent::denied_unknown_principal(
            TraceId::new(1),
            Permission::AdminCatalogPublish,
        );

        assert!(!event.is_allowed());
        assert!(event.is_denied());
    }

    #[test]
    fn test_denial_reason_explanation() {
        assert!(
            !DenialAuditReason::NoPermissionsGranted
                .explanation()
                .is_empty()
        );
        assert!(
            !DenialAuditReason::PermissionNotGranted
                .explanation()
                .is_empty()
        );
        assert!(
            !DenialAuditReason::ProcedureIdMismatch
                .explanation()
                .is_empty()
        );
        assert!(
            !DenialAuditReason::SuperAdminOperationNotAudited
                .explanation()
                .is_empty()
        );
        assert!(
            !DenialAuditReason::WildcardDeniedByPolicy
                .explanation()
                .is_empty()
        );
    }

    #[test]
    fn test_decision_trace_conversion() {
        let event = PermissionAuditEvent::allowed(
            TraceId::new(100),
            PrincipalId::new(42),
            Permission::ReadContractMetadata,
        );

        let trace = event.to_decision_trace();
        assert_eq!(trace.trace_id, TraceId::new(100));
        assert_eq!(trace.decision, CriticalDecisionKind::SecurityAuthorization);
        assert!(trace.reason.contains("permission allowed"));
    }

    #[test]
    fn test_noop_emitter_accepts_all_events() {
        let emitter = NoOpPermissionAuditEmitter;
        let event = PermissionAuditEvent::denied(
            TraceId::new(1),
            PrincipalId::new(100),
            Permission::AdminShutdown,
            DenialAuditReason::PermissionNotGranted,
        );

        let result = emitter.emit_permission_decision(event);
        assert!(result.is_ok());
    }
}
