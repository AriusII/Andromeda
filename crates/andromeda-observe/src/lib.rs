#![forbid(unsafe_code)]

mod emitter;
mod events;
mod exporters;
mod principal_binding;
mod query;
mod restore_trace;

pub use andromeda_observability::{
    EventCorrelation, EventId, EventSchemaVersion, ProtocolCorrelation, ProtocolEventScope,
    TraceId, V0_EVENT_SCHEMA_VERSION,
};
pub use emitter::EventEmitter;
pub use events::{
    AdminOperation, AdminOperationTrace, AdmissionAuditEvent, AdmissionDecisionKind,
    AffectedPrincipal, AuditTrace, AuthorizationDeniedTrace, BackpressureReason, BackpressureTrace,
    BackupAuditEvent, BackupAuditTrace, BackupId, CatalogMutationTrace, CertificateIdentity,
    CommitVisibleTrace, CompletionEmittedTrace, ContractRejectedTrace, ContractValidationResult,
    CorruptionBoundaryTrace, CriticalDecisionKind, DecisionTrace, DurableAuditCompactionReport,
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
    FileDurableAuditWalSink, FrameRejectionTrace, GpuPolicyDecisionTrace, HadrAuditEvent,
    HadrAuditTrace, InMemoryEventSequence, InMemoryEventSink, InvocationTrace,
    IoBudgetDecisionTrace, IoPipelineStage, IoPlacementDecisionTrace, IoStorageTier,
    ManifestEventKind, ManifestTrace, MvccTrace, PendingDurableAuditRecord, Permission,
    PermissionFamily, PlacementAuditEvent, PlacementAuditTransition, ProcedureId,
    ProcedureLifecycleTrace, PromotionCompletion, PromotionEligibility, ProtocolRejectionReason,
    ProtocolRejectionTrace, ProtocolSurfacePlane, QuorumRole, RecoveryStage, RecoveryTrace,
    ReplicaHealthState, ResourceTrace, RestoreCompletion, RollbackDurableTrace,
    SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID, SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION,
    SchemaLayoutDecisionTrace, SecurityAdmissionAuditEventV0, SecurityAuditDenialReason,
    SecurityAuditOutcome, SecurityAuditTrace, SecurityPolicyVersionEvidence,
    StreamRoleRejectionTrace, SurfaceScope, TraceEvent, TransactionPhaseCode,
    TransactionTransitionTrace, TransitionReasonCode, UnsupportedVersionTrace, UserPrincipal,
    UserPrincipalKind, WalEventTrace, WalOperation, WalTrace, classify_policy_evidence_requirement,
};
pub use exporters::{
    ExportDecisionTrace, ExporterBackend, ExporterConfig, ExporterTrait, Metric, MockExporter,
    RetryPolicy,
};
pub use principal_binding::{
    AuthorizationDenialReason, AuthorizationOutcome, PrincipalBinding, PrincipalRegistry,
    SurfaceAction, SurfaceAuthorizer, core_principal_to_observe_user_principal,
    observe_user_principal_to_core,
};
pub use query::{
    DurableAuditTraceQueryResult, DurableAuditTraceQueryRow, DurableAuditTraceQuerySource,
    TRACE_QUERY_DEFAULT_LIMIT, TRACE_QUERY_MAX_LIMIT, TraceEventFamily, TraceQueryFilter,
    TraceQueryLsnRange, TraceQueryMetadata, TraceQueryPermissionMatrix, TraceQueryResult,
    TraceQueryRow, TraceQuerySpec,
};
pub use restore_trace::{RestoreCompletionStatus, RestoreId, RestoreTrace};
