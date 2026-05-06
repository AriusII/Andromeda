//! # Observability Event Types and Audit Trace Specifications
//!
//! This module defines all trace event types emitted by Andromeda. Each event
//! is bound to a [`TraceId`] for forensic correlation and must be validated
//! before emission via [`EventEnvelope::validate`]. No event is ever dropped
//! silently by the [`EventEmitter`]; all rejections are counted and observable.
//!
//! ## Recovery Audit Trail (C6 spec)
//!
//! Recovery startup and WAL replay MUST emit the following sequences:
//!
//! ### Startup Phase
//! - **RecoveryTrace** (`RecoveryStartup` variant): Emitted when recovery begins
//!   - Captures: `trace_id`, `last_durable_lsn`, `corruption_boundary_lsn`
//!   - Carries startup mode (FastStart/SafeStart/ForensicStart) implicitly from manifest
//!   - Proves: Recovery boundary established with durable LSN
//!
//! ### WAL Replay Phase
//! - **WalEventTrace** (per batch or operation): Emitted for each WAL append/flush
//!   - Captures: `trace_id`, `transaction_id`, `appended_lsn`, `durable_lsn`
//!   - Carries operation type: `WalOperation::Append` or `WalOperation::Flush`
//!   - Incomplete transactions observed during replay are traced with `has_lsn_evidence()`
//!   - Proves: Replay progress and LSN sequence integrity
//!
//! ### Manifest Publication Phase
//! - **ManifestTrace** (`Switch` variant): Emitted when new snapshot becomes visible
//!   - Captures: `trace_id`, `catalog_version`, `manifest_epoch`, `required_wal_start_lsn`
//!   - Carries: `accepted=true` when publication succeeds
//!   - Proves: Catalog version transition with WAL anchor evidence
//!
//! **Invariant**: Each phase emits at least one event with the same `trace_id`.
//! Recovery completion is only visible when a `RecoveryTrace` is observable.
//!
//! ## Security Audit Trail (C6 spec)
//!
//! Authorization and admission decisions MUST emit the following:
//!
//! ### Authorization Phase
//! - **SecurityAuditTrace** (allow path): Emitted after permission check succeeds
//!   - Captures: `trace_id`, `surface`, `certificate`, `principal`, `permission`, `outcome`
//!   - Outcome: `SecurityAuditOutcome::Allowed`
//!   - Reason: Describes the authorization path (e.g., "execute_procedure on application surface")
//!   - Proves: mTLS cert → principal binding → permission grant
//!
//! - **SecurityAuditTrace** (deny path): Emitted even when authorization fails
//!   - Captures: Same fields as allowed path
//!   - Outcome: `SecurityAuditOutcome::Denied`
//!   - Reason: Typed denial reason (from `principal_binding` module)
//!   - Proves: Decision was made and observable (not silent)
//!
//! ### Admission Gate Phase
//! - **ContractRejectedTrace**: Emitted when contract validation fails before admission
//!   - Carries rejection code and reason explaining the contract mismatch
//!
//! - **AuthorizationDeniedTrace**: Emitted as final step if gate rejects
//!   - Reason documents why admission was denied (auth, budget, etc.)
//!   - Proves: No transaction created, no WAL entry written
//!
//! **Invariant**: Every authorization and admission decision is observable.
//! Denials carry machine-classifiable reasons for audit pipelines.
//! No sensitive key material appears in audit trails (validated by `contains_sensitive_evidence`).
//!
//! ## Event Emission Guarantees
//!
//! The [`EventEmitter`] enforces:
//! - **Monotonic EventId allocation**: IDs never repeat within a session
//! - **Non-zero EventId**: All emitted events have EventId > 0
//! - **Envelope validation**: All events validated before insertion (no silent drops)
//! - **Rejection counting**: Failed emissions are counted in `rejected_count()`
//! - **Exhaustion guard**: Emitter poisons itself when EventId space exhausted
//!
//! Tests verify all 4 C6 scenarios:
//! 1. `test_recovery_audit_trace_covers_startup_and_replay`
//! 2. `test_security_audit_trail_covers_mtls_and_permission`
//! 3. `test_recovery_incomplete_transaction_rejection_traced`
//! 4. `test_admission_gate_rejection_leaves_no_silent_drop`

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ContractHash, InvocationId, TransactionId,
};
#[cfg(test)]
use andromeda_core::{RequestId, SessionId};

use crate::TraceId;

mod admission_audit;
mod backup_audit;
mod correlation;
mod decision;
mod durable_audit;
mod hadr_audit;
mod protocol_rejection;
mod sequence;
mod sink;
mod transition;

pub use admission_audit::*;
pub use backup_audit::*;
pub use correlation::*;
pub use decision::*;
pub use durable_audit::*;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceScope {
    Application,
    Administration,
    Cluster,
    BackupAgent,
    MonitoringAgent,
}

impl SurfaceScope {
    pub const fn permits_admin_operation(self) -> bool {
        !matches!(self, Self::Application)
    }

