//! Observability event contracts.
//!
//! Every emitted event is tied to a non-zero [`TraceId`] and validated through
//! [`EventEnvelope::validate`] before it enters a sink. The important invariants
//! live with the specific event modules: recovery events carry durable LSN
//! evidence, security/admission events are observable on both allow and deny
//! paths, and text fields reject obvious secret markers.

#[cfg(test)]
use crate::TraceId;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
#[cfg(test)]
use andromeda_types::{CatalogObjectId, CatalogVersion, InvocationId, TransactionId};

mod decision;
mod durability;
mod durable_audit;
mod envelope;
mod family;
mod protocol;
mod protocol_rejection;
mod sequence;
mod sink;
mod transition;

pub use andromeda_audit::{
    AdminOperation, AdminOperationTrace, AdmissionAuditEvent, AdmissionDecisionKind,
    AffectedPrincipal, AuditTrace, BackpressureReason, BackupAuditEvent, BackupAuditTrace,
    BackupId, CertificateIdentity, ContractValidationResult, FencingDecision, FencingEvent,
    FencingPolicy, HadrAuditEvent, HadrAuditTrace, Permission, PermissionFamily, ProcedureId,
    PromotionCompletion, PromotionEligibility, QuorumRole, RecoveryStage, ReplicaHealthState,
    RestoreCompletion, SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID,
    SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION, SecurityAdmissionAuditEventV0,
    SecurityAuditDenialReason, SecurityAuditOutcome, SecurityAuditTrace,
    SecurityPolicyVersionEvidence, SurfaceScope, UserPrincipal, UserPrincipalKind,
};
#[cfg(test)]
use andromeda_observability::ProtocolCorrelation;
pub(crate) use andromeda_observability::{EventCorrelation, EventId, ProtocolEventScope};
pub use andromeda_observability::{InvocationTrace, MvccTrace, ResourceTrace};
pub use decision::{
    CriticalDecisionKind, DecisionTrace, GpuBudgetTraceEvidence, GpuExecutionFallbackReason,
    GpuExecutionJobClass, GpuExecutionOutcome, GpuExecutionTraceEvent, GpuPolicyDecisionTrace,
    GpuValidationOutcome, IoBudgetDecisionTrace, IoPipelineStage, IoPlacementDecisionTrace,
    IoStorageTier, PlacementAuditEvent, PlacementAuditTransition, SchemaLayoutDecisionTrace,
};
pub use durability::{
    CatalogMutationTrace, CommitVisibleTrace, CorruptionBoundaryTrace, ManifestEventKind,
    ManifestTrace, RecoveryTrace, RollbackDurableTrace, WalEventTrace, WalOperation, WalTrace,
};
pub use durable_audit::{
    DurableAuditAppendRecord, DurableAuditCompactionReport, DurableAuditDecisionGate,
    DurableAuditEventFamily, DurableAuditFailureKind, DurableAuditPolicyEvidenceRequirement,
    DurableAuditPrincipalBinding, DurableAuditPruneBlockReason, DurableAuditPruneEvidence,
    DurableAuditRecordIdentity, DurableAuditReplayBehavior, DurableAuditReplayEvidence,
    DurableAuditReplayLsnRange, DurableAuditReplayQuery, DurableAuditReplayRecord,
    DurableAuditReplayResult, DurableAuditReplayWindow, DurableAuditRetentionBoundary,
    DurableAuditRetentionManager, DurableAuditRetentionPolicy, DurableAuditSinkFailure,
    DurableAuditSinkReport, DurableAuditSinkResult, DurableAuditVisibleDecisionProof,
    DurableAuditWalEvidence, DurableAuditWalSegmentArchiveProof, DurableAuditWalSink,
    FileDurableAuditWalSink, PendingDurableAuditRecord, classify_policy_evidence_requirement,
};
pub use envelope::EventEnvelope;
pub use family::TraceEvent;
pub use protocol::{
    AuthorizationDeniedTrace, BackpressureTrace, CompletionEmittedTrace, ContractRejectedTrace,
    FrameRejectionTrace, StreamRoleRejectionTrace, UnsupportedVersionTrace,
};
pub use protocol_rejection::{
    ProtocolRejectionReason, ProtocolRejectionTrace, ProtocolSurfacePlane,
};
pub use sequence::{InMemoryEventSequence, ProcedureLifecycleTrace};
pub use sink::{EventSink, InMemoryEventSink};
pub use transition::{
    ExecutionTransitionTrace, TransactionPhaseCode, TransactionTransitionTrace,
    TransitionReasonCode,
};

pub(crate) fn observe_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Internal, message)
}

pub(super) fn non_empty_reason(reason: impl Into<String>) -> AndromedaResult<String> {
    let reason = reason.into();
    if reason.trim().is_empty() {
        return Err(observe_error(
            "observability decision evidence requires a non-empty reason",
        ));
    }

    Ok(reason)
}

pub(super) fn contains_sensitive_marker(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "-----begin",
        "private key",
        "private_key",
        "bearer ",
        "credential=",
        "password=",
        "passwd=",
        "secret=",
        "token=",
        "authorization:",
        "x-api-key",
        "payload:",
        "payload body",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

#[cfg(test)]
mod tests;
