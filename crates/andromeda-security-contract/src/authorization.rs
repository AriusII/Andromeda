/// Stable authorization denial reasons shared by security runtime and audit evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorizationDenialReason {
    UnknownCertificate,
    SurfaceScopeMismatch,
    SurfaceDoesNotPermitPermission,
    PrincipalMissingPermission,
}

pub const ALL_AUTHORIZATION_DENIAL_REASONS: [AuthorizationDenialReason; 4] = [
    AuthorizationDenialReason::UnknownCertificate,
    AuthorizationDenialReason::SurfaceScopeMismatch,
    AuthorizationDenialReason::SurfaceDoesNotPermitPermission,
    AuthorizationDenialReason::PrincipalMissingPermission,
];

impl AuthorizationDenialReason {
    pub const fn label(self) -> &'static str {
        match self {
            Self::UnknownCertificate => "unknown_certificate",
            Self::SurfaceScopeMismatch => "surface_scope_mismatch",
            Self::SurfaceDoesNotPermitPermission => "surface_does_not_permit_permission",
            Self::PrincipalMissingPermission => "principal_missing_permission",
        }
    }

    pub fn from_audit_reason(reason: &str) -> Option<Self> {
        let reason = reason.trim();
        let label = reason.strip_prefix("denied:")?.split(':').next()?;
        match label {
            "unknown_certificate" => Some(Self::UnknownCertificate),
            "surface_scope_mismatch" => Some(Self::SurfaceScopeMismatch),
            "surface_does_not_permit_permission" => Some(Self::SurfaceDoesNotPermitPermission),
            "principal_missing_permission" => Some(Self::PrincipalMissingPermission),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_denial_reasons_keep_canonical_labels() {
        let expected = [
            (
                AuthorizationDenialReason::UnknownCertificate,
                "unknown_certificate",
            ),
            (
                AuthorizationDenialReason::SurfaceScopeMismatch,
                "surface_scope_mismatch",
            ),
            (
                AuthorizationDenialReason::SurfaceDoesNotPermitPermission,
                "surface_does_not_permit_permission",
            ),
            (
                AuthorizationDenialReason::PrincipalMissingPermission,
                "principal_missing_permission",
            ),
        ];

        for (index, (reason, label)) in expected.into_iter().enumerate() {
            assert_eq!(ALL_AUTHORIZATION_DENIAL_REASONS[index], reason);
            assert_eq!(reason.label(), label);
            assert_eq!(
                AuthorizationDenialReason::from_audit_reason(&format!("denied:{label}:context")),
                Some(reason)
            );
        }
    }
}
