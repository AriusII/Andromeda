use andromeda_audit::{Permission, SurfaceScope};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::SurfaceAction;

/// Formal, bounded emergency override policy.
///
/// This policy is intentionally narrow: it can authorize only explicitly
/// listed permissions and surfaces, during a bounded time window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakGlassPolicy {
    ticket_id: String,
    approved_by: String,
    issued_at_epoch_secs: u64,
    expires_at_epoch_secs: u64,
    allowed_permissions: Vec<Permission>,
    allowed_surfaces: Vec<SurfaceScope>,
}

impl BreakGlassPolicy {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ticket_id: impl Into<String>,
        approved_by: impl Into<String>,
        issued_at_epoch_secs: u64,
        expires_at_epoch_secs: u64,
        allowed_permissions: Vec<Permission>,
        allowed_surfaces: Vec<SurfaceScope>,
    ) -> AndromedaResult<Self> {
        let ticket_id = non_empty("break-glass ticket id", ticket_id)?;
        let approved_by = non_empty("break-glass approver", approved_by)?;
        if expires_at_epoch_secs <= issued_at_epoch_secs {
            return Err(security_error(
                "break-glass policy requires expires_at > issued_at",
            ));
        }

        if allowed_permissions.is_empty() {
            return Err(security_error(
                "break-glass policy requires at least one allowed permission",
            ));
        }
        if allowed_surfaces.is_empty() {
            return Err(security_error(
                "break-glass policy requires at least one allowed surface",
            ));
        }

        // Break-glass is strictly an operator override and must not bypass the
        // procedure-only application execution surface.
        if allowed_permissions.iter().any(|permission| {
            matches!(
                permission,
                Permission::ExecuteProcedure | Permission::ReadContract
            )
        }) {
            return Err(security_error(
                "break-glass policy cannot grant application-plane permissions",
            ));
        }
        if allowed_surfaces.contains(&SurfaceScope::Application) {
            return Err(security_error(
                "break-glass policy cannot target application surface",
            ));
        }

        Ok(Self {
            ticket_id,
            approved_by,
            issued_at_epoch_secs,
            expires_at_epoch_secs,
            allowed_permissions,
            allowed_surfaces,
        })
    }

    pub fn is_active_at(&self, now_epoch_secs: u64) -> bool {
        now_epoch_secs >= self.issued_at_epoch_secs && now_epoch_secs <= self.expires_at_epoch_secs
    }

    pub fn allows(&self, scope: SurfaceScope, permission: Permission, now_epoch_secs: u64) -> bool {
        self.is_active_at(now_epoch_secs)
            && self.allowed_surfaces.contains(&scope)
            && self.allowed_permissions.contains(&permission)
    }

    pub fn allows_action(
        &self,
        scope: SurfaceScope,
        action: SurfaceAction,
        now_epoch_secs: u64,
    ) -> bool {
        self.allows(scope, action.required_permission(), now_epoch_secs)
    }

    pub fn ticket_id(&self) -> &str {
        &self.ticket_id
    }

    pub fn approved_by(&self) -> &str {
        &self.approved_by
    }
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}

fn non_empty(label: &'static str, value: impl Into<String>) -> AndromedaResult<String> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(security_error(match label {
            "break-glass ticket id" => "break-glass ticket id must not be empty",
            _ => "break-glass approver must not be empty",
        }));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_audit::AdminOperation;

    #[test]
    fn break_glass_policy_requires_valid_time_window() {
        let err = BreakGlassPolicy::new(
            "INC-1001",
            "ops-lead",
            100,
            100,
            vec![Permission::ManageSecurity],
            vec![SurfaceScope::Administration],
        )
        .unwrap_err();
        assert!(err.message().contains("expires_at > issued_at"));
    }

    #[test]
    fn break_glass_policy_rejects_application_plane_scope_and_permissions() {
        let err = BreakGlassPolicy::new(
            "INC-1002",
            "ops-lead",
            100,
            200,
            vec![Permission::ExecuteProcedure],
            vec![SurfaceScope::Application],
        )
        .unwrap_err();
        assert!(err.message().contains("application-plane"));
    }

    #[test]
    fn break_glass_policy_is_bounded_and_explicit() {
        let policy = BreakGlassPolicy::new(
            "INC-1003",
            "ops-lead",
            100,
            200,
            vec![Permission::ManageSecurity, Permission::ClusterPromote],
            vec![SurfaceScope::Administration, SurfaceScope::Cluster],
        )
        .unwrap();

        assert!(policy.allows(
            SurfaceScope::Administration,
            Permission::ManageSecurity,
            150
        ));
        assert!(policy.allows_action(
            SurfaceScope::Cluster,
            SurfaceAction::Admin(AdminOperation::ClusterPromote),
            180
        ));

        assert!(!policy.allows(SurfaceScope::Administration, Permission::Backup, 150));
        assert!(!policy.allows(
            SurfaceScope::Administration,
            Permission::ManageSecurity,
            250
        ));
    }
}
