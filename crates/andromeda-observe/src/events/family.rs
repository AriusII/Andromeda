use crate::TraceId;

use super::{
    AdminOperationTrace, AuditTrace, AuthorizationDeniedTrace, BackpressureTrace,
    CatalogMutationTrace, CommitVisibleTrace, CompletionEmittedTrace, ContractRejectedTrace,
    CorruptionBoundaryTrace, CriticalDecisionKind, DecisionTrace, ExecutionTransitionTrace,
    FrameRejectionTrace, GpuExecutionTraceEvent, GpuPolicyDecisionTrace, HadrAuditTrace,
    InvocationTrace, IoBudgetDecisionTrace, IoPlacementDecisionTrace, ManifestEventKind,
    ManifestTrace, MvccTrace, PlacementAuditEvent, ProtocolEventScope, RecoveryTrace,
    ResourceTrace, RollbackDurableTrace, SchemaLayoutDecisionTrace, SecurityAuditOutcome,
    SecurityAuditTrace, StreamRoleRejectionTrace, TransactionTransitionTrace,
    UnsupportedVersionTrace, WalEventTrace, WalOperation, WalTrace,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceEvent {
    Decision(DecisionTrace),
    Invocation(InvocationTrace),
    Wal(WalTrace),
    WalEvent(WalEventTrace),
    CommitVisible(CommitVisibleTrace),
    RollbackDurable(RollbackDurableTrace),
    RecoveryStartup(RecoveryTrace),
    Manifest(ManifestTrace),
    CatalogMutation(CatalogMutationTrace),
    FrameRejection(FrameRejectionTrace),
    StreamRoleRejection(StreamRoleRejectionTrace),
    Backpressure(BackpressureTrace),
    CompletionEmitted(CompletionEmittedTrace),
    ContractRejected(ContractRejectedTrace),
    AuthorizationDenied(AuthorizationDeniedTrace),
    SecurityAudit(SecurityAuditTrace),
    AdminOperation(AdminOperationTrace),
    UnsupportedVersion(UnsupportedVersionTrace),
    SchemaLayoutDecision(SchemaLayoutDecisionTrace),
    CorruptionBoundary(CorruptionBoundaryTrace),
    Mvcc(MvccTrace),
    Audit(AuditTrace),
    Resource(ResourceTrace),
    IoPlacementDecision(IoPlacementDecisionTrace),
    PlacementAudit(PlacementAuditEvent),
    IoBudgetDecision(IoBudgetDecisionTrace),
    GpuPolicyDecision(GpuPolicyDecisionTrace),
    GpuExecution(GpuExecutionTraceEvent),
    TransactionTransition(TransactionTransitionTrace),
    ExecutionTransition(ExecutionTransitionTrace),
    HadrCluster(HadrAuditTrace),
}

impl TraceEvent {
    pub fn trace_id(&self) -> TraceId {
        match self {
            Self::Decision(trace) => trace.trace_id,
            Self::Invocation(trace) => trace.trace_id,
            Self::Wal(trace) => trace.trace_id,
            Self::WalEvent(trace) => trace.trace_id,
            Self::CommitVisible(trace) => trace.trace_id,
            Self::RollbackDurable(trace) => trace.trace_id,
            Self::RecoveryStartup(trace) => trace.trace_id,
            Self::Manifest(trace) => trace.trace_id,
            Self::CatalogMutation(trace) => trace.trace_id,
            Self::FrameRejection(trace) => trace.trace_id,
            Self::StreamRoleRejection(trace) => trace.trace_id,
            Self::Backpressure(trace) => trace.trace_id,
            Self::CompletionEmitted(trace) => trace.trace_id,
            Self::ContractRejected(trace) => trace.trace_id,
            Self::AuthorizationDenied(trace) => trace.trace_id,
            Self::SecurityAudit(trace) => trace.trace_id,
            Self::AdminOperation(trace) => trace.trace_id,
            Self::UnsupportedVersion(trace) => trace.trace_id,
            Self::SchemaLayoutDecision(trace) => trace.trace_id,
            Self::CorruptionBoundary(trace) => trace.trace_id,
            Self::Mvcc(trace) => trace.trace_id,
            Self::Audit(trace) => trace.trace_id,
            Self::Resource(trace) => trace.trace_id,
            Self::IoPlacementDecision(trace) => trace.trace_id,
            Self::PlacementAudit(trace) => trace.trace_id,
            Self::IoBudgetDecision(trace) => trace.trace_id,
            Self::GpuPolicyDecision(trace) => trace.trace_id,
            Self::GpuExecution(trace) => trace.trace_id,
            Self::TransactionTransition(trace) => trace.trace_id,
            Self::ExecutionTransition(trace) => trace.trace_id,
            Self::HadrCluster(trace) => trace.trace_id,
        }
    }

    pub fn kind(&self) -> CriticalDecisionKind {
        match self {
            Self::Decision(trace) => trace.decision,
            Self::Invocation(_) => CriticalDecisionKind::ContractValidation,
            Self::Wal(_) => CriticalDecisionKind::WalFlush,
            Self::WalEvent(trace) => match trace.operation {
                WalOperation::Append => CriticalDecisionKind::WalAppend,
                WalOperation::Flush => CriticalDecisionKind::WalFlush,
            },
            Self::CommitVisible(_) => CriticalDecisionKind::CommitVisible,
            Self::RollbackDurable(_) => CriticalDecisionKind::RollbackDurable,
            Self::RecoveryStartup(_) => CriticalDecisionKind::RecoveryStartup,
            Self::Manifest(trace) => match trace.event {
                ManifestEventKind::Validation => CriticalDecisionKind::ManifestValidation,
                ManifestEventKind::Switch => CriticalDecisionKind::ManifestSwitch,
            },
            Self::CatalogMutation(_) => CriticalDecisionKind::CatalogMutation,
            Self::FrameRejection(_) => CriticalDecisionKind::FrameRejection,
            Self::StreamRoleRejection(_) => CriticalDecisionKind::StreamRoleRejection,
            Self::Backpressure(_) => CriticalDecisionKind::Backpressure,
            Self::CompletionEmitted(_) => CriticalDecisionKind::CompletionEmitted,
            Self::ContractRejected(_) => CriticalDecisionKind::ContractRejected,
            Self::AuthorizationDenied(_) => CriticalDecisionKind::AuthorizationDenial,
            Self::SecurityAudit(_) => CriticalDecisionKind::SecurityAudit,
            Self::AdminOperation(_) => CriticalDecisionKind::AdminOperation,
            Self::UnsupportedVersion(_) => CriticalDecisionKind::UnsupportedVersion,
            Self::SchemaLayoutDecision(_) => CriticalDecisionKind::SchemaLayoutDecision,
            Self::CorruptionBoundary(_) => CriticalDecisionKind::CorruptionBoundary,
            Self::Mvcc(_) => CriticalDecisionKind::MvccVisibility,
            Self::Audit(_) => CriticalDecisionKind::SecurityAuthorization,
            Self::Resource(_) => CriticalDecisionKind::ResourceGovernance,
            Self::IoPlacementDecision(_) => CriticalDecisionKind::IoPlacementDecision,
            Self::PlacementAudit(_) => CriticalDecisionKind::PlacementAudit,
            Self::IoBudgetDecision(_) => CriticalDecisionKind::IoBudgetValidation,
            Self::GpuPolicyDecision(_) => CriticalDecisionKind::GpuPolicyDecision,
            Self::GpuExecution(_) => CriticalDecisionKind::GpuExecutionTrace,
            Self::TransactionTransition(_) => CriticalDecisionKind::TransactionTransition,
            Self::ExecutionTransition(_) => CriticalDecisionKind::ExecutionTransition,
            Self::HadrCluster(_) => CriticalDecisionKind::RecoveryStartup,
        }
    }

    pub const fn protocol_scope(&self) -> Option<ProtocolEventScope> {
        match self {
            Self::FrameRejection(trace) => Some(trace.scope),
            Self::StreamRoleRejection(trace) => Some(trace.scope),
            Self::Backpressure(trace) => Some(trace.scope),
            Self::CompletionEmitted(_) => Some(ProtocolEventScope::Request),
            Self::ContractRejected(_) => Some(ProtocolEventScope::Request),
            Self::UnsupportedVersion(trace) => Some(trace.scope),
            Self::SchemaLayoutDecision(trace) => Some(trace.scope),
            _ => None,
        }
    }

    pub const fn requires_request_session_correlation(&self) -> bool {
        matches!(
            self,
            Self::CompletionEmitted(_)
                | Self::ContractRejected(_)
                | Self::AuthorizationDenied(_)
                | Self::SecurityAudit(_)
                | Self::AdminOperation(_)
                | Self::SchemaLayoutDecision(SchemaLayoutDecisionTrace {
                    scope: ProtocolEventScope::Request,
                    ..
                })
                | Self::FrameRejection(FrameRejectionTrace {
                    scope: ProtocolEventScope::Request,
                    ..
                })
                | Self::StreamRoleRejection(StreamRoleRejectionTrace {
                    scope: ProtocolEventScope::Request,
                    ..
                })
        )
    }

    pub const fn must_not_have_transaction_correlation(&self) -> bool {
        matches!(
            self,
            Self::ContractRejected(_)
                | Self::AuthorizationDenied(_)
                | Self::SecurityAudit(SecurityAuditTrace {
                    outcome: SecurityAuditOutcome::Denied,
                    ..
                })
        )
    }
}
