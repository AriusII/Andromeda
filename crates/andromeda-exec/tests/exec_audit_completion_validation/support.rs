pub use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId, Permission, PrincipalId,
    TransactionId,
};
pub use andromeda_exec::dispatch::permission_validation::{
    validate_dispatch_permissions_with_audit, validate_dispatch_permissions_with_durable_audit,
};
pub use andromeda_exec::services::permission_audit_emitter::{
    AuditEmissionEvidence, AuditEmissionKind, AuditEmissionOutcome, AuditEmissionPolicy,
    AuditSinkAvailability, DenialAuditReason, NoOpPermissionAuditEmitter, PermissionAuditEmitter,
    PermissionAuditEvent, audit_text_contains_sensitive_marker,
};
pub use andromeda_exec::{
    CompletionEmission, CompletionMappingService, CompletionStatus, InvocationCompletionEmitter,
};
pub use andromeda_observe::{
    DurableAuditEventFamily, DurableAuditRecordIdentity, DurableAuditReplayBehavior,
    DurableAuditRetentionBoundary, DurableAuditSinkReport, DurableAuditWalEvidence, EventId,
    TraceId,
};
pub use andromeda_storage::Lsn;
pub use andromeda_tx::TransactionState;

pub(crate) fn durable_audit_report(
    trace_id: TraceId,
    family: DurableAuditEventFamily,
    record_lsn: u64,
) -> DurableAuditSinkReport {
    DurableAuditSinkReport {
        identity: DurableAuditRecordIdentity {
            event_id: EventId::new(100_000 + record_lsn as u128),
            trace_id,
            family,
            sequence_number: record_lsn,
        },
        evidence: DurableAuditWalEvidence {
            record_lsn,
            durable_lsn: record_lsn,
            checksum: 0xABCD_0000 + record_lsn,
        },
        replay_behavior: DurableAuditReplayBehavior::RebuildDecisionIndex,
        retention: retention_boundary_for_family(family),
    }
}

pub(crate) fn retention_boundary_for_family(
    family: DurableAuditEventFamily,
) -> DurableAuditRetentionBoundary {
    match family {
        DurableAuditEventFamily::CatalogDecision => DurableAuditRetentionBoundary::CatalogVersion,
        DurableAuditEventFamily::BackupDecision | DurableAuditEventFamily::RestoreDecision => {
            DurableAuditRetentionBoundary::WalSegment
        }
        DurableAuditEventFamily::ForensicDecision => DurableAuditRetentionBoundary::ForensicHold,
        _ => DurableAuditRetentionBoundary::SecurityPolicy,
    }
}
