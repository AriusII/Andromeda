use crate::{AdminOperation, Permission, SurfaceScope};

/// Permission matrix for trace query access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceQueryPermissionMatrix {
    pub surface: SurfaceScope,
    pub required_permission: Permission,
    pub audit_operation: AdminOperation,
    pub audit_required: bool,
}

impl TraceQueryPermissionMatrix {
    /// Administration-only trace query gate.
    pub const V1_ADMIN: Self = Self {
        surface: SurfaceScope::Administration,
        required_permission: Permission::InspectPlans,
        audit_operation: AdminOperation::InspectPlans,
        audit_required: true,
    };

    pub const fn permits(self, surface: SurfaceScope, permission: Permission) -> bool {
        matches!(
            (surface, self.surface),
            (SurfaceScope::Application, SurfaceScope::Application)
                | (SurfaceScope::Administration, SurfaceScope::Administration)
                | (SurfaceScope::Cluster, SurfaceScope::Cluster)
                | (SurfaceScope::BackupAgent, SurfaceScope::BackupAgent)
                | (SurfaceScope::MonitoringAgent, SurfaceScope::MonitoringAgent)
        ) && matches!(
            permission,
            Permission::InspectPlans | Permission::ManageSecurity
        )
    }
}
