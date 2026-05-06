//! Observability event contracts.
//!
//! Every emitted event is tied to a non-zero [`TraceId`] and validated through
//! [`EventEnvelope::validate`] before it enters a sink. The important invariants
//! live with the specific event modules: recovery events carry durable LSN
//! evidence, security/admission events are observable on both allow and deny
//! paths, and text fields reject obvious secret markers.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    InvocationId, TransactionId,
};

use crate::TraceId;

mod admission_audit;
mod audit;
mod backup_audit;
mod correlation;
mod decision;
mod durable_audit;
mod envelope;
mod hadr_audit;
mod protocol_rejection;
mod sequence;
mod sink;
mod transition;

pub use admission_audit::*;
pub use audit::*;
pub use backup_audit::*;
pub use correlation::*;
pub use decision::*;
pub use durable_audit::*;
pub use envelope::*;
pub use hadr_audit::*;
pub use protocol_rejection::*;
pub use sequence::*;
pub use sink::*;
pub use transition::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(u128);

impl EventId {
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u128 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventSchemaVersion(u16);

impl EventSchemaVersion {
    pub const V0: Self = Self(1);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    pub const fn is_v0(self) -> bool {
        self.0 == Self::V0.0
    }
}

pub const V0_EVENT_SCHEMA_VERSION: EventSchemaVersion = EventSchemaVersion::V0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvocationTrace {
    pub trace_id: TraceId,
    pub invocation_id: InvocationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalTrace {
    pub trace_id: TraceId,
    pub transaction_id: Option<TransactionId>,
    pub durable_lsn: u64,
}

impl WalTrace {
    pub const fn proves_durable_boundary(self) -> bool {
        self.durable_lsn != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalOperation {
    Append,
    Flush,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalEventTrace {
    pub trace_id: TraceId,
    pub transaction_id: Option<TransactionId>,
    pub operation: WalOperation,
    pub appended_lsn: u64,
    pub durable_lsn: Option<u64>,
}

impl WalEventTrace {
    pub const fn has_lsn_evidence(self) -> bool {
        self.appended_lsn != 0
            && match self.operation {
                WalOperation::Append => true,
                WalOperation::Flush => match self.durable_lsn {
                    Some(durable_lsn) => durable_lsn != 0,
                    None => false,
                },
            }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitVisibleTrace {
    pub trace_id: TraceId,
    pub transaction_id: TransactionId,
    pub durable_commit_lsn: u64,
}

impl CommitVisibleTrace {
    pub const fn proves_wal_before_visible_commit(self) -> bool {
        self.durable_commit_lsn != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollbackDurableTrace {
    pub trace_id: TraceId,
    pub transaction_id: TransactionId,
    pub durable_rollback_lsn: u64,
}

impl RollbackDurableTrace {
    pub const fn proves_durable_rollback(self) -> bool {
        self.durable_rollback_lsn != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryTrace {
    pub trace_id: TraceId,
    pub last_durable_lsn: u64,
    pub corruption_boundary_lsn: Option<u64>,
}

impl RecoveryTrace {
    pub const fn proves_recovery_boundary(self) -> bool {
        self.last_durable_lsn != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestEventKind {
    Validation,
    Switch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestTrace {
    pub trace_id: TraceId,
    pub event: ManifestEventKind,
    pub catalog_version: CatalogVersion,
    pub manifest_epoch: u64,
    pub base_checkpoint_lsn: u64,
    pub required_wal_start_lsn: u64,
    pub accepted: bool,
    pub reason: String,
}

impl ManifestTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_wal_anchor_evidence(&self) -> bool {
        self.manifest_epoch != 0
            && self.required_wal_start_lsn != 0
            && self.required_wal_start_lsn >= self.base_checkpoint_lsn
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogMutationTrace {
    pub trace_id: TraceId,
    pub catalog_version: CatalogVersion,
    pub object_id: Option<CatalogObjectId>,
    pub action: String,
}

impl CatalogMutationTrace {
    pub fn has_action(&self) -> bool {
        !self.action.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameRejectionTrace {
    pub trace_id: TraceId,
    pub scope: ProtocolEventScope,
    pub protocol: ProtocolCorrelation,
    pub reason: String,
}

impl FrameRejectionTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_frame_evidence(&self) -> bool {
        self.protocol.has_frame_evidence()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRoleRejectionTrace {
    pub trace_id: TraceId,
    pub scope: ProtocolEventScope,
    pub stream_id: Option<u64>,
    pub observed_role: Option<u16>,
    pub expected_role: Option<u16>,
    pub reason: String,
}

impl StreamRoleRejectionTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_role_evidence(&self) -> bool {
        self.stream_id.is_some() && self.observed_role.is_some() && self.expected_role.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackpressureTrace {
    pub trace_id: TraceId,
    pub scope: ProtocolEventScope,
    pub protocol: ProtocolCorrelation,
    pub retry_after_micros: Option<u64>,
    pub pending_units: Option<u64>,
    pub limit_units: Option<u64>,
    pub reason: String,
}

impl BackpressureTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_pressure_evidence(&self) -> bool {
        self.retry_after_micros.is_some()
            || (self.pending_units.is_some() && self.limit_units.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionEmittedTrace {
    pub trace_id: TraceId,
    pub protocol: ProtocolCorrelation,
    pub completion_code: Option<u16>,
    pub committed: bool,
    pub durable_lsn: Option<u64>,
    pub reason: String,
}

impl CompletionEmittedTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_completion_evidence(&self) -> bool {
        self.completion_code.is_some()
    }

    pub const fn proves_committed_completion(&self) -> bool {
        !self.committed
            || match self.durable_lsn {
                Some(durable_lsn) => durable_lsn != 0,
                None => false,
            }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractRejectedTrace {
    pub trace_id: TraceId,
    pub protocol: ProtocolCorrelation,
    pub contract_kind: Option<u16>,
    pub rejection_code: Option<u16>,
    pub reason: String,
}

impl ContractRejectedTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_contract_evidence(&self) -> bool {
        self.contract_kind.is_some() && self.rejection_code.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationDeniedTrace {
    pub trace_id: TraceId,
    pub denied_permission: String,
    pub reason: String,
}

impl AuthorizationDeniedTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub fn has_permission_evidence(&self) -> bool {
        !self.denied_permission.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedVersionTrace {
    pub trace_id: TraceId,
    pub scope: ProtocolEventScope,
    pub offered_version: Option<u16>,
    pub min_supported_version: Option<u16>,
    pub max_supported_version: Option<u16>,
    pub reason: String,
}

impl UnsupportedVersionTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_version_evidence(&self) -> bool {
        match (
            self.offered_version,
            self.min_supported_version,
            self.max_supported_version,
        ) {
            (Some(_), Some(min_supported), Some(max_supported)) => min_supported <= max_supported,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorruptionBoundaryTrace {
    pub trace_id: TraceId,
    pub boundary_lsn: u64,
    pub reason: String,
}

impl CorruptionBoundaryTrace {
    pub fn proves_boundary(&self) -> bool {
        self.boundary_lsn != 0 && !self.reason.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MvccTrace {
    pub trace_id: TraceId,
    pub snapshot_ts: u64,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditTrace {
    pub trace_id: TraceId,
    pub actor: String,
    pub object: String,
    pub action: String,
}

impl AuditTrace {
    pub fn is_complete(&self) -> bool {
        !self.actor.trim().is_empty()
            && !self.object.trim().is_empty()
            && !self.action.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceTrace {
    pub trace_id: TraceId,
    pub memory_bytes: u64,
    pub temp_bytes: u64,
}

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
    TransactionTransition(TransactionTransitionTrace),
    ExecutionTransition(ExecutionTransitionTrace),
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
            Self::TransactionTransition(trace) => trace.trace_id,
            Self::ExecutionTransition(trace) => trace.trace_id,
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
            Self::TransactionTransition(_) => CriticalDecisionKind::TransactionTransition,
            Self::ExecutionTransition(_) => CriticalDecisionKind::ExecutionTransition,
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
        "password=",
        "passwd=",
        "secret=",
        "token=",
        "authorization:",
        "payload:",
        "payload body",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

#[cfg(test)]
mod tests;
