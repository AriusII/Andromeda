use super::event::PermissionAuditEvent;
use super::evidence::{AuditSinkAvailability, PermissionAuditEvidence, audit_emission_error};
use super::policy::AuditEmissionPolicy;
use andromeda_error::AndromedaResult;

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

/// Test-support emitter that never persists audit records.
/// It rejects durable-audit policies so it cannot satisfy visible decisions.
#[derive(Debug, Clone, Copy)]
#[must_use = "no-op permission audit emitters are test support and do not persist audit records"]
pub struct NoOpPermissionAuditEmitter {
    _test_only: (),
}

impl NoOpPermissionAuditEmitter {
    /// Builds the no-op emitter for tests that exercise audit validation only.
    pub const fn new_for_tests() -> Self {
        Self { _test_only: () }
    }
}

impl PermissionAuditEmitter for NoOpPermissionAuditEmitter {
    fn emit_permission_decision(&self, _event: PermissionAuditEvent) -> AndromedaResult<()> {
        Err(audit_emission_error(
            "test-only no-op permission audit emitter cannot emit audit records directly",
        ))
    }

    fn emit_permission_decision_with_policy(
        &self,
        event: PermissionAuditEvent,
        policy: AuditEmissionPolicy,
        sink: AuditSinkAvailability,
    ) -> AndromedaResult<PermissionAuditEvidence> {
        if policy.requires_durable_wal_evidence() {
            return Err(audit_emission_error(
                "test-only no-op permission audit emitter cannot satisfy durable audit policy",
            ));
        }

        let evidence = event.audit_evidence(policy, sink)?;
        Ok(evidence)
    }
}
