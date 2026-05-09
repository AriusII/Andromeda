use andromeda_error::AndromedaResult;
use andromeda_principal::{Permission, Principal, PrincipalId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allowed {
        principal_id: PrincipalId,
        granted_permission: Permission,
    },
    Denied {
        principal_id: Option<PrincipalId>,
        required_permission: Permission,
        reason: DenialReason,
    },
}

impl PermissionDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Denied { .. })
    }

    pub fn reason_str(&self) -> &str {
        match self {
            Self::Allowed { .. } => "permission_allowed",
            Self::Denied { reason, .. } => reason.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialReason {
    PrincipalNotFound,
    MissingPermission,
    NoPermissionsGranted,
    InternalError,
}

impl DenialReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PrincipalNotFound => "principal_not_found",
            Self::MissingPermission => "missing_permission",
            Self::NoPermissionsGranted => "no_permissions_granted",
            Self::InternalError => "internal_error",
        }
    }
}

impl std::fmt::Display for DenialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub trait PermissionEvaluator: Send + Sync {
    fn evaluate_permission(
        &self,
        cert_fingerprint: &str,
        required_permission: &Permission,
    ) -> PermissionDecision;

    fn evaluate_all_permissions(
        &self,
        cert_fingerprint: &str,
        required_permissions: &[Permission],
    ) -> Result<(), PermissionDecision>;

    fn get_principal(&self, cert_fingerprint: &str) -> AndromedaResult<Principal>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_decision_allowed_reports_reason() {
        let decision = PermissionDecision::Allowed {
            principal_id: PrincipalId::new(1),
            granted_permission: Permission::AdminCatalogPublish,
        };

        assert!(decision.is_allowed());
        assert!(!decision.is_denied());
        assert_eq!(decision.reason_str(), "permission_allowed");
    }

    #[test]
    fn permission_decision_denied_reports_reason() {
        let decision = PermissionDecision::Denied {
            principal_id: Some(PrincipalId::new(1)),
            required_permission: Permission::AdminShutdown,
            reason: DenialReason::MissingPermission,
        };

        assert!(!decision.is_allowed());
        assert!(decision.is_denied());
        assert_eq!(decision.reason_str(), "missing_permission");
    }
}
