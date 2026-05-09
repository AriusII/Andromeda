use super::{Permission as PrincipalPermission, SurfaceScope};
use crate::{Permission as SecurityPermission, SecuritySurface};
use andromeda_types::ProcedureId;

impl SurfaceScope {
    /// Projects the principal certificate scope onto the public security contract surface vocabulary.
    pub const fn to_security_surface(self) -> SecuritySurface {
        match self {
            Self::Application => SecuritySurface::Application,
            Self::Administration => SecuritySurface::Administration,
            Self::Cluster => SecuritySurface::Cluster,
            Self::BackupAgent => SecuritySurface::BackupAgent,
            Self::MonitoringAgent => SecuritySurface::MonitoringAgent,
        }
    }
}

impl PrincipalPermission {
    /// Projects this principal permission to a security contract permission when the semantics are exact.
    pub const fn to_security_permission(&self) -> Option<SecurityPermission> {
        match self {
            Self::ExecuteProcedure(_) => Some(SecurityPermission::ExecuteProcedure),
            Self::ReadContractMetadata => Some(SecurityPermission::ReadContractMetadata),
            Self::AdminRoleManagement
            | Self::AdminCatalogPublish
            | Self::AdminShutdown
            | Self::AdminRecovery => None,
            Self::AuditRead => Some(SecurityPermission::ReadAudit),
            Self::AdminCertificateRotate => Some(SecurityPermission::RotateCertificate),
            Self::ClusterPromote => Some(SecurityPermission::ClusterPromote),
            Self::ClusterFenceNode => Some(SecurityPermission::ClusterFenceNode),
            Self::ClusterManifestUpdate => Some(SecurityPermission::ClusterUpdateManifest),
        }
    }

    /// Returns the procedure identifier carried by `ExecuteProcedure` without embedding it in the contract permission.
    pub const fn security_contract_procedure_id(&self) -> Option<ProcedureId> {
        match self {
            Self::ExecuteProcedure(procedure_id) => Some(*procedure_id),
            _ => None,
        }
    }
}
