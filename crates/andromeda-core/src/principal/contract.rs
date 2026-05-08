use super::registry::PrincipalPolicyVersion;
use super::{Permission, SurfaceScope};
use crate::ProcedureId;
use andromeda_security_contract as security_contract;

impl SurfaceScope {
    /// Projects the core certificate scope onto the public security contract surface vocabulary.
    pub const fn to_security_surface(self) -> security_contract::SecuritySurface {
        match self {
            Self::Application => security_contract::SecuritySurface::Application,
            Self::Administration => security_contract::SecuritySurface::Administration,
            Self::Cluster => security_contract::SecuritySurface::Cluster,
            Self::BackupAgent => security_contract::SecuritySurface::BackupAgent,
            Self::MonitoringAgent => security_contract::SecuritySurface::MonitoringAgent,
        }
    }
}

impl Permission {
    /// Projects this core permission to a security contract permission when the semantics are exact.
    pub const fn to_security_permission(&self) -> Option<security_contract::SecurityPermission> {
        match self {
            Self::ExecuteProcedure(_) => {
                Some(security_contract::SecurityPermission::ExecuteProcedure)
            }
            Self::ReadContractMetadata => {
                Some(security_contract::SecurityPermission::ReadContractMetadata)
            }
            Self::AdminRoleManagement
            | Self::AdminCatalogPublish
            | Self::AdminShutdown
            | Self::AdminRecovery => None,
            Self::AuditRead => Some(security_contract::SecurityPermission::ReadAudit),
            Self::AdminCertificateRotate => {
                Some(security_contract::SecurityPermission::RotateCertificate)
            }
            Self::ClusterPromote => Some(security_contract::SecurityPermission::ClusterPromote),
            Self::ClusterFenceNode => Some(security_contract::SecurityPermission::ClusterFenceNode),
            Self::ClusterManifestUpdate => {
                Some(security_contract::SecurityPermission::ClusterUpdateManifest)
            }
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

impl PrincipalPolicyVersion {
    /// Projects the core principal policy version onto the public security contract version shape.
    pub const fn to_security_policy_version(self) -> security_contract::SecurityPolicyVersion {
        security_contract::SecurityPolicyVersion::new(self.as_bytes())
    }

    /// Projects the core principal policy version to validated security contract evidence.
    pub fn to_security_policy_evidence(
        self,
    ) -> Result<security_contract::SecurityPolicyEvidence, security_contract::SecurityContractError>
    {
        security_contract::SecurityPolicyEvidence::for_policy_version(
            self.to_security_policy_version(),
        )
    }
}
