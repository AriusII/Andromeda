use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ContractHash, GpuExecutionPolicy, GpuProfile, InvocationId, PipelineClass, RequestId,
    ResourceBudget, SessionId, TransactionId,
};

use crate::TraceId;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriticalDecisionKind {
    ContractValidation,
    AuthorizationDenial,
    PlanSelection,
    WalAppend,
    TransactionCommit,
    WalFlush,
    CommitVisible,
    RollbackDurable,
    MvccVisibility,
    RecoveryStartup,
    ManifestValidation,
    ManifestSwitch,
    CatalogMutation,
    FrameRejection,
    StreamRoleRejection,
    Backpressure,
    CompletionEmitted,
    ContractRejected,
    UnsupportedVersion,
    SchemaLayoutDecision,
    CorruptionBoundary,
    SecurityAuthorization,
    SecurityAudit,
    AdminOperation,
    ResourceGovernance,
    BusinessRuleDecision,
    IoPlacementDecision,
    IoBudgetValidation,
    GpuPolicyDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionTrace {
    pub trace_id: TraceId,
    pub decision: CriticalDecisionKind,
    pub reason: String,
}

impl DecisionTrace {
    pub fn has_explanation(&self) -> bool {
        !self.reason.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolCorrelation {
    pub protocol_version: Option<u16>,
    pub stream_id: Option<u64>,
    pub stream_role: Option<u16>,
    pub frame_type: Option<u16>,
    pub payload_kind: Option<u16>,
    pub sequence: Option<u64>,
}

impl ProtocolCorrelation {
    pub const fn empty() -> Self {
        Self {
            protocol_version: None,
            stream_id: None,
            stream_role: None,
            frame_type: None,
            payload_kind: None,
            sequence: None,
        }
    }

    pub const fn has_frame_evidence(self) -> bool {
        self.stream_id.is_some() && self.frame_type.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolEventScope {
    Connection,
    Session,
    Request,
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
pub struct EventCorrelation {
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
    pub contract_hash: Option<ContractHash>,
    pub catalog_version: Option<CatalogVersion>,
    pub catalog_object_id: Option<CatalogObjectId>,
    pub transaction_id: Option<TransactionId>,
    pub durable_lsn: Option<u64>,
    pub protocol: Option<ProtocolCorrelation>,
}

impl EventCorrelation {
    pub const fn empty() -> Self {
        Self {
            request_id: None,
            session_id: None,
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
            transaction_id: None,
            durable_lsn: None,
            protocol: None,
        }
    }

    pub fn has_request_session(self) -> bool {
        self.request_id
            .is_some_and(|request_id| request_id.get() != 0)
            && self
                .session_id
                .is_some_and(|session_id| session_id.get() != 0)
    }

    pub fn has_contract_catalog(self) -> bool {
        self.contract_hash
            .is_some_and(|contract_hash| !contract_hash.is_zero())
            && self
                .catalog_version
                .is_some_and(|catalog_version| catalog_version.get() != 0)
    }

    pub fn has_no_transaction_evidence(self) -> bool {
        self.transaction_id.is_none() && self.durable_lsn.is_none()
    }

    pub fn has_transaction_evidence(self) -> bool {
        self.transaction_id
            .is_some_and(|transaction_id| transaction_id.get() != 0)
    }

    pub fn has_durable_lsn(self) -> bool {
        self.durable_lsn.is_some_and(|durable_lsn| durable_lsn != 0)
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
pub struct SchemaLayoutDecisionTrace {
    pub trace_id: TraceId,
    pub scope: ProtocolEventScope,
    pub schema_id: Option<u64>,
    pub schema_version: Option<u64>,
    pub layout_id: Option<u64>,
    pub layout_version: Option<u64>,
    pub accepted: bool,
    pub reason: String,
}

impl SchemaLayoutDecisionTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_schema_layout_evidence(&self) -> bool {
        self.schema_id.is_some()
            && self.schema_version.is_some()
            && self.layout_id.is_some()
            && self.layout_version.is_some()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoStorageTier {
    Ram,
    Hot,
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoPipelineStage {
    Ram,
    Hot,
    Cold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoPlacementDecisionTrace {
    pub trace_id: TraceId,
    pub pipeline: PipelineClass,
    pub stage: IoPipelineStage,
    pub selected_tier: IoStorageTier,
    pub accepted: bool,
    pub reason: String,
}

impl IoPlacementDecisionTrace {
    pub fn accepted(
        trace_id: TraceId,
        pipeline: PipelineClass,
        stage: IoPipelineStage,
        selected_tier: IoStorageTier,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Self::new(trace_id, pipeline, stage, selected_tier, true, reason)
    }

    pub fn rejected(
        trace_id: TraceId,
        pipeline: PipelineClass,
        stage: IoPipelineStage,
        selected_tier: IoStorageTier,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Self::new(trace_id, pipeline, stage, selected_tier, false, reason)
    }

    pub fn new(
        trace_id: TraceId,
        pipeline: PipelineClass,
        stage: IoPipelineStage,
        selected_tier: IoStorageTier,
        accepted: bool,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let reason = non_empty_reason(reason)?;
        Ok(Self {
            trace_id,
            pipeline,
            stage,
            selected_tier,
            accepted,
            reason,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoBudgetDecisionTrace {
    pub trace_id: TraceId,
    pub pipeline: PipelineClass,
    pub stage: IoPipelineStage,
    pub budget: ResourceBudget,
    pub requested_memory_bytes: u64,
    pub requested_temp_bytes: u64,
    pub requested_streams: u32,
    pub accepted: bool,
    pub reason: String,
}

impl IoBudgetDecisionTrace {
    pub fn from_budget_request(
        trace_id: TraceId,
        pipeline: PipelineClass,
        stage: IoPipelineStage,
        budget: ResourceBudget,
        requested_memory_bytes: u64,
        requested_temp_bytes: u64,
        requested_streams: u32,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let reason = non_empty_reason(reason)?;
        let accepted = requested_memory_bytes <= budget.max_memory_bytes
            && requested_temp_bytes <= budget.max_temp_bytes
            && requested_streams <= budget.max_streams;

        Ok(Self {
            trace_id,
            pipeline,
            stage,
            budget,
            requested_memory_bytes,
            requested_temp_bytes,
            requested_streams,
            accepted,
            reason,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_budget_evidence(&self) -> bool {
        self.budget.max_memory_bytes != 0
            || self.budget.max_temp_bytes != 0
            || self.budget.max_streams != 0
    }

    pub const fn requested_within_budget(&self) -> bool {
        self.requested_memory_bytes <= self.budget.max_memory_bytes
            && self.requested_temp_bytes <= self.budget.max_temp_bytes
            && self.requested_streams <= self.budget.max_streams
    }

    pub const fn outcome_matches_budget(&self) -> bool {
        self.accepted == self.requested_within_budget()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuPolicyDecisionTrace {
    pub trace_id: TraceId,
    pub pipeline: PipelineClass,
    pub policy: GpuExecutionPolicy,
    pub gpu_declared_available: bool,
    pub accepted: bool,
    pub reason: String,
}

impl GpuPolicyDecisionTrace {
    pub fn from_policy(
        trace_id: TraceId,
        pipeline: PipelineClass,
        policy: GpuExecutionPolicy,
        gpu_declared_available: bool,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let reason = non_empty_reason(reason)?;
        Ok(Self {
            trace_id,
            pipeline,
            policy,
            gpu_declared_available,
            accepted: gpu_declared_available && policy.permits_pipeline(pipeline),
            reason,
        })
    }

    pub fn from_profile(
        trace_id: TraceId,
        pipeline: PipelineClass,
        profile: GpuProfile,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Self::from_policy(
            trace_id,
            pipeline,
            profile.execution_policy,
            profile.available,
            reason,
        )
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn permitted_by_policy(&self) -> bool {
        self.gpu_declared_available && self.policy.permits_pipeline(self.pipeline)
    }

    pub const fn outcome_matches_policy(&self) -> bool {
        self.accepted == self.permitted_by_policy()
    }
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
    IoBudgetDecision(IoBudgetDecisionTrace),
    GpuPolicyDecision(GpuPolicyDecisionTrace),
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
            Self::IoBudgetDecision(trace) => trace.trace_id,
            Self::GpuPolicyDecision(trace) => trace.trace_id,
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
            Self::IoBudgetDecision(_) => CriticalDecisionKind::IoBudgetValidation,
            Self::GpuPolicyDecision(_) => CriticalDecisionKind::GpuPolicyDecision,
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
            | TraceEvent::Resource(_) => false,
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

pub trait EventSink {
    fn emit(&mut self, event: EventEnvelope) -> AndromedaResult<()>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryEventSink {
    events: Vec<EventEnvelope>,
    max_events: Option<usize>,
}

impl InMemoryEventSink {
    pub const fn new() -> Self {
        Self {
            events: Vec::new(),
            max_events: None,
        }
    }

    pub const fn with_capacity_limit(max_events: usize) -> Self {
        Self {
            events: Vec::new(),
            max_events: Some(max_events),
        }
    }

    pub fn events(&self) -> &[EventEnvelope] {
        &self.events
    }

    pub fn into_events(self) -> Vec<EventEnvelope> {
        self.events
    }
}

impl EventSink for InMemoryEventSink {
    fn emit(&mut self, event: EventEnvelope) -> AndromedaResult<()> {
        event.validate()?;

        if self
            .max_events
            .is_some_and(|max_events| self.events.len() >= max_events)
        {
            return Err(observe_error(
                "in-memory event sink capacity exhausted; event was not recorded",
            ));
        }

        self.events.push(event);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcedureLifecycleCursor {
    Empty,
    AdmissionAccepted,
    Authorized,
    IoAdmitted,
    WalFlushed,
    CommitVisible,
    RollbackDurable,
    CompletionEmitted,
    RecoveryStarted,
    PreTransactionRejected,
    PreTransactionCompletionEmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcedureLifecycleStep {
    AdmissionAccepted,
    Authorized,
    IoAdmitted,
    WalFlushed {
        transaction_id: TransactionId,
        durable_lsn: u64,
    },
    CommitVisible {
        transaction_id: TransactionId,
        durable_lsn: u64,
    },
    RollbackDurable {
        transaction_id: TransactionId,
        durable_lsn: u64,
    },
    CompletionEmitted {
        committed: bool,
        durable_lsn: Option<u64>,
    },
    RecoveryStarted {
        last_durable_lsn: u64,
    },
    PreTransactionRejected,
}

/// In-memory sequence validator for procedure lifecycle evidence.
///
/// This is not a filesystem, network, or external audit sink. It is a bounded
/// in-process evidence foundation for tests and local validation. Each appended
/// [`EventEnvelope`] is validated before it is stored, then checked against
/// procedure lifecycle ordering and correlation gates:
///
/// * request/session/contract/catalog identity is required before a transaction
///   can be observed and must remain stable for the sequence;
/// * transaction id and durable LSN are introduced only by a WAL flush;
/// * commit, rollback, completion, and recovery evidence must follow the
///   durable boundary they claim;
/// * pre-transaction rejection paths must not carry transaction or durable LSN
///   evidence.
#[derive(Debug, Clone)]
pub struct InMemoryEventSequence {
    events: Vec<EventEnvelope>,
    max_events: Option<usize>,
    cursor: ProcedureLifecycleCursor,
    last_event_id: Option<EventId>,
    request_id: Option<RequestId>,
    session_id: Option<SessionId>,
    contract_hash: Option<ContractHash>,
    catalog_version: Option<CatalogVersion>,
    catalog_object_id: Option<CatalogObjectId>,
    transaction_id: Option<TransactionId>,
    durable_lsn: Option<u64>,
}

/// Procedure lifecycle trace helper backed by an in-memory validated sequence.
pub type ProcedureLifecycleTrace = InMemoryEventSequence;

impl Default for InMemoryEventSequence {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEventSequence {
    pub const fn new() -> Self {
        Self {
            events: Vec::new(),
            max_events: None,
            cursor: ProcedureLifecycleCursor::Empty,
            last_event_id: None,
            request_id: None,
            session_id: None,
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
            transaction_id: None,
            durable_lsn: None,
        }
    }

    pub const fn with_capacity_limit(max_events: usize) -> Self {
        Self {
            events: Vec::new(),
            max_events: Some(max_events),
            cursor: ProcedureLifecycleCursor::Empty,
            last_event_id: None,
            request_id: None,
            session_id: None,
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
            transaction_id: None,
            durable_lsn: None,
        }
    }

    pub fn events(&self) -> &[EventEnvelope] {
        &self.events
    }

    pub fn into_events(self) -> Vec<EventEnvelope> {
        self.events
    }

    pub fn append(&mut self, event: EventEnvelope) -> AndromedaResult<()> {
        event.validate()?;

        if self
            .max_events
            .is_some_and(|max_events| self.events.len() >= max_events)
        {
            return Err(observe_error(
                "in-memory event sequence capacity exhausted; event was not recorded",
            ));
        }

        self.validate_event_id_order(event.event_id)?;
        self.validate_stable_procedure_correlation(&event)?;

        let step = ProcedureLifecycleStep::from_event(&event)?;
        let next = self.validate_transition(step, &event)?;

        self.apply_step(step, &event);
        self.cursor = next;
        self.last_event_id = Some(event.event_id);
        self.events.push(event);
        Ok(())
    }

    pub fn is_recovery_started(&self) -> bool {
        self.cursor == ProcedureLifecycleCursor::RecoveryStarted
    }

    pub fn has_terminal_pre_transaction_rejection(&self) -> bool {
        matches!(
            self.cursor,
            ProcedureLifecycleCursor::PreTransactionRejected
                | ProcedureLifecycleCursor::PreTransactionCompletionEmitted
        )
    }

    fn validate_event_id_order(&self, event_id: EventId) -> AndromedaResult<()> {
        if self
            .last_event_id
            .is_some_and(|last_event_id| event_id <= last_event_id)
        {
            return Err(observe_error(
                "procedure lifecycle event sequence requires strictly increasing event_id values",
            ));
        }

        Ok(())
    }

    fn validate_stable_procedure_correlation(&self, event: &EventEnvelope) -> AndromedaResult<()> {
        let correlation = event.correlation;
        if !correlation.has_request_session()
            || !correlation.has_contract_catalog()
            || !correlation
                .catalog_object_id
                .is_some_and(|catalog_object_id| catalog_object_id.get() != 0)
        {
            return Err(observe_error(
                "procedure lifecycle events require non-zero request_id, session_id, contract_hash, catalog_version, and catalog_object_id correlation",
            ));
        }

        Self::validate_anchor("request_id", self.request_id, correlation.request_id)?;
        Self::validate_anchor("session_id", self.session_id, correlation.session_id)?;
        Self::validate_anchor(
            "contract_hash",
            self.contract_hash,
            correlation.contract_hash,
        )?;
        Self::validate_anchor(
            "catalog_version",
            self.catalog_version,
            correlation.catalog_version,
        )?;
        Self::validate_anchor(
            "catalog_object_id",
            self.catalog_object_id,
            correlation.catalog_object_id,
        )
    }

    fn validate_anchor<T: Copy + Eq>(
        label: &str,
        expected: Option<T>,
        observed: Option<T>,
    ) -> AndromedaResult<()> {
        if let Some(expected) = expected
            && observed != Some(expected)
        {
            return Err(observe_error(format!(
                "procedure lifecycle {label} correlation must remain stable across the sequence",
            )));
        }

        Ok(())
    }

    fn validate_transition(
        &self,
        step: ProcedureLifecycleStep,
        event: &EventEnvelope,
    ) -> AndromedaResult<ProcedureLifecycleCursor> {
        match (self.cursor, step) {
            (ProcedureLifecycleCursor::Empty, ProcedureLifecycleStep::AdmissionAccepted) => self
                .require_no_transaction_evidence(
                    event,
                    ProcedureLifecycleCursor::AdmissionAccepted,
                    "admission evidence",
                ),
            (ProcedureLifecycleCursor::AdmissionAccepted, ProcedureLifecycleStep::Authorized) => {
                self.require_no_transaction_evidence(
                    event,
                    ProcedureLifecycleCursor::Authorized,
                    "authorization evidence",
                )
            }
            (ProcedureLifecycleCursor::Authorized, ProcedureLifecycleStep::IoAdmitted) => self
                .require_no_transaction_evidence(
                    event,
                    ProcedureLifecycleCursor::IoAdmitted,
                    "IO admission evidence",
                ),
            (
                ProcedureLifecycleCursor::IoAdmitted,
                ProcedureLifecycleStep::WalFlushed {
                    transaction_id,
                    durable_lsn,
                },
            ) => {
                self.require_transaction_boundary(event, transaction_id, durable_lsn)?;
                Ok(ProcedureLifecycleCursor::WalFlushed)
            }
            (
                ProcedureLifecycleCursor::WalFlushed,
                ProcedureLifecycleStep::CommitVisible {
                    transaction_id,
                    durable_lsn,
                },
            ) => {
                self.require_same_transaction_boundary(transaction_id, durable_lsn)?;
                Ok(ProcedureLifecycleCursor::CommitVisible)
            }
            (
                ProcedureLifecycleCursor::WalFlushed,
                ProcedureLifecycleStep::RollbackDurable {
                    transaction_id,
                    durable_lsn,
                },
            ) => {
                self.require_same_transaction_boundary(transaction_id, durable_lsn)?;
                Ok(ProcedureLifecycleCursor::RollbackDurable)
            }
            (
                ProcedureLifecycleCursor::CommitVisible,
                ProcedureLifecycleStep::CompletionEmitted {
                    committed: true,
                    durable_lsn: Some(durable_lsn),
                },
            ) => {
                self.require_same_durable_lsn(durable_lsn)?;
                Ok(ProcedureLifecycleCursor::CompletionEmitted)
            }
            (
                ProcedureLifecycleCursor::RollbackDurable,
                ProcedureLifecycleStep::CompletionEmitted {
                    committed: false,
                    durable_lsn: Some(durable_lsn),
                },
            ) => {
                self.require_same_durable_lsn(durable_lsn)?;
                Ok(ProcedureLifecycleCursor::CompletionEmitted)
            }
            (
                ProcedureLifecycleCursor::CompletionEmitted,
                ProcedureLifecycleStep::RecoveryStarted { last_durable_lsn },
            ) => {
                let durable_lsn = self.durable_lsn.ok_or_else(|| {
                    observe_error("recovery startup requires prior durable transaction evidence")
                })?;
                if last_durable_lsn < durable_lsn {
                    return Err(observe_error(
                        "recovery startup last durable LSN must not precede procedure durable LSN",
                    ));
                }
                Ok(ProcedureLifecycleCursor::RecoveryStarted)
            }
            (
                ProcedureLifecycleCursor::Empty | ProcedureLifecycleCursor::AdmissionAccepted,
                ProcedureLifecycleStep::PreTransactionRejected,
            ) => self.require_no_transaction_evidence(
                event,
                ProcedureLifecycleCursor::PreTransactionRejected,
                "pre-transaction rejection evidence",
            ),
            (
                ProcedureLifecycleCursor::PreTransactionRejected,
                ProcedureLifecycleStep::CompletionEmitted {
                    committed: false,
                    durable_lsn: None,
                },
            ) => self.require_no_transaction_evidence(
                event,
                ProcedureLifecycleCursor::PreTransactionCompletionEmitted,
                "pre-transaction completion evidence",
            ),
            _ => Err(observe_error(format!(
                "invalid procedure lifecycle transition from {:?} using {:?}",
                self.cursor,
                event.event.kind()
            ))),
        }
    }

    fn require_no_transaction_evidence(
        &self,
        event: &EventEnvelope,
        next: ProcedureLifecycleCursor,
        label: &str,
    ) -> AndromedaResult<ProcedureLifecycleCursor> {
        if !event.correlation.has_no_transaction_evidence() {
            return Err(observe_error(format!(
                "{label} must not include transaction_id or durable_lsn correlation before WAL flush",
            )));
        }

        Ok(next)
    }

    fn require_transaction_boundary(
        &self,
        event: &EventEnvelope,
        transaction_id: TransactionId,
        durable_lsn: u64,
    ) -> AndromedaResult<()> {
        if event.correlation.transaction_id != Some(transaction_id)
            || event.correlation.durable_lsn != Some(durable_lsn)
        {
            return Err(observe_error(
                "WAL flush must introduce matching transaction_id and durable_lsn correlation",
            ));
        }

        Ok(())
    }

    fn require_same_transaction_boundary(
        &self,
        transaction_id: TransactionId,
        durable_lsn: u64,
    ) -> AndromedaResult<()> {
        if self.transaction_id != Some(transaction_id) || self.durable_lsn != Some(durable_lsn) {
            return Err(observe_error(
                "transaction terminal evidence must match the WAL flush transaction_id and durable_lsn",
            ));
        }

        Ok(())
    }

    fn require_same_durable_lsn(&self, durable_lsn: u64) -> AndromedaResult<()> {
        if self.durable_lsn != Some(durable_lsn) {
            return Err(observe_error(
                "completion evidence must match the durable LSN established by WAL flush",
            ));
        }

        Ok(())
    }

    fn apply_step(&mut self, step: ProcedureLifecycleStep, event: &EventEnvelope) {
        self.request_id = event.correlation.request_id;
        self.session_id = event.correlation.session_id;
        self.contract_hash = event.correlation.contract_hash;
        self.catalog_version = event.correlation.catalog_version;
        self.catalog_object_id = event.correlation.catalog_object_id;

        match step {
            ProcedureLifecycleStep::WalFlushed {
                transaction_id,
                durable_lsn,
            }
            | ProcedureLifecycleStep::CommitVisible {
                transaction_id,
                durable_lsn,
            }
            | ProcedureLifecycleStep::RollbackDurable {
                transaction_id,
                durable_lsn,
            } => {
                self.transaction_id = Some(transaction_id);
                self.durable_lsn = Some(durable_lsn);
            }
            ProcedureLifecycleStep::RecoveryStarted { last_durable_lsn } => {
                self.durable_lsn = Some(last_durable_lsn);
            }
            ProcedureLifecycleStep::AdmissionAccepted
            | ProcedureLifecycleStep::Authorized
            | ProcedureLifecycleStep::IoAdmitted
            | ProcedureLifecycleStep::CompletionEmitted { .. }
            | ProcedureLifecycleStep::PreTransactionRejected => {}
        }
    }
}

impl ProcedureLifecycleStep {
    fn from_event(event: &EventEnvelope) -> AndromedaResult<Self> {
        match &event.event {
            TraceEvent::Decision(trace)
                if trace.decision == CriticalDecisionKind::ContractValidation =>
            {
                Ok(Self::AdmissionAccepted)
            }
            TraceEvent::Decision(trace)
                if trace.decision == CriticalDecisionKind::SecurityAuthorization =>
            {
                Ok(Self::Authorized)
            }
            TraceEvent::SecurityAudit(trace) if trace.outcome == SecurityAuditOutcome::Allowed => {
                Ok(Self::Authorized)
            }
            TraceEvent::Decision(trace)
                if matches!(
                    trace.decision,
                    CriticalDecisionKind::IoBudgetValidation
                        | CriticalDecisionKind::IoPlacementDecision
                ) =>
            {
                Ok(Self::IoAdmitted)
            }
            TraceEvent::IoBudgetDecision(trace) if trace.accepted => Ok(Self::IoAdmitted),
            TraceEvent::IoPlacementDecision(trace) if trace.accepted => Ok(Self::IoAdmitted),
            TraceEvent::WalEvent(trace) if trace.operation == WalOperation::Flush => {
                let transaction_id = trace.transaction_id.ok_or_else(|| {
                    observe_error("procedure lifecycle WAL flush requires transaction_id evidence")
                })?;
                let durable_lsn = trace.durable_lsn.ok_or_else(|| {
                    observe_error("procedure lifecycle WAL flush requires durable_lsn evidence")
                })?;
                Ok(Self::WalFlushed {
                    transaction_id,
                    durable_lsn,
                })
            }
            TraceEvent::CommitVisible(trace) => Ok(Self::CommitVisible {
                transaction_id: trace.transaction_id,
                durable_lsn: trace.durable_commit_lsn,
            }),
            TraceEvent::RollbackDurable(trace) => Ok(Self::RollbackDurable {
                transaction_id: trace.transaction_id,
                durable_lsn: trace.durable_rollback_lsn,
            }),
            TraceEvent::CompletionEmitted(trace) => Ok(Self::CompletionEmitted {
                committed: trace.committed,
                durable_lsn: trace.durable_lsn,
            }),
            TraceEvent::RecoveryStartup(trace) => Ok(Self::RecoveryStarted {
                last_durable_lsn: trace.last_durable_lsn,
            }),
            TraceEvent::ContractRejected(_)
            | TraceEvent::AuthorizationDenied(_)
            | TraceEvent::SecurityAudit(SecurityAuditTrace {
                outcome: SecurityAuditOutcome::Denied,
                ..
            }) => Ok(Self::PreTransactionRejected),
            _ => Err(observe_error(
                "event is not accepted as procedure lifecycle sequence evidence",
            )),
        }
    }
}

impl EventSink for InMemoryEventSequence {
    fn emit(&mut self, event: EventEnvelope) -> AndromedaResult<()> {
        self.append(event)
    }
}

fn observe_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Internal, message)
}

fn non_empty_reason(reason: impl Into<String>) -> AndromedaResult<String> {
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

fn contains_sensitive_marker(text: &str) -> bool {
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
}
