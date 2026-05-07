//! Permission decision audit events for IAM admission.

mod denial;
mod emitter;
mod event;
mod evidence;
mod policy;
mod redaction;

pub use denial::DenialAuditReason;
pub use emitter::{NoOpPermissionAuditEmitter, PermissionAuditEmitter};
pub use event::{PermissionAuditEvent, PermissionDecisionAudit};
pub use evidence::{
    AuditEmissionEvidence, AuditEmissionKind, AuditEmissionOutcome, AuditSinkAvailability,
    AuditSinkDurabilityEvidence, PermissionAuditEvidence,
};
pub use policy::AuditEmissionPolicy;
pub use redaction::{audit_text_contains_sensitive_marker, redact_audit_reason};

#[cfg(test)]
mod tests;
