use andromeda_types::ProcedureId;

pub const ALL_PROCEDURES: ProcedureId = ProcedureId::new(u64::MAX);

/// Atomic permission in the principal RBAC model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Permission {
    ExecuteProcedure(ProcedureId),
    ReadContractMetadata,
    AdminRoleManagement,
    AdminCatalogPublish,
    AdminShutdown,
    AdminRecovery,
    AuditRead,
    AdminCertificateRotate,
    ClusterPromote,
    ClusterFenceNode,
    ClusterManifestUpdate,
}

impl Permission {
    pub const fn as_str(&self) -> &str {
        match self {
            Self::ExecuteProcedure(_) => "execute_procedure",
            Self::ReadContractMetadata => "read_contract_metadata",
            Self::AdminRoleManagement => "admin_role_management",
            Self::AdminCatalogPublish => "admin_catalog_publish",
            Self::AdminShutdown => "admin_shutdown",
            Self::AdminRecovery => "admin_recovery",
            Self::AuditRead => "audit_read",
            Self::AdminCertificateRotate => "admin_certificate_rotate",
            Self::ClusterPromote => "cluster_promote",
            Self::ClusterFenceNode => "cluster_fence_node",
            Self::ClusterManifestUpdate => "cluster_manifest_update",
        }
    }

    /// Exact match, except `ExecuteProcedure(u64::MAX)` grants all procedures.
    pub fn matches(&self, required: &Permission) -> bool {
        match (self, required) {
            (Self::ExecuteProcedure(granted_id), Self::ExecuteProcedure(required_id)) => {
                *granted_id == ALL_PROCEDURES || granted_id == required_id
            },
            (a, b) => a == b,
        }
    }
}

impl core::fmt::Display for Permission {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ExecuteProcedure(pid) => write!(f, "execute_procedure({})", pid.get()),
            _ => f.write_str(self.as_str()),
        }
    }
}