    pub const fn permits_permission(self, permission: Permission) -> bool {
        match self {
            Self::Application => !permission.is_admin_operation_permission(),
            Self::BackupAgent => matches!(
                permission.family(),
                PermissionFamily::Recovery | PermissionFamily::Diagnostics
            ),
            Self::MonitoringAgent => matches!(permission.family(), PermissionFamily::Diagnostics),
            Self::Administration | Self::Cluster => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionFamily {
    Application,
    Definition,
    Diagnostics,
    Security,
    Recovery,
    Cluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    ExecuteProcedure,
    ReadContract,
    CreateTable,
    CreateMap,
    CreateProcedure,
    ImportDefinitionBatch,
    DebugProcedure,
    ReadProcedureStore,
    InspectPlans,
    ManageSecurity,
    RotateCertificate,
    RevokeCertificateIdentity,
    Backup,
    Restore,
    ForensicStart,
    ClusterPromote,
    FenceNode,
    UpdateClusterManifest,
}

impl Permission {
    pub const fn family(self) -> PermissionFamily {
        match self {
            Self::ExecuteProcedure | Self::ReadContract => PermissionFamily::Application,
            Self::CreateTable
            | Self::CreateMap
            | Self::CreateProcedure
            | Self::ImportDefinitionBatch => PermissionFamily::Definition,
            Self::DebugProcedure | Self::ReadProcedureStore | Self::InspectPlans => {
                PermissionFamily::Diagnostics
            }
            Self::ManageSecurity | Self::RotateCertificate | Self::RevokeCertificateIdentity => {
                PermissionFamily::Security
            }
            Self::Backup | Self::Restore | Self::ForensicStart => PermissionFamily::Recovery,
            Self::ClusterPromote | Self::FenceNode | Self::UpdateClusterManifest => {
                PermissionFamily::Cluster
            }
        }
    }

    pub const fn is_admin_operation_permission(self) -> bool {
        matches!(
            self,
            Self::DebugProcedure
                | Self::ReadProcedureStore
                | Self::InspectPlans
                | Self::ManageSecurity
                | Self::RotateCertificate
                | Self::RevokeCertificateIdentity
                | Self::Backup
                | Self::Restore
                | Self::ForensicStart
                | Self::ClusterPromote
                | Self::FenceNode
                | Self::UpdateClusterManifest
        )
    }

    pub const fn authorizes_admin_operation(self, operation: AdminOperation) -> bool {
        matches!(
            (self, operation),
            (Self::DebugProcedure, AdminOperation::DebugProcedure)
                | (Self::ReadProcedureStore, AdminOperation::ReadProcedureStore,)
                | (Self::InspectPlans, AdminOperation::InspectPlans)
                | (Self::ManageSecurity, AdminOperation::ManageSecurity)
                | (Self::RotateCertificate, AdminOperation::RotateCertificate)
                | (
                    Self::RevokeCertificateIdentity,
                    AdminOperation::RevokeCertificateIdentity,
                )
                | (Self::Backup, AdminOperation::Backup)
                | (Self::Restore, AdminOperation::Restore)
                | (Self::ForensicStart, AdminOperation::ForensicStart)
                | (Self::ClusterPromote, AdminOperation::ClusterPromote)
                | (Self::FenceNode, AdminOperation::FenceNode)
                | (
                    Self::UpdateClusterManifest,
                    AdminOperation::UpdateClusterManifest,
                )
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdminOperation {
    DebugProcedure,
    ReadProcedureStore,
    InspectPlans,
    ManageSecurity,
    RotateCertificate,
    RevokeCertificateIdentity,
    Backup,
    Restore,
    ForensicStart,
    ClusterPromote,
    FenceNode,
    UpdateClusterManifest,
}

impl AdminOperation {
    pub const fn required_permission(self) -> Permission {
        match self {
            Self::DebugProcedure => Permission::DebugProcedure,
            Self::ReadProcedureStore => Permission::ReadProcedureStore,
            Self::InspectPlans => Permission::InspectPlans,
            Self::ManageSecurity => Permission::ManageSecurity,
            Self::RotateCertificate => Permission::RotateCertificate,
            Self::RevokeCertificateIdentity => Permission::RevokeCertificateIdentity,
            Self::Backup => Permission::Backup,
            Self::Restore => Permission::Restore,
            Self::ForensicStart => Permission::ForensicStart,
            Self::ClusterPromote => Permission::ClusterPromote,
            Self::FenceNode => Permission::FenceNode,
            Self::UpdateClusterManifest => Permission::UpdateClusterManifest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateIdentity {
    pub fingerprint: String,
    pub subject: String,
    pub surface: SurfaceScope,
}

impl CertificateIdentity {
    pub fn new(
        fingerprint: impl Into<String>,
        subject: impl Into<String>,
        surface: SurfaceScope,
    ) -> AndromedaResult<Self> {
        Ok(Self {
            fingerprint: non_empty_evidence("certificate fingerprint", fingerprint)?,
            subject: non_empty_evidence("certificate subject", subject)?,
            surface,
        })
    }

    pub fn has_identity_evidence(&self) -> bool {
        !self.fingerprint.trim().is_empty() && !self.subject.trim().is_empty()
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        contains_sensitive_marker(&self.fingerprint) || contains_sensitive_marker(&self.subject)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserPrincipalKind {
    Human,
    Service,
    BreakGlass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPrincipal {
    pub principal_id: String,
    pub kind: UserPrincipalKind,
}

impl UserPrincipal {
    pub fn new(principal_id: impl Into<String>, kind: UserPrincipalKind) -> AndromedaResult<Self> {
        Ok(Self {
            principal_id: non_empty_evidence("principal id", principal_id)?,
            kind,
        })
    }

    pub fn has_identity_evidence(&self) -> bool {
        !self.principal_id.trim().is_empty()
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        contains_sensitive_marker(&self.principal_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAuditOutcome {
    Allowed,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityAuditTrace {
    pub trace_id: TraceId,
    pub schema_version: EventSchemaVersion,
    pub surface: SurfaceScope,
    pub certificate: CertificateIdentity,
    pub principal: UserPrincipal,
    pub permission: Permission,
    pub outcome: SecurityAuditOutcome,
    pub reason: String,
}

impl SecurityAuditTrace {
    pub fn new(
        trace_id: TraceId,
        surface: SurfaceScope,
        certificate: CertificateIdentity,
        principal: UserPrincipal,
        permission: Permission,
        outcome: SecurityAuditOutcome,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Ok(Self {
            trace_id,
            schema_version: EventSchemaVersion::V0,
            surface,
            certificate,
            principal,
            permission,
            outcome,
            reason: non_empty_reason(reason)?,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_supported_schema_version(&self) -> bool {
        self.schema_version.is_v0()
    }

    pub fn has_identity_evidence(&self) -> bool {
        self.certificate.has_identity_evidence() && self.principal.has_identity_evidence()
    }

    pub const fn surface_matches_certificate(&self) -> bool {
        self.surface as u8 == self.certificate.surface as u8
    }

    pub const fn surface_permits_permission(&self) -> bool {
        self.surface.permits_permission(self.permission)
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        self.certificate.contains_sensitive_evidence()
            || self.principal.contains_sensitive_evidence()
            || contains_sensitive_marker(&self.reason)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminOperationTrace {
    pub trace_id: TraceId,
    pub schema_version: EventSchemaVersion,
    pub surface: SurfaceScope,
    pub certificate: CertificateIdentity,
    pub principal: UserPrincipal,
    pub operation: AdminOperation,
    pub permission: Permission,
    pub accepted: bool,
    pub reason: String,
}

impl AdminOperationTrace {
    #[allow(
        clippy::too_many_arguments,
        reason = "Audit trace construction keeps certificate, principal, permission, and decision fields explicit."
    )]
    pub fn new(
        trace_id: TraceId,
        surface: SurfaceScope,
        certificate: CertificateIdentity,
        principal: UserPrincipal,
        operation: AdminOperation,
        permission: Permission,
        accepted: bool,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Ok(Self {
            trace_id,
            schema_version: EventSchemaVersion::V0,
            surface,
            certificate,
            principal,
            operation,
            permission,
            accepted,
            reason: non_empty_reason(reason)?,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_supported_schema_version(&self) -> bool {
        self.schema_version.is_v0()
    }

    pub const fn surface_permits_operation(&self) -> bool {
        self.surface.permits_admin_operation()
    }

    pub fn has_identity_evidence(&self) -> bool {
        self.certificate.has_identity_evidence() && self.principal.has_identity_evidence()
    }

    pub const fn surface_matches_certificate(&self) -> bool {
        self.surface as u8 == self.certificate.surface as u8
    }

    pub const fn permission_matches_operation(&self) -> bool {
        self.permission.authorizes_admin_operation(self.operation)
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        self.certificate.contains_sensitive_evidence()
            || self.principal.contains_sensitive_evidence()
            || contains_sensitive_marker(&self.reason)
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEnvelope {
    pub event_id: EventId,
    pub trace_id: TraceId,
    pub correlation: EventCorrelation,
    pub event: TraceEvent,
}

impl EventEnvelope {
    pub fn new(
        event_id: EventId,
        correlation: EventCorrelation,
        event: TraceEvent,
    ) -> AndromedaResult<Self> {
        let envelope = Self {
            event_id,
            trace_id: event.trace_id(),
            correlation,
            event,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.event_id.is_zero() {
            return Err(observe_error("observability event_id must be non-zero"));
        }

        if self.trace_id.is_zero() {
            return Err(observe_error("observability trace_id must be non-zero"));
        }

        if self.trace_id != self.event.trace_id() {
            return Err(observe_error(
                "observability envelope trace_id must match payload trace_id",
            ));
        }

        // Transition traces own their own structural validation surface
        // (phase/reason/durable-LSN/sensitive-marker checks). Run it before the
        // envelope-level fall-through so payload defects are surfaced with the
        // same diagnostic the standalone validators emit.
        match &self.event {
            TraceEvent::TransactionTransition(trace) => trace.validate()?,
            TraceEvent::ExecutionTransition(trace) => trace.validate()?,
            _ => {}
        }

        match &self.event {
            TraceEvent::Decision(trace) if !trace.has_explanation() => Err(observe_error(
                "critical decision traces require a non-empty reason",
            )),
            TraceEvent::Wal(trace) if !trace.proves_durable_boundary() => Err(observe_error(
                "legacy WAL traces require non-zero durable LSN evidence",
            )),
            TraceEvent::WalEvent(trace) if !trace.has_lsn_evidence() => Err(observe_error(
                "WAL append/flush traces require explicit LSN evidence",
            )),
            TraceEvent::CommitVisible(trace) if !trace.proves_wal_before_visible_commit() => Err(
                observe_error("commit-visible traces require non-zero durable commit LSN evidence"),
            ),
            TraceEvent::RollbackDurable(trace) if !trace.proves_durable_rollback() => Err(
                observe_error("rollback-durable traces require non-zero rollback LSN evidence"),
            ),
            TraceEvent::RecoveryStartup(trace) if !trace.proves_recovery_boundary() => Err(
                observe_error("recovery startup traces require non-zero durable LSN evidence"),
            ),
            TraceEvent::Manifest(trace) if !trace.has_reason() => Err(observe_error(
                "manifest validation/switch traces require a non-empty reason",
            )),
            TraceEvent::Manifest(trace) if !trace.has_wal_anchor_evidence() => Err(observe_error(
                "manifest validation/switch traces require manifest epoch and WAL anchor evidence",
            )),
            TraceEvent::CatalogMutation(trace) if !trace.has_action() => Err(observe_error(
                "catalog mutation traces require a non-empty action",
            )),
            TraceEvent::FrameRejection(trace) if !trace.has_reason() => Err(observe_error(
                "frame rejection traces require a non-empty reason",
            )),
            TraceEvent::FrameRejection(trace) if !trace.has_frame_evidence() => Err(observe_error(
                "frame rejection traces require stream_id and frame_type evidence",
            )),
            TraceEvent::StreamRoleRejection(trace) if !trace.has_reason() => Err(observe_error(
                "stream role rejection traces require a non-empty reason",
            )),
            TraceEvent::StreamRoleRejection(trace) if !trace.has_role_evidence() => {
                Err(observe_error(
                    "stream role rejection traces require stream_id, observed_role, and expected_role evidence",
                ))
            }
            TraceEvent::Backpressure(trace) if !trace.has_reason() => Err(observe_error(
                "backpressure traces require a non-empty reason",
            )),
            TraceEvent::Backpressure(trace) if !trace.has_pressure_evidence() => Err(
                observe_error("backpressure traces require retry or queue pressure evidence"),
            ),
            TraceEvent::CompletionEmitted(trace) if !trace.has_reason() => Err(observe_error(
                "completion emitted traces require a non-empty reason",
            )),
            TraceEvent::CompletionEmitted(trace) if !trace.has_completion_evidence() => Err(
                observe_error("completion emitted traces require completion code evidence"),
            ),
            TraceEvent::CompletionEmitted(trace) if !trace.proves_committed_completion() => Err(
                observe_error("committed completion traces require non-zero durable LSN evidence"),
            ),
            TraceEvent::ContractRejected(trace) if !trace.has_reason() => Err(observe_error(
                "contract rejected traces require a non-empty reason",
            )),
            TraceEvent::ContractRejected(trace) if !trace.has_contract_evidence() => {
                Err(observe_error(
                    "contract rejected traces require contract kind and rejection code evidence",
                ))
            }
            TraceEvent::AuthorizationDenied(trace) if !trace.has_reason() => Err(observe_error(
                "authorization denial traces require a non-empty reason",
            )),
            TraceEvent::AuthorizationDenied(trace) if !trace.has_permission_evidence() => Err(
                observe_error("authorization denial traces require denied permission evidence"),
            ),
            TraceEvent::SecurityAudit(trace) if !trace.has_supported_schema_version() => Err(
                observe_error("security audit traces require the V0 event schema version"),
            ),
            TraceEvent::SecurityAudit(trace) if !trace.has_identity_evidence() => {
                Err(observe_error(
                    "security audit traces require certificate and principal identity evidence",
                ))
            }
            TraceEvent::SecurityAudit(trace) if !trace.surface_matches_certificate() => Err(
                observe_error("security audit trace surface must match certificate surface scope"),
            ),
            TraceEvent::SecurityAudit(trace) if !trace.surface_permits_permission() => {
                Err(observe_error(
                    "security audit traces require surface scope matching permission family",
                ))
            }
            TraceEvent::SecurityAudit(trace) if !trace.has_reason() => Err(observe_error(
                "security audit traces require a non-empty reason",
            )),
            TraceEvent::AdminOperation(trace) if !trace.has_supported_schema_version() => Err(
                observe_error("admin operation traces require the V0 event schema version"),
            ),
            TraceEvent::AdminOperation(trace) if !trace.surface_permits_operation() => Err(
                observe_error("application surface cannot carry admin operation traces"),
            ),
            TraceEvent::AdminOperation(trace) if !trace.has_identity_evidence() => {
                Err(observe_error(
                    "admin operation traces require certificate and principal identity evidence",
                ))
            }
            TraceEvent::AdminOperation(trace) if !trace.surface_matches_certificate() => Err(
                observe_error("admin operation trace surface must match certificate surface scope"),
            ),
            TraceEvent::AdminOperation(trace) if !trace.permission_matches_operation() => {
                Err(observe_error(
                    "admin operation traces require permission evidence matching the operation",
                ))
            }
            TraceEvent::AdminOperation(trace) if !trace.has_reason() => Err(observe_error(
                "admin operation traces require a non-empty reason",
            )),
            TraceEvent::UnsupportedVersion(trace) if !trace.has_reason() => Err(observe_error(
                "unsupported version traces require a non-empty reason",
            )),
            TraceEvent::UnsupportedVersion(trace) if !trace.has_version_evidence() => {
                Err(observe_error(
                    "unsupported version traces require offered, minimum, and maximum version evidence",
                ))
            }
            TraceEvent::SchemaLayoutDecision(trace) if !trace.has_reason() => Err(observe_error(
                "schema/layout decision traces require a non-empty reason",
            )),
            TraceEvent::SchemaLayoutDecision(trace) if !trace.has_schema_layout_evidence() => {
                Err(observe_error(
                    "schema/layout decision traces require schema and layout numeric evidence",
                ))
            }
            TraceEvent::CorruptionBoundary(trace) if !trace.proves_boundary() => Err(
                observe_error("corruption boundary traces require boundary LSN and reason"),
            ),
            TraceEvent::Audit(trace) if !trace.is_complete() => Err(observe_error(
                "audit traces require actor, object, and action evidence",
            )),
            TraceEvent::IoPlacementDecision(trace) if !trace.has_reason() => Err(observe_error(
                "IO placement decision traces require a non-empty reason",
            )),
            TraceEvent::PlacementAudit(trace) if !trace.has_reason() => Err(observe_error(
                "placement audit events require a non-empty reason",
            )),
            TraceEvent::PlacementAudit(trace) if !trace.has_transition_evidence() => {
                Err(observe_error(
                    "placement audit events require transition-specific segment/extent evidence",
                ))
            }
            TraceEvent::PlacementAudit(trace) if !trace.outcome_matches_transition() => Err(
                observe_error("placement audit event acceptance must match transition semantics"),
            ),
            TraceEvent::IoBudgetDecision(trace) if !trace.has_reason() => Err(observe_error(
                "IO budget decision traces require a non-empty reason",
            )),
            TraceEvent::IoBudgetDecision(trace) if !trace.has_budget_evidence() => Err(
                observe_error("IO budget decision traces require explicit budget limit evidence"),
            ),
            TraceEvent::IoBudgetDecision(trace) if !trace.outcome_matches_budget() => {
                Err(observe_error(
                    "IO budget decision outcome must match requested usage and budget limits",
                ))
            }
            TraceEvent::GpuPolicyDecision(trace) if !trace.has_reason() => Err(observe_error(
                "GPU policy decision traces require a non-empty reason",
            )),
            TraceEvent::GpuPolicyDecision(trace) if !trace.outcome_matches_policy() => {
                Err(observe_error(
                    "GPU policy decision outcome must match availability, policy, and pipeline",
                ))
            }
            _ => {
                self.validate_text_safety()?;
                self.validate_correlation()?;
                Ok(())
            }
        }
    }

    fn validate_correlation(&self) -> AndromedaResult<()> {
        self.validate_correlation_values()?;
        self.validate_protocol_correlation()?;
        self.validate_request_session_correlation()?;
        self.validate_denied_path_correlation()?;
        self.validate_transaction_correlation()?;
        self.validate_catalog_correlation()
    }

    fn validate_correlation_values(&self) -> AndromedaResult<()> {
        if self
            .correlation
            .request_id
            .is_some_and(|request_id| request_id.get() == 0)
        {
            return Err(observe_error(
                "observability request_id correlation must be non-zero when present",
            ));
        }

        if self
            .correlation
            .session_id
            .is_some_and(|session_id| session_id.get() == 0)
        {
            return Err(observe_error(
                "observability session_id correlation must be non-zero when present",
            ));
        }

        if self
            .correlation
            .contract_hash
            .is_some_and(ContractHash::is_zero)
        {
            return Err(observe_error(
                "observability contract_hash correlation must be non-zero when present",
            ));
        }

        if self
            .correlation
            .catalog_version
            .is_some_and(|catalog_version| catalog_version.get() == 0)
        {
            return Err(observe_error(
                "observability catalog_version correlation must be non-zero when present",
            ));
        }

        if self
            .correlation
            .catalog_object_id
            .is_some_and(|catalog_object_id| catalog_object_id.get() == 0)
        {
            return Err(observe_error(
                "observability catalog_object_id correlation must be non-zero when present",
            ));
        }

        if self
            .correlation
            .transaction_id
            .is_some_and(|transaction_id| transaction_id.get() == 0)
        {
            return Err(observe_error(
                "observability transaction_id correlation must be non-zero when present",
            ));
        }

        if self
            .correlation
            .durable_lsn
            .is_some_and(|durable_lsn| durable_lsn == 0)
        {
            return Err(observe_error(
                "observability durable_lsn correlation must be non-zero when present",
            ));
        }

        Ok(())
    }

    fn validate_request_session_correlation(&self) -> AndromedaResult<()> {
        if self.event.requires_request_session_correlation()
            && !self.correlation.has_request_session()
        {
            return Err(observe_error(
                "request-scoped security and protocol events require non-zero request_id and session_id correlation",
            ));
        }

        Ok(())
    }

    fn validate_denied_path_correlation(&self) -> AndromedaResult<()> {
        if self.event.must_not_have_transaction_correlation()
            && !self.correlation.has_no_transaction_evidence()
        {
            return Err(observe_error(
                "denied pre-transaction paths must not include transaction or durable LSN correlation",
            ));
        }

        Ok(())
    }

    fn validate_transaction_correlation(&self) -> AndromedaResult<()> {
        match &self.event {
            TraceEvent::WalEvent(trace) => {
                if let Some(transaction_id) = trace.transaction_id
                    && self.correlation.transaction_id != Some(transaction_id)
                {
                    return Err(observe_error(
                        "WAL event transaction_id correlation must match WAL trace payload",
                    ));
                }

                if trace.operation == WalOperation::Flush
                    && self.correlation.durable_lsn != trace.durable_lsn
                {
                    return Err(observe_error(
                        "WAL flush durable_lsn correlation must match WAL trace payload",
                    ));
                }
            }
            TraceEvent::CommitVisible(trace)
                if self.correlation.transaction_id != Some(trace.transaction_id)
                    || self.correlation.durable_lsn != Some(trace.durable_commit_lsn) =>
            {
                return Err(observe_error(
                    "commit-visible traces require matching transaction_id and durable_lsn correlation",
                ));
            }
            TraceEvent::RollbackDurable(trace)
                if self.correlation.transaction_id != Some(trace.transaction_id)
                    || self.correlation.durable_lsn != Some(trace.durable_rollback_lsn) =>
            {
                return Err(observe_error(
                    "rollback-durable traces require matching transaction_id and durable_lsn correlation",
                ));
            }
            TraceEvent::RecoveryStartup(trace)
                if self.correlation.durable_lsn != Some(trace.last_durable_lsn) =>
            {
                return Err(observe_error(
                    "recovery startup traces require matching durable_lsn correlation",
                ));
            }
            TraceEvent::CompletionEmitted(trace)
                if trace.committed && self.correlation.durable_lsn != trace.durable_lsn =>
            {
                return Err(observe_error(
                    "committed completion traces require matching durable_lsn correlation",
                ));
            }
            TraceEvent::TransactionTransition(trace) => {
                if self.correlation.transaction_id != Some(trace.transaction_id) {
                    return Err(observe_error(
                        "transaction transition traces require matching transaction_id correlation",
                    ));
                }
                if let Some(payload_lsn) = trace.durable_lsn {
                    if self.correlation.durable_lsn != Some(payload_lsn) {
                        return Err(observe_error(
                            "transaction transition traces require matching durable_lsn correlation when payload carries one",
                        ));
                    }
                } else if trace.next_phase.requires_durable_evidence() {
                    // payload validate() already guards this, but keep the
                    // envelope-level diagnostic explicit.
                    return Err(observe_error(
                        "terminal transaction transition traces require durable_lsn correlation",
                    ));
                }
                if let Some(req) = trace.request_id
                    && self.correlation.request_id != Some(req)
                {
                    return Err(observe_error(
                        "transaction transition traces require matching request_id correlation when payload carries one",
                    ));
                }
                if let Some(sess) = trace.session_id
                    && self.correlation.session_id != Some(sess)
                {
                    return Err(observe_error(
                        "transaction transition traces require matching session_id correlation when payload carries one",
                    ));
                }
            }
            TraceEvent::ExecutionTransition(trace) => {
                if let Some(payload_tx) = trace.transaction_id
                    && self.correlation.transaction_id != Some(payload_tx)
                {
                    return Err(observe_error(
                        "execution transition traces require matching transaction_id correlation when payload carries one",
                    ));
                }
                if let Some(payload_lsn) = trace.durable_lsn {
                    if self.correlation.durable_lsn != Some(payload_lsn) {
                        return Err(observe_error(
                            "execution transition traces require matching durable_lsn correlation when payload carries one",
                        ));
                    }
                } else if trace
                    .next_phase
                    .is_some_and(TransactionPhaseCode::requires_durable_evidence)
                {
                    return Err(observe_error(
                        "terminal execution transition traces require durable_lsn correlation",
                    ));
                }
                let denied = matches!(
                    trace.reason_code,
                    TransitionReasonCode::PERMISSION_DENIED
                        | TransitionReasonCode::PRE_TRANSACTION_REJECTION
                );
                if denied && !self.correlation.has_no_transaction_evidence() {
                    return Err(observe_error(
                        "execution transition traces with a pre-transaction rejection reason must not carry transaction or durable_lsn correlation",
                    ));
                }
                if let Some(req) = trace.request_id
                    && self.correlation.request_id != Some(req)
                {
                    return Err(observe_error(
                        "execution transition traces require matching request_id correlation when payload carries one",
                    ));
                }
                if let Some(sess) = trace.session_id
                    && self.correlation.session_id != Some(sess)
                {
                    return Err(observe_error(
                        "execution transition traces require matching session_id correlation when payload carries one",
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn validate_catalog_correlation(&self) -> AndromedaResult<()> {
        if let TraceEvent::Manifest(trace) = &self.event
            && self.correlation.catalog_version != Some(trace.catalog_version)
        {
            return Err(observe_error(
                "manifest traces require matching catalog_version correlation",
            ));
        }

        Ok(())
    }

    fn validate_text_safety(&self) -> AndromedaResult<()> {
        let text_has_sensitive_marker = match &self.event {
            TraceEvent::Decision(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::CatalogMutation(trace) => contains_sensitive_marker(&trace.action),
            TraceEvent::FrameRejection(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::StreamRoleRejection(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::Backpressure(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::CompletionEmitted(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::ContractRejected(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::AuthorizationDenied(trace) => {
                contains_sensitive_marker(&trace.reason)
                    || contains_sensitive_marker(&trace.denied_permission)
            }
            TraceEvent::SecurityAudit(trace) => trace.contains_sensitive_evidence(),
            TraceEvent::AdminOperation(trace) => trace.contains_sensitive_evidence(),
            TraceEvent::UnsupportedVersion(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::SchemaLayoutDecision(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::CorruptionBoundary(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::Manifest(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::IoPlacementDecision(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::PlacementAudit(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::IoBudgetDecision(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::GpuPolicyDecision(trace) => contains_sensitive_marker(&trace.reason),
            TraceEvent::Audit(trace) => {
                contains_sensitive_marker(&trace.actor)
                    || contains_sensitive_marker(&trace.object)
                    || contains_sensitive_marker(&trace.action)
            }
            TraceEvent::Invocation(_)
            | TraceEvent::Wal(_)
            | TraceEvent::WalEvent(_)
            | TraceEvent::CommitVisible(_)
            | TraceEvent::RollbackDurable(_)
            | TraceEvent::RecoveryStartup(_)
            | TraceEvent::Mvcc(_)
            | TraceEvent::Resource(_)
            | TraceEvent::TransactionTransition(_)
            | TraceEvent::ExecutionTransition(_) => false,
        };

        if text_has_sensitive_marker {
            return Err(observe_error(
                "observability text fields must not include secrets, private key material, tokens, passwords, or payload bodies",
            ));
        }

        Ok(())
    }

    fn validate_protocol_correlation(&self) -> AndromedaResult<()> {
        match self.event.protocol_scope() {
            Some(ProtocolEventScope::Request) => {
                let has_request_id = self
                    .correlation
                    .request_id
                    .is_some_and(|request_id| request_id.get() != 0);
                let has_session_id = self
                    .correlation
                    .session_id
                    .is_some_and(|session_id| session_id.get() != 0);

                if !has_request_id || !has_session_id {
                    return Err(observe_error(
                        "request-scoped protocol events require non-zero request_id and session_id correlation",
                    ));
                }

                Ok(())
            }
            Some(ProtocolEventScope::Session) => {
                let has_session_id = self
                    .correlation
                    .session_id
                    .is_some_and(|session_id| session_id.get() != 0);

                if !has_session_id {
                    return Err(observe_error(
                        "session-scoped protocol events require non-zero session_id correlation",
                    ));
                }

                Ok(())
            }
            Some(ProtocolEventScope::Connection) | None => Ok(()),
        }
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

fn non_empty_evidence(label: &str, value: impl Into<String>) -> AndromedaResult<String> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(observe_error(format!(
            "observability {label} evidence requires a non-empty value",
        )));
    }

    Ok(value)
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
mod tests {
    use super::*;

    #[test]
    fn critical_decisions_need_explanations() {
        let trace = DecisionTrace {
            trace_id: TraceId::new(1),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "ContractHash matched manifest".to_string(),
        };

        assert!(trace.has_explanation());
    }

    #[test]
    fn wal_trace_proves_nonzero_durable_boundary() {
        let trace = WalTrace {
            trace_id: TraceId::new(1),
            transaction_id: Some(TransactionId::new(7)),
            durable_lsn: 42,
        };

        assert!(trace.proves_durable_boundary());
    }

    #[test]
    fn event_envelope_validates_non_zero_ids_and_payload_shape() {
        let envelope = EventEnvelope::new(
            EventId::new(10),
            EventCorrelation {
                request_id: Some(RequestId::new(11)),
                session_id: Some(SessionId::new(12)),
                contract_hash: Some(ContractHash::test_vector(7)),
                catalog_version: Some(CatalogVersion::new(13)),
                catalog_object_id: Some(CatalogObjectId::new(14)),
                transaction_id: None,
                durable_lsn: None,
                protocol: Some(ProtocolCorrelation {
                    protocol_version: Some(1),
                    stream_id: Some(15),
                    stream_role: Some(1),
                    frame_type: Some(1),
                    payload_kind: Some(2),
                    sequence: Some(16),
                }),
            },
            TraceEvent::Decision(DecisionTrace {
                trace_id: TraceId::new(9),
                decision: CriticalDecisionKind::ContractValidation,
                reason: "contract hash and catalog version matched request".to_string(),
            }),
        )
        .expect("valid correlated decision envelope");

        assert_eq!(envelope.event_id.get(), 10);
        assert_eq!(envelope.trace_id, TraceId::new(9));

        let err = EventEnvelope::new(
            EventId::new(0),
            EventCorrelation::empty(),
            TraceEvent::Decision(DecisionTrace {
                trace_id: TraceId::new(9),
                decision: CriticalDecisionKind::ContractValidation,
                reason: "valid reason".to_string(),
            }),
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Internal);

        let forged = EventEnvelope {
            event_id: EventId::new(1),
            trace_id: TraceId::new(100),
            correlation: EventCorrelation::empty(),
            event: TraceEvent::Decision(DecisionTrace {
                trace_id: TraceId::new(101),
                decision: CriticalDecisionKind::ContractValidation,
                reason: "payload trace diverges from envelope".to_string(),
            }),
        };
        let err = forged.validate().unwrap_err();
        assert!(err.message().contains("must match payload trace_id"));
    }

    #[test]
    fn event_envelope_rejects_empty_decision_reasons() {
        let err = EventEnvelope::new(
            EventId::new(1),
            EventCorrelation::empty(),
            TraceEvent::Decision(DecisionTrace {
                trace_id: TraceId::new(2),
                decision: CriticalDecisionKind::AuthorizationDenial,
                reason: "   ".to_string(),
            }),
        )
        .unwrap_err();

        assert!(err.message().contains("non-empty reason"));
    }

    #[test]
    fn commit_and_recovery_events_require_durable_lsn_evidence() {
        let commit_without_lsn = EventEnvelope::new(
            EventId::new(1),
            EventCorrelation::empty(),
            TraceEvent::CommitVisible(CommitVisibleTrace {
                trace_id: TraceId::new(2),
                transaction_id: TransactionId::new(3),
                durable_commit_lsn: 0,
            }),
        )
        .unwrap_err();
        assert!(commit_without_lsn.message().contains("durable commit LSN"));

        let recovery_without_lsn = EventEnvelope::new(
            EventId::new(4),
            EventCorrelation::empty(),
            TraceEvent::RecoveryStartup(RecoveryTrace {
                trace_id: TraceId::new(5),
                last_durable_lsn: 0,
                corruption_boundary_lsn: None,
            }),
        )
        .unwrap_err();
        assert!(
            recovery_without_lsn
                .message()
                .contains("recovery startup traces")
        );

        assert!(
            EventEnvelope::new(
                EventId::new(6),
                EventCorrelation {
                    transaction_id: Some(TransactionId::new(8)),
                    durable_lsn: Some(9),
                    ..EventCorrelation::empty()
                },
                TraceEvent::CommitVisible(CommitVisibleTrace {
                    trace_id: TraceId::new(7),
                    transaction_id: TransactionId::new(8),
                    durable_commit_lsn: 9,
                }),
            )
            .is_ok()
        );

        assert!(
            EventEnvelope::new(
                EventId::new(10),
                EventCorrelation {
                    durable_lsn: Some(12),
                    ..EventCorrelation::empty()
                },
                TraceEvent::RecoveryStartup(RecoveryTrace {
                    trace_id: TraceId::new(11),
                    last_durable_lsn: 12,
                    corruption_boundary_lsn: Some(13),
                }),
            )
            .is_ok()
        );
    }

    #[test]
    fn durable_events_require_queryable_transaction_and_lsn_correlation() {
        let append_without_transaction_correlation = EventEnvelope::new(
            EventId::new(20),
            EventCorrelation::empty(),
            TraceEvent::WalEvent(WalEventTrace {
                trace_id: TraceId::new(21),
                transaction_id: Some(TransactionId::new(22)),
                operation: WalOperation::Append,
                appended_lsn: 23,
                durable_lsn: None,
            }),
        )
        .unwrap_err();
        assert!(
            append_without_transaction_correlation
                .message()
                .contains("transaction_id correlation")
        );

        let flush = EventEnvelope::new(
            EventId::new(24),
            EventCorrelation {
                transaction_id: Some(TransactionId::new(22)),
                durable_lsn: Some(25),
                ..EventCorrelation::empty()
            },
            TraceEvent::WalEvent(WalEventTrace {
                trace_id: TraceId::new(26),
                transaction_id: Some(TransactionId::new(22)),
                operation: WalOperation::Flush,
                appended_lsn: 25,
                durable_lsn: Some(25),
            }),
        )
        .expect("WAL flush has matching transaction and durable LSN evidence");
        assert!(flush.correlation.has_transaction_evidence());
        assert!(flush.correlation.has_durable_lsn());

        let commit_visible = EventEnvelope::new(
            EventId::new(27),
            EventCorrelation {
                transaction_id: Some(TransactionId::new(22)),
                durable_lsn: Some(25),
                ..EventCorrelation::empty()
            },
            TraceEvent::CommitVisible(CommitVisibleTrace {
                trace_id: TraceId::new(28),
                transaction_id: TransactionId::new(22),
                durable_commit_lsn: 25,
            }),
        )
        .expect("visible commit points at the durable commit LSN");
        assert_eq!(commit_visible.correlation.durable_lsn, Some(25));
    }

    #[test]
    fn manifest_validation_evidence_is_catalog_correlated_and_secret_safe() {
        let manifest = EventEnvelope::new(
            EventId::new(30),
            EventCorrelation {
                catalog_version: Some(CatalogVersion::new(31)),
                ..EventCorrelation::empty()
            },
            TraceEvent::Manifest(ManifestTrace {
                trace_id: TraceId::new(32),
                event: ManifestEventKind::Validation,
                catalog_version: CatalogVersion::new(31),
                manifest_epoch: 33,
                base_checkpoint_lsn: 34,
                required_wal_start_lsn: 35,
                accepted: true,
                reason: "manifest identity and WAL recovery floor accepted".to_string(),
            }),
        )
        .expect("manifest validation evidence has catalog and WAL anchors");
        assert_eq!(
            manifest.correlation.catalog_version,
            Some(CatalogVersion::new(31))
        );

        let leak = EventEnvelope::new(
            EventId::new(36),
            EventCorrelation {
                catalog_version: Some(CatalogVersion::new(31)),
                ..EventCorrelation::empty()
            },
            TraceEvent::Manifest(ManifestTrace {
                trace_id: TraceId::new(37),
                event: ManifestEventKind::Validation,
                catalog_version: CatalogVersion::new(31),
                manifest_epoch: 33,
                base_checkpoint_lsn: 34,
                required_wal_start_lsn: 35,
                accepted: false,
                reason: "payload: raw manifest body".to_string(),
            }),
        )
        .unwrap_err();
        assert!(leak.message().contains("payload bodies"));
    }

    #[test]
    fn event_envelope_rejects_success_shaped_zero_defaults() {
        let err = EventEnvelope::new(
            EventId::new(1),
            EventCorrelation::empty(),
            TraceEvent::FrameRejection(FrameRejectionTrace {
                trace_id: TraceId::new(0),
                scope: ProtocolEventScope::Connection,
                protocol: ProtocolCorrelation::empty(),
                reason: "reserved frame flag set".to_string(),
            }),
        )
        .unwrap_err();

        assert!(err.message().contains("trace_id must be non-zero"));
    }

    #[test]
    fn event_sink_returns_emit_failures_explicitly() {
        struct FailingSink;

        impl EventSink for FailingSink {
            fn emit(&mut self, _event: EventEnvelope) -> AndromedaResult<()> {
                Err(observe_error("sink unavailable"))
            }
        }

        let mut sink = FailingSink;
        let envelope = EventEnvelope::new(
            EventId::new(1),
            EventCorrelation::empty(),
            TraceEvent::Backpressure(BackpressureTrace {
                trace_id: TraceId::new(2),
                scope: ProtocolEventScope::Connection,
                protocol: ProtocolCorrelation::empty(),
                retry_after_micros: Some(100),
                pending_units: None,
                limit_units: None,
                reason: "bounded queue is full".to_string(),
            }),
        )
        .expect("valid backpressure event");

        let err = sink.emit(envelope).unwrap_err();
        assert_eq!(err.message(), "sink unavailable");
    }

    // ------------------------------------------------------------------
    // ProtocolRejectionTrace shape tests (Wave 5 protocol observability)
    // ------------------------------------------------------------------

    #[test]
    fn protocol_rejection_crc_constructor_carries_pre_auth_evidence() {
        let trace = ProtocolRejectionTrace::header_crc_mismatch(
            TraceId::new(1),
            Some(7),
            Some(ProtocolSurfacePlane::Application),
            true,
            "frame header CRC mismatch on Hello",
        );
        assert_eq!(trace.reason, ProtocolRejectionReason::HeaderCrcMismatch);
        assert!(trace.pre_auth);
        assert_eq!(trace.scope, ProtocolEventScope::Connection);
        assert!(trace.has_minimum_evidence());
        assert!(trace.reason.is_safety_critical());
    }

    #[test]
    fn protocol_rejection_unsupported_version_requires_version_evidence() {
        let trace = ProtocolRejectionTrace::unsupported_version(
            TraceId::new(2),
            Some(11),
            42,
            true,
            "unsupported QUIC frame codec version",
        );
        assert_eq!(trace.protocol_version, Some(42));
        assert_eq!(trace.reason, ProtocolRejectionReason::UnsupportedVersion);
        assert!(trace.has_minimum_evidence());
        let projected = trace.as_decision_trace();
        assert_eq!(projected.decision, CriticalDecisionKind::UnsupportedVersion);
    }

    #[test]
    fn protocol_rejection_surface_mismatch_records_both_planes() {
        let trace = ProtocolRejectionTrace::surface_plane_mismatch(
            TraceId::new(3),
            Some(99),
            Some(123),
            ProtocolSurfacePlane::Application,
            ProtocolSurfacePlane::Administration,
            Some(5),
            false,
        );
        assert_eq!(trace.reason, ProtocolRejectionReason::SurfacePlaneMismatch);
        assert!(trace.detail.contains("plane code 1"));
        assert!(trace.detail.contains("plane code 2"));
        assert!(trace.has_minimum_evidence());
    }

    #[test]
    fn protocol_rejection_family_blocked_records_frame_type() {
        let trace = ProtocolRejectionTrace::frame_family_blocked(
            TraceId::new(4),
            Some(1),
            Some(2),
            ProtocolSurfacePlane::Monitoring,
            5,
            false,
            "RpcExecuteRequest not permitted on Monitoring plane",
        );
        assert_eq!(
            trace.reason,
            ProtocolRejectionReason::FrameFamilyNotPermitted
        );
        assert_eq!(trace.frame_type_code, Some(5));
        assert!(trace.has_minimum_evidence());
    }

    #[test]
    fn protocol_rejection_sequence_violation_carries_position() {
        let trace = ProtocolRejectionTrace::result_stream_sequence_violation(
            TraceId::new(5),
            Some(101),
            Some(202),
            7,
            Some(3),
            "RpcBatch frame received before RpcMetadata",
        );
        assert_eq!(
            trace.reason,
            ProtocolRejectionReason::ResultStreamSequenceViolation
        );
        assert_eq!(trace.sequence_position, Some(3));
        assert_eq!(trace.scope, ProtocolEventScope::Request);
        assert!(trace.has_minimum_evidence());
    }

    #[test]
    fn protocol_rejection_oversized_payload_records_length_and_scope() {
        let pre = ProtocolRejectionTrace::oversized_payload(
            TraceId::new(6),
            None,
            None,
            None,
            16 * 1024 * 1024 + 1,
            true,
            "frame payload length exceeds maximum during Hello",
        );
        assert_eq!(pre.scope, ProtocolEventScope::Connection);
        assert_eq!(pre.payload_length, Some(16 * 1024 * 1024 + 1));
        assert!(pre.has_minimum_evidence());

        let post = ProtocolRejectionTrace::oversized_payload(
            TraceId::new(7),
            Some(1),
            Some(2),
            Some(5),
            16 * 1024 * 1024 + 1,
            false,
            "frame payload length exceeds maximum",
        );
        assert_eq!(post.scope, ProtocolEventScope::Request);
        assert!(post.has_minimum_evidence());
    }

    #[test]
    fn protocol_rejection_pre_auth_command_marks_pre_auth_true() {
        let trace = ProtocolRejectionTrace::pre_auth_command(
            TraceId::new(8),
            Some(1),
            Some(2),
            5,
            Some(ProtocolSurfacePlane::Application),
            "RPC dispatch attempted before session handshake completed",
        );
        assert!(trace.pre_auth);
        assert_eq!(
            trace.reason,
            ProtocolRejectionReason::PreAuthCommandRejected
        );
        assert!(trace.has_minimum_evidence());
        assert!(trace.reason.is_safety_critical());
    }

    #[test]
    fn protocol_rejection_minimum_evidence_rejects_zero_trace_or_empty_detail() {
        let zero_trace = ProtocolRejectionTrace::header_crc_mismatch(
            TraceId::new(0),
            Some(1),
            None,
            true,
            "crc mismatch",
        );
        assert!(!zero_trace.has_minimum_evidence());

        let empty_detail = ProtocolRejectionTrace::header_crc_mismatch(
            TraceId::new(1),
            Some(1),
            None,
            true,
            "   ",
        );
        assert!(!empty_detail.has_minimum_evidence());
    }

    // --- transition trace envelope integration ------------------------------

    fn transition_correlation(
        request_id: u64,
        session_id: u64,
        transaction_id: Option<u64>,
        durable_lsn: Option<u64>,
    ) -> EventCorrelation {
        EventCorrelation {
            request_id: Some(RequestId::new(request_id)),
            session_id: Some(SessionId::new(session_id)),
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
            transaction_id: transaction_id.map(TransactionId::new),
            durable_lsn,
            protocol: None,
        }
    }

    #[test]
    fn envelope_accepts_terminal_transaction_transition_with_durable_lsn() {
        let trace = TransactionTransitionTrace {
            trace_id: TraceId::new(70),
            transaction_id: TransactionId::new(901),
            invocation_id: Some(InvocationId::new(11)),
            request_id: Some(RequestId::new(3)),
            session_id: Some(SessionId::new(4)),
            prev_phase: TransactionPhaseCode::COMMITTING,
            next_phase: TransactionPhaseCode::COMMITTED,
            durable_lsn: Some(4242),
            reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
            reason: "WAL flush proved durable commit boundary".to_string(),
        };

        let envelope = EventEnvelope::new(
            EventId::new(1),
            transition_correlation(3, 4, Some(901), Some(4242)),
            TraceEvent::TransactionTransition(trace),
        )
        .expect("terminal transaction transition envelope must validate");

        assert_eq!(
            envelope.event.kind(),
            CriticalDecisionKind::TransactionTransition
        );
        assert_eq!(envelope.event.trace_id(), TraceId::new(70));
    }

    #[test]
    fn envelope_rejects_terminal_transaction_transition_without_durable_lsn() {
        let trace = TransactionTransitionTrace {
            trace_id: TraceId::new(71),
            transaction_id: TransactionId::new(902),
            invocation_id: None,
            request_id: None,
            session_id: None,
            prev_phase: TransactionPhaseCode::COMMITTING,
            next_phase: TransactionPhaseCode::COMMITTED,
            durable_lsn: None,
            reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
            reason: "claims terminal commit without durable LSN".to_string(),
        };

        let err = EventEnvelope::new(
            EventId::new(2),
            transition_correlation(0, 0, Some(902), None),
            TraceEvent::TransactionTransition(trace),
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Internal);
    }

    #[test]
    fn envelope_rejects_transaction_transition_correlation_mismatch() {
        let trace = TransactionTransitionTrace {
            trace_id: TraceId::new(72),
            transaction_id: TransactionId::new(903),
            invocation_id: None,
            request_id: Some(RequestId::new(5)),
            session_id: Some(SessionId::new(6)),
            prev_phase: TransactionPhaseCode::ACTIVE,
            next_phase: TransactionPhaseCode::COMMITTING,
            durable_lsn: None,
            reason_code: TransitionReasonCode::NORMAL_PROGRESS,
            reason: "caller requested commit".to_string(),
        };

        // Envelope correlation transaction_id deliberately does not match
        // payload transaction_id.
        let err = EventEnvelope::new(
            EventId::new(3),
            transition_correlation(5, 6, Some(999), None),
            TraceEvent::TransactionTransition(trace),
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Internal);
    }

    #[test]
    fn envelope_accepts_execution_transition_committed_with_durable_lsn() {
        let trace = ExecutionTransitionTrace {
            trace_id: TraceId::new(80),
            invocation_id: InvocationId::new(31),
            request_id: Some(RequestId::new(9)),
            session_id: Some(SessionId::new(10)),
            transaction_id: Some(TransactionId::new(555)),
            completion_code: Some(1),
            prev_phase: Some(TransactionPhaseCode::COMMITTING),
            next_phase: Some(TransactionPhaseCode::COMMITTED),
            durable_lsn: Some(7777),
            reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
            reason: "execution observed durable commit".to_string(),
        };

        let envelope = EventEnvelope::new(
            EventId::new(4),
            transition_correlation(9, 10, Some(555), Some(7777)),
            TraceEvent::ExecutionTransition(trace),
        )
        .expect("committed execution transition envelope must validate");

        assert_eq!(
            envelope.event.kind(),
            CriticalDecisionKind::ExecutionTransition
        );
    }

    #[test]
    fn envelope_rejects_execution_transition_terminal_without_durable_lsn() {
        let trace = ExecutionTransitionTrace {
            trace_id: TraceId::new(81),
            invocation_id: InvocationId::new(32),
            request_id: Some(RequestId::new(9)),
            session_id: Some(SessionId::new(10)),
            transaction_id: Some(TransactionId::new(556)),
            completion_code: Some(2),
            prev_phase: Some(TransactionPhaseCode::ROLLING_BACK),
            next_phase: Some(TransactionPhaseCode::ROLLED_BACK),
            durable_lsn: None,
            reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
            reason: "claims rollback durable without LSN".to_string(),
        };

        let err = EventEnvelope::new(
            EventId::new(5),
            transition_correlation(9, 10, Some(556), None),
            TraceEvent::ExecutionTransition(trace),
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Internal);
    }

    #[test]
    fn envelope_rejects_pre_transaction_rejection_carrying_transaction_or_lsn() {
        // Payload-level forging is already blocked by the trace's own
        // validate(); this test exercises the envelope-level guard against
        // smuggling transaction/durable_lsn evidence through the correlation
        // header on a pre-transaction rejection path.
        let trace = ExecutionTransitionTrace {
            trace_id: TraceId::new(82),
            invocation_id: InvocationId::new(33),
            request_id: Some(RequestId::new(9)),
            session_id: Some(SessionId::new(10)),
            transaction_id: None,
            completion_code: Some(8),
            prev_phase: None,
            next_phase: None,
            durable_lsn: None,
            reason_code: TransitionReasonCode::PRE_TRANSACTION_REJECTION,
            reason: "contract validation failed before any transaction".to_string(),
        };

        let envelope_with_tx = EventEnvelope::new(
            EventId::new(6),
            transition_correlation(9, 10, Some(123), None),
            TraceEvent::ExecutionTransition(trace.clone()),
        );
        assert!(envelope_with_tx.is_err());

        let envelope_with_lsn = EventEnvelope::new(
            EventId::new(7),
            transition_correlation(9, 10, None, Some(42)),
            TraceEvent::ExecutionTransition(trace.clone()),
        );
        assert!(envelope_with_lsn.is_err());

        let envelope_clean = EventEnvelope::new(
            EventId::new(8),
            transition_correlation(9, 10, None, None),
            TraceEvent::ExecutionTransition(trace),
        )
        .expect("clean pre-transaction rejection envelope must validate");
        assert_eq!(
            envelope_clean.event.kind(),
            CriticalDecisionKind::ExecutionTransition
        );
    }

    #[test]
    fn in_memory_event_sink_records_and_queries_transition_events() {
        let mut sink = InMemoryEventSink::new();

        let tx_trace = TransactionTransitionTrace {
            trace_id: TraceId::new(90),
            transaction_id: TransactionId::new(700),
            invocation_id: None,
            request_id: Some(RequestId::new(2)),
            session_id: Some(SessionId::new(3)),
            prev_phase: TransactionPhaseCode::ACTIVE,
            next_phase: TransactionPhaseCode::COMMITTING,
            durable_lsn: None,
            reason_code: TransitionReasonCode::NORMAL_PROGRESS,
            reason: "caller requested commit".to_string(),
        };
        let tx_envelope = EventEnvelope::new(
            EventId::new(1),
            transition_correlation(2, 3, Some(700), None),
            TraceEvent::TransactionTransition(tx_trace),
        )
        .expect("tx transition envelope valid");

        let exec_trace = ExecutionTransitionTrace {
            trace_id: TraceId::new(91),
            invocation_id: InvocationId::new(40),
            request_id: Some(RequestId::new(2)),
            session_id: Some(SessionId::new(3)),
            transaction_id: Some(TransactionId::new(700)),
            completion_code: Some(1),
            prev_phase: Some(TransactionPhaseCode::COMMITTING),
            next_phase: Some(TransactionPhaseCode::COMMITTED),
            durable_lsn: Some(8181),
            reason_code: TransitionReasonCode::DURABLE_WAL_FLUSH,
            reason: "exec observed durable commit".to_string(),
        };
        let exec_envelope = EventEnvelope::new(
            EventId::new(2),
            transition_correlation(2, 3, Some(700), Some(8181)),
            TraceEvent::ExecutionTransition(exec_trace),
        )
        .expect("exec transition envelope valid");

        sink.emit(tx_envelope).expect("sink accepts tx transition");
        sink.emit(exec_envelope)
            .expect("sink accepts exec transition");

        assert_eq!(sink.transaction_transition_events().len(), 1);
        assert_eq!(sink.execution_transition_events().len(), 1);
        assert_eq!(
            sink.events_for_transaction(TransactionId::new(700)).len(),
            2
        );
    }
}
