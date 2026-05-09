use super::*;

impl Principal {
    pub fn permissions(&self) -> PermissionSet {
        if self.is_active() {
            self.role.permissions()
        } else {
            PermissionSet::new()
        }
    }

    pub fn has_permission(&self, required: &Permission) -> bool {
        self.is_active() && self.permissions().has_permission(required)
    }

    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    pub fn with_status(&self, status: PrincipalStatus) -> Self {
        let mut principal = self.clone();
        principal.status = status;
        principal
    }
}
