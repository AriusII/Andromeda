use super::permission::ALL_PROCEDURES;
use super::{Permission, PrincipalRole};
use andromeda_types::ProcedureId;
use std::collections::HashSet;

/// Set of permissions granted to a principal via role assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionSet {
    permissions: HashSet<Permission>,
}

impl PermissionSet {
    pub fn new() -> Self {
        Self {
            permissions: HashSet::new(),
        }
    }

    pub fn from_vec(permissions: Vec<Permission>) -> Self {
        Self {
            permissions: permissions.into_iter().collect(),
        }
    }

    pub fn with_permission(mut self, permission: Permission) -> Self {
        self.permissions.insert(permission);
        self
    }

    pub fn has_permission(&self, required: &Permission) -> bool {
        self.permissions.iter().any(|p| p.matches(required))
    }

    pub fn len(&self) -> usize {
        self.permissions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.permissions.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Permission> {
        self.permissions.iter()
    }

    pub(crate) fn for_role(role: PrincipalRole) -> Self {
        match role {
            PrincipalRole::SuperAdmin => Self::super_admin_permissions(),
            PrincipalRole::Admin => Self::admin_permissions(),
            PrincipalRole::Operator => Self::operator_permissions(),
            PrincipalRole::User => Self::user_permissions(),
            PrincipalRole::Guest => Self::guest_permissions(),
        }
    }

    fn super_admin_permissions() -> Self {
        Self::new()
            .with_permission(Permission::ExecuteProcedure(ALL_PROCEDURES))
            .with_permission(Permission::ReadContractMetadata)
            .with_permission(Permission::AdminRoleManagement)
            .with_permission(Permission::AdminCatalogPublish)
            .with_permission(Permission::AdminShutdown)
            .with_permission(Permission::AdminRecovery)
            .with_permission(Permission::AuditRead)
            .with_permission(Permission::AdminCertificateRotate)
            .with_permission(Permission::ClusterPromote)
            .with_permission(Permission::ClusterFenceNode)
            .with_permission(Permission::ClusterManifestUpdate)
    }

    fn admin_permissions() -> Self {
        Self::new()
            .with_permission(Permission::ReadContractMetadata)
            .with_permission(Permission::AdminRoleManagement)
            .with_permission(Permission::AdminCatalogPublish)
            .with_permission(Permission::AdminRecovery)
            .with_permission(Permission::AuditRead)
            .with_permission(Permission::AdminCertificateRotate)
    }

    fn operator_permissions() -> Self {
        Self::new()
            .with_permission(Permission::ExecuteProcedure(ALL_PROCEDURES))
            .with_permission(Permission::ReadContractMetadata)
            .with_permission(Permission::AuditRead)
            .with_permission(Permission::AdminRecovery)
    }

    fn user_permissions() -> Self {
        Self::new()
            .with_permission(Permission::ExecuteProcedure(ALL_PROCEDURES))
            .with_permission(Permission::ReadContractMetadata)
    }

    fn guest_permissions() -> Self {
        Self::new().with_permission(Permission::ExecuteProcedure(ProcedureId::new(0)))
    }
}

impl Default for PermissionSet {
    fn default() -> Self {
        Self::new()
    }
}
