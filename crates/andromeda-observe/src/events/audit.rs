use crate::TraceId;
use andromeda_core::AndromedaResult;

use super::{EventSchemaVersion, contains_sensitive_marker, non_empty_reason, observe_error};

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

fn non_empty_evidence(label: &str, value: impl Into<String>) -> AndromedaResult<String> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(observe_error(format!(
            "observability {label} evidence requires a non-empty value",
        )));
    }

    Ok(value)
}
