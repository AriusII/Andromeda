#![forbid(unsafe_code)]

mod emitter;
mod events;
mod exporters;
mod query;
mod restore_trace;

pub(crate) use andromeda_observability::{EventId, TraceId};
pub use emitter::EventEmitter;
pub use events::{
    AdminOperation, AdminOperationTrace, AdmissionAuditEvent, AdmissionDecisionKind,
    AffectedPrincipal, AuditTrace, AuthorizationDeniedTrace, BackpressureReason, BackpressureTrace,
    BackupAuditEvent, BackupAuditTrace, BackupId, CatalogMutationTrace, CertificateIdentity,
    CommitVisibleTrace, CompletionEmittedTrace, ContractRejectedTrace, ContractValidationResult,
    CorruptionBoundaryTrace, DurableAuditAppendRecord, DurableAuditCompactionReport,
    DurableAuditDecisionGate, DurableAuditEventFamily, DurableAuditFailureKind,
    DurableAuditPolicyEvidenceRequirement, DurableAuditPrincipalBinding,
    DurableAuditPruneBlockReason, DurableAuditPruneEvidence, DurableAuditRecordIdentity,
    DurableAuditReplayBehavior, DurableAuditReplayEvidence, DurableAuditReplayLsnRange,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditReplayResult,
    DurableAuditReplayWindow, DurableAuditRetentionBoundary, DurableAuditRetentionManager,
    DurableAuditRetentionPolicy, DurableAuditSinkFailure, DurableAuditSinkReport,
    DurableAuditSinkResult, DurableAuditVisibleDecisionProof, DurableAuditWalEvidence,
    DurableAuditWalSegmentArchiveProof, DurableAuditWalSink, EventEnvelope, EventSink,
    ExecutionTransitionTrace, FencingDecision, FencingEvent, FencingPolicy,
    FileDurableAuditWalSink, FrameRejectionTrace, GpuBudgetTraceEvidence,
    GpuExecutionFallbackReason, GpuExecutionJobClass, GpuExecutionOutcome, GpuExecutionTraceEvent,
    GpuPolicyDecisionTrace, GpuValidationOutcome, HadrAuditEvent, HadrAuditTrace,
    InMemoryEventSequence, InMemoryEventSink, InvocationTrace, IoBudgetDecisionTrace,
    IoPipelineStage, IoPlacementDecisionTrace, IoStorageTier, ManifestEventKind, ManifestTrace,
    MvccTrace, PendingDurableAuditRecord, Permission, PermissionFamily, PlacementAuditEvent,
    PlacementAuditTransition, ProcedureId, ProcedureLifecycleTrace, PromotionCompletion,
    PromotionEligibility, ProtocolRejectionReason, ProtocolRejectionTrace, ProtocolSurfacePlane,
    QuorumRole, RecoveryStage, RecoveryTrace, ReplicaHealthState, ResourceTrace, RestoreCompletion,
    RollbackDurableTrace, SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID,
    SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION, SchemaLayoutDecisionTrace,
    SecurityAdmissionAuditEventV0, SecurityAuditDenialReason, SecurityAuditOutcome,
    SecurityAuditTrace, SecurityPolicyVersionEvidence, StreamRoleRejectionTrace, SurfaceScope,
    TraceEvent, TransactionPhaseCode, TransactionTransitionTrace, TransitionReasonCode,
    UnsupportedVersionTrace, UserPrincipal, UserPrincipalKind, WalEventTrace, WalOperation,
    WalTrace, classify_policy_evidence_requirement,
};
pub use exporters::{
    ExportDecisionTrace, ExporterBackend, ExporterConfig, ExporterTrait, Metric, MockExporter,
    RetryPolicy,
};
pub use query::{
    DurableAuditTraceQueryResult, DurableAuditTraceQueryRow, DurableAuditTraceQuerySource,
    TraceQueryMetadata, TraceQueryResult, TraceQueryRow,
};
pub use restore_trace::{RestoreCompletionStatus, RestoreId, RestoreTrace};
