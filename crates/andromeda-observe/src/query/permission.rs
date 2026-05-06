use crate::{AdminOperation, Permission, SurfaceScope};

/// Permission/audit matrix for V1 administration trace access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceQueryPermissionMatrix {
    pub surface: SurfaceScope,
    pub required_permission: Permission,
    pub audit_operation: AdminOperation,
    pub audit_required: bool,
}

impl TraceQueryPermissionMatrix {
    /// V1 trace query is administration-only and uses the existing diagnostics
    /// permission until IAM adds a dedicated trace-read permission.
    pub const V1_ADMIN: Self = Self {
        surface: SurfaceScope::Administration,
        required_permission: Permission::InspectPlans,
        audit_operation: AdminOperation::InspectPlans,
        audit_required: true,
    };

    pub const fn permits(self, surface: SurfaceScope, permission: Permission) -> bool {
        surface as u8 == self.surface as u8
            && matches!(
                permission,
                Permission::InspectPlans | Permission::ManageSecurity
            )
    }
}
