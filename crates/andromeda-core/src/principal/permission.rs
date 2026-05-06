use crate::ProcedureId;

/// Atomic permission in the RBAC model.
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
}

impl Permission {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ExecuteProcedure(_) => "execute_procedure",
            Self::ReadContractMetadata => "read_contract_metadata",
            Self::AdminRoleManagement => "admin_role_management",
            Self::AdminCatalogPublish => "admin_catalog_publish",
            Self::AdminShutdown => "admin_shutdown",
            Self::AdminRecovery => "admin_recovery",
            Self::AuditRead => "audit_read",
            Self::AdminCertificateRotate => "admin_certificate_rotate",
        }
    }

    /// Exact match, except `ExecuteProcedure(u64::MAX)` grants all procedures.
    pub fn matches(&self, required: &Permission) -> bool {
        match (self, required) {
            (Self::ExecuteProcedure(granted_id), Self::ExecuteProcedure(required_id)) => {
                granted_id.get() == u64::MAX || granted_id == required_id
            }
            (a, b) => a == b,
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExecuteProcedure(pid) => write!(f, "execute_procedure({})", pid.get()),
            _ => write!(f, "{}", self.as_str()),
        }
    }
}
