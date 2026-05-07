use super::event::PermissionAuditEvent;
use super::evidence::{AuditSinkAvailability, PermissionAuditEvidence};
use super::policy::AuditEmissionPolicy;
use andromeda_core::AndromedaResult;

pub trait PermissionAuditEmitter: Send + Sync {
    fn emit_permission_decision(&self, event: PermissionAuditEvent) -> AndromedaResult<()>;

    fn emit_permission_decision_with_policy(
        &self,
        event: PermissionAuditEvent,
        policy: AuditEmissionPolicy,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<PermissionAuditEvidence> {
        let evidence = event.audit_evidence(policy, sink)?;
        self.emit_permission_decision(event)?;
        Ok(evidence)
    }
}

pub struct NoOpPermissionAuditEmitter;

impl PermissionAuditEmitter for NoOpPermissionAuditEmitter {
    fn emit_permission_decision(&self, _event: PermissionAuditEvent) -> AndromedaResult<()> {
        Ok(())
    }
}
