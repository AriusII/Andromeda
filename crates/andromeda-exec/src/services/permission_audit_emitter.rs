//! Permission decision audit event emission for IAM hardening.
//!
//! This module implements hardened permission enforcement with explicit audit logging
//! for every authorization decision (approved and denied). It enforces the doctrine:
//! - Deny by default at every gate
//! - All permission denials logged to audit trace
//! - No silent privilege escalation
//! - Super-admin operations require explicit audit consent
//! - Wildcard permissions do not bypass explicit deny
//!
//! ## Audit Event Flow
//!
//! 1. Permission check requested (principal + required_permission)
//! 2. Decision made (allow or deny with reason)
//! 3. **Audit event emitted** (decision trace binding to principal)
//! 4. Result returned to caller
//!
//! ## Deny-by-Default Enforcement
//!
//! Every gate enforces:
//! - Unknown principal → DENY + audit
//! - Missing permission → DENY + audit
//! - Empty permission set → DENY + audit
//! - Wildcard not granted → DENY + audit
//! - Procedure ID mismatch → DENY + audit

use andromeda_core::{AndromedaResult, Permission, PrincipalId};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use std::fmt;

/// Machine-parseable audit event for permission decisions.
///
/// Each event is immutable and fully specified:
/// - No optional fields (all fields required)
/// - Principal binding is immutable at emission time
/// - Decision is deterministic (same inputs → same decision)
/// - Reason is machine-parseable (not free-form strings)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionAuditEvent {
    /// Trace correlation ID from invocation.
    pub trace_id: TraceId,

    /// Principal that was evaluated.
    pub principal_id: PrincipalId,

    /// Permission that was required.
    pub required_permission: Permission,

    /// Decision: allowed or denied with reason.
    pub decision: PermissionDecisionAudit,

    /// Timestamp of decision (for timeline correlation).
    pub timestamp: std::time::SystemTime,
}

impl PermissionAuditEvent {
    /// Create a new permission audit event for an allowed decision.
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

    /// Create a new permission audit event for a denied decision.
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

    /// Create a new permission audit event for a denied decision (unknown principal).
    pub fn denied_unknown_principal(trace_id: TraceId, required_permission: Permission) -> Self {
        Self {
            trace_id,
            principal_id: PrincipalId::new(0), // Placeholder for unknown
            required_permission,
            decision: PermissionDecisionAudit::DeniedUnknownPrincipal,
            timestamp: std::time::SystemTime::now(),
        }
    }

    /// Convert to decision trace for critical decision logging.
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

/// Permission decision for audit logging (allowed or denied).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecisionAudit {
    /// Permission was granted.
    Allowed,

    /// Permission was denied with machine-parseable reason.
    Denied(DenialAuditReason),

    /// Permission was denied because principal is unknown.
    DeniedUnknownPrincipal,
}

/// Machine-parseable denial reason for audit logging.
///
/// Each reason is deterministic and machine-parseable:
/// - Used for forensic analysis of permission denials
/// - Enables automated alerting on specific denial types
/// - Supports compliance auditing and privilege escalation detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialAuditReason {
    /// Principal has no permissions granted (empty permission set).
    NoPermissionsGranted,

    /// Required permission is not in principal's permission set.
    PermissionNotGranted,

    /// Requested procedure ID does not match granted procedure (wildcard not applicable).
    ProcedureIdMismatch,

    /// Super-admin operation attempted without explicit authorization audit record.
    SuperAdminOperationNotAudited,

    /// Wildcard permission was explicitly denied by policy.
    WildcardDeniedByPolicy,

    /// Principal's session has expired (Wave 21+).
    SessionExpired,

    /// Certificate revocation check failed (Wave 21+).
    CertificateRevoked,

    /// Internal error during permission evaluation.
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

/// Audit event emitter trait for permission decisions.
///
/// Implementations must guarantee:
/// - Every permission check emits an audit event (no silent denials)
/// - Events are immutable (no modification after emission)
/// - Events carry complete context (principal, permission, decision, reason)
/// - Events are durable (persist to audit trace before returning to caller)
pub trait PermissionAuditEmitter: Send + Sync {
    /// Emit a permission decision audit event.
    ///
    /// This must be called for every permission evaluation, both allowed and denied.
    /// Implementations must ensure the event is durably stored before returning.
    ///
    /// # Arguments
    ///
    /// * `event` - Permission audit event with complete decision context
    ///
    /// # Returns
    ///
    /// `Ok(())` if event was successfully emitted and durable
    /// `Err(AndromedaError)` if event emission failed (e.g., audit log full, I/O error)
    fn emit_permission_decision(&self, event: PermissionAuditEvent) -> AndromedaResult<()>;
}

/// No-op audit emitter for development and testing (Wave 19).
///
/// This implementation accepts all audit events without error,
/// but does not persist them. It is intended for development and testing only.
///
/// ## Wave 21+ Migration
///
/// Replace with PersistentPermissionAuditEmitter backed by:
/// - WAL-replicated audit table
/// - Immutable append-only audit log
/// - Cryptographic log sealing
pub struct NoOpPermissionAuditEmitter;

impl PermissionAuditEmitter for NoOpPermissionAuditEmitter {
    fn emit_permission_decision(&self, _event: PermissionAuditEvent) -> AndromedaResult<()> {
        // Accept all events without error; do not persist (Wave 19 development mode)
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
