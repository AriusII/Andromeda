//! Compatibility surface for permission audit ownership.

pub use andromeda_audit::{
    AuditEmissionEventFamily, AuditEmissionEvidence, AuditEmissionKind, AuditEmissionOutcome,
    AuditEmissionPolicy, AuditEmissionReplayBehavior, AuditEmissionRetentionBoundary,
    AuditSinkAvailability, AuditSinkDurabilityEvidence, AuditSinkDurabilityReport,
    DenialAuditReason, NoOpPermissionAuditEmitter, PermissionAuditDecisionTrace,
    PermissionAuditEmitter, PermissionAuditEvent, PermissionAuditEvidence, PermissionDecisionAudit,
    audit_emission_error, audit_text_contains_sensitive_marker, redact_audit_reason,
};
