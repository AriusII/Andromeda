use super::super::CertificateIdentityStatus;

/// Machine-classifiable denial reason for IAM policy decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalAuthorizationDenialReason {
    UnknownCertificate,
    CertificateDisabled,
    CertificateRevoked,
    SurfaceScopeMismatch,
    PrincipalDisabled,
    SurfaceDoesNotPermitPermission,
    PrincipalMissingPermission,
}

impl PrincipalAuthorizationDenialReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownCertificate => "unknown_certificate",
            Self::CertificateDisabled => "certificate_disabled",
            Self::CertificateRevoked => "certificate_revoked",
            Self::SurfaceScopeMismatch => "surface_scope_mismatch",
            Self::PrincipalDisabled => "principal_disabled",
            Self::SurfaceDoesNotPermitPermission => "surface_does_not_permit_permission",
            Self::PrincipalMissingPermission => "principal_missing_permission",
        }
    }

    pub const fn evaluation_stage(self) -> PrincipalAuthorizationEvaluationStage {
        match self {
            Self::UnknownCertificate => PrincipalAuthorizationEvaluationStage::CertificateLookup,
            Self::CertificateDisabled => PrincipalAuthorizationEvaluationStage::CertificateStatus,
            Self::CertificateRevoked => PrincipalAuthorizationEvaluationStage::CertificateStatus,
            Self::SurfaceScopeMismatch => PrincipalAuthorizationEvaluationStage::SurfaceScope,
            Self::PrincipalDisabled => PrincipalAuthorizationEvaluationStage::PrincipalStatus,
            Self::SurfaceDoesNotPermitPermission => {
                PrincipalAuthorizationEvaluationStage::SurfacePolicy
            }
            Self::PrincipalMissingPermission => PrincipalAuthorizationEvaluationStage::Permission,
        }
    }
}

impl std::fmt::Display for PrincipalAuthorizationDenialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable audit stage reached by an IAM authorization decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalAuthorizationEvaluationStage {
    CertificateLookup,
    CertificateStatus,
    SurfaceScope,
    PrincipalStatus,
    SurfacePolicy,
    Permission,
    Allowed,
}

impl PrincipalAuthorizationEvaluationStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CertificateLookup => "certificate_lookup",
            Self::CertificateStatus => "certificate_status",
            Self::SurfaceScope => "surface_scope",
            Self::PrincipalStatus => "principal_status",
            Self::SurfacePolicy => "surface_policy",
            Self::Permission => "permission",
            Self::Allowed => "allowed",
        }
    }
}

impl std::fmt::Display for PrincipalAuthorizationEvaluationStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub(super) fn non_active_certificate_status_denial_reason(
    status: CertificateIdentityStatus,
) -> PrincipalAuthorizationDenialReason {
    debug_assert!(!matches!(status, CertificateIdentityStatus::Active));
    match status {
        CertificateIdentityStatus::Active => PrincipalAuthorizationDenialReason::CertificateRevoked,
        CertificateIdentityStatus::Disabled => {
            PrincipalAuthorizationDenialReason::CertificateDisabled
        }
        CertificateIdentityStatus::Revoked => {
            PrincipalAuthorizationDenialReason::CertificateRevoked
        }
    }
}
