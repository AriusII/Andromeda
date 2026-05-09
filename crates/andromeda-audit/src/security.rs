use andromeda_error::AndromedaResult;

use crate::TraceId;
use crate::identity::{has_identity_pair_evidence, identity_pair_contains_sensitive_evidence};

use super::EventSchemaVersion;
use super::{
    CertificateIdentity, Permission, SecurityPolicyVersionEvidence, SurfaceScope, UserPrincipal,
    contains_sensitive_marker, non_empty_reason, observe_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAuditOutcome {
    Allowed,
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAuditDenialReason {
    UnknownCertificate,
    SurfaceScopeMismatch,
    SurfaceDoesNotPermitPermission,
    PrincipalMissingPermission,
}

impl SecurityAuditDenialReason {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityAuditTrace {
    pub trace_id: TraceId,
    pub schema_version: EventSchemaVersion,
    pub surface: SurfaceScope,
    pub certificate: CertificateIdentity,
    pub principal: UserPrincipal,
    pub permission: Permission,
    pub outcome: SecurityAuditOutcome,
    pub policy_version: SecurityPolicyVersionEvidence,
    pub reason: String,
}

impl SecurityAuditTrace {
    pub fn new(
        trace_id: TraceId,
        surface: SurfaceScope,
        certificate: CertificateIdentity,
        principal: UserPrincipal,
        permission: Permission,
        outcome: SecurityAuditOutcome,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Self::new_with_policy_version(
            trace_id,
            surface,
            certificate,
            principal,
            permission,
            outcome,
            SecurityPolicyVersionEvidence::try_bootstrap_v0()?,
            reason,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Security audit construction keeps identity, permission, decision, policy version, and reason evidence explicit."
    )]
    pub fn new_with_policy_version(
        trace_id: TraceId,
        surface: SurfaceScope,
        certificate: CertificateIdentity,
        principal: UserPrincipal,
        permission: Permission,
        outcome: SecurityAuditOutcome,
        policy_version: SecurityPolicyVersionEvidence,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        if !policy_version.has_version_evidence() {
            return Err(observe_error(
                "security audit traces require security policy version evidence",
            ));
        }
        Ok(Self {
            trace_id,
            schema_version: EventSchemaVersion::V0,
            surface,
            certificate,
            principal,
            permission,
            outcome,
            policy_version,
            reason: non_empty_reason(reason)?,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_supported_schema_version(&self) -> bool {
        self.schema_version.is_v0()
    }

    pub fn has_identity_evidence(&self) -> bool {
        has_identity_pair_evidence(&self.certificate, &self.principal)
    }

    pub const fn surface_matches_certificate(&self) -> bool {
        self.surface.same_surface(self.certificate.surface)
    }

    pub fn denial_reason(&self) -> Option<SecurityAuditDenialReason> {
        if !matches!(self.outcome, SecurityAuditOutcome::Denied) {
            return None;
        }
        SecurityAuditDenialReason::from_audit_reason(&self.reason)
    }

    pub fn has_typed_surface_scope_mismatch_denial(&self) -> bool {
        self.denial_reason() == Some(SecurityAuditDenialReason::SurfaceScopeMismatch)
    }

    pub const fn surface_permits_permission(&self) -> bool {
        self.surface.permits_permission(self.permission)
    }

    pub fn has_policy_version_evidence(&self) -> bool {
        self.policy_version.has_version_evidence()
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        identity_pair_contains_sensitive_evidence(&self.certificate, &self.principal)
            || self.policy_version.contains_sensitive_evidence()
            || contains_sensitive_marker(&self.reason)
    }
}
