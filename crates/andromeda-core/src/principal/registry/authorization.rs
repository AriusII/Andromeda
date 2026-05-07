use super::super::{
    CertificateIdentityStatus, Permission, PrincipalId, PrincipalStatus, SurfaceScope,
};
use super::binding_store::PrincipalBinding;
use super::denial::{
    PrincipalAuthorizationDenialReason, PrincipalAuthorizationEvaluationStage,
    non_active_certificate_status_denial_reason,
};
use super::policy::{PrincipalPolicyEvidenceBinding, PrincipalPolicyVersion};
use crate::AndromedaResult;

pub(super) fn authorize(
    policy_version: PrincipalPolicyVersion,
    requested_scope: SurfaceScope,
    presented_fingerprint: &str,
    required_permission: &Permission,
    binding: Option<&PrincipalBinding>,
) -> PrincipalAuthorizationDecision {
    let Some(binding) = binding else {
        return PrincipalAuthorizationDecision::denied(
            AuthorizationDecisionSeed::unknown(
                policy_version,
                requested_scope,
                presented_fingerprint,
                required_permission.clone(),
            ),
            PrincipalAuthorizationDenialReason::UnknownCertificate,
        );
    };

    if !binding.certificate().is_active() {
        return PrincipalAuthorizationDecision::denied_from_binding(
            requested_scope,
            policy_version,
            binding,
            required_permission.clone(),
            non_active_certificate_status_denial_reason(binding.certificate().status()),
            AuthorizationCheckState::not_evaluated(),
        );
    }

    if binding.certificate().surface_scope() != requested_scope {
        return PrincipalAuthorizationDecision::denied_from_binding(
            requested_scope,
            policy_version,
            binding,
            required_permission.clone(),
            PrincipalAuthorizationDenialReason::SurfaceScopeMismatch,
            AuthorizationCheckState::not_evaluated(),
        );
    }

    if !binding.principal().is_active() {
        return PrincipalAuthorizationDecision::denied_from_binding(
            requested_scope,
            policy_version,
            binding,
            required_permission.clone(),
            PrincipalAuthorizationDenialReason::PrincipalDisabled,
            AuthorizationCheckState::not_evaluated(),
        );
    }

    if !requested_scope.permits_permission(required_permission) {
        return PrincipalAuthorizationDecision::denied_from_binding(
            requested_scope,
            policy_version,
            binding,
            required_permission.clone(),
            PrincipalAuthorizationDenialReason::SurfaceDoesNotPermitPermission,
            AuthorizationCheckState::surface_policy(false),
        );
    }

    let role_permission_granted = binding.role_grants(required_permission);
    let direct_permission_granted = binding.direct_grants(required_permission);
    if !(role_permission_granted || direct_permission_granted) {
        return PrincipalAuthorizationDecision::denied_from_binding(
            requested_scope,
            policy_version,
            binding,
            required_permission.clone(),
            PrincipalAuthorizationDenialReason::PrincipalMissingPermission,
            AuthorizationCheckState::permission_grants(
                role_permission_granted,
                direct_permission_granted,
            ),
        );
    }

    PrincipalAuthorizationDecision::allowed_from_binding(
        requested_scope,
        policy_version,
        binding,
        required_permission.clone(),
        AuthorizationCheckState::permission_grants(
            role_permission_granted,
            direct_permission_granted,
        ),
    )
}

/// Stable authorization outcome for core IAM decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalAuthorizationOutcome {
    Allowed,
    Denied,
}

impl PrincipalAuthorizationOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
        }
    }
}

/// Audit-ready evidence emitted by core IAM decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrincipalAuthorizationEvidence {
    pub policy_version: PrincipalPolicyVersion,
    pub surface_scope: SurfaceScope,
    pub certificate_surface_scope: Option<SurfaceScope>,
    pub certificate_fingerprint: String,
    pub certificate_subject: String,
    pub certificate_status: Option<CertificateIdentityStatus>,
    pub principal_id: Option<PrincipalId>,
    pub principal_status: Option<PrincipalStatus>,
    pub required_permission: Permission,
    pub outcome: PrincipalAuthorizationOutcome,
    pub reason: String,
    pub evaluation_stage: PrincipalAuthorizationEvaluationStage,
    pub surface_policy_evaluated: bool,
    pub surface_policy_allowed: bool,
    pub role_permission_evaluated: bool,
    pub role_permission_granted: bool,
    pub direct_permission_evaluated: bool,
    pub direct_permission_granted: bool,
}

impl PrincipalAuthorizationEvidence {
    pub fn has_identity_evidence(&self) -> bool {
        !self.certificate_fingerprint.trim().is_empty()
            && !self.certificate_subject.trim().is_empty()
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub fn has_policy_version(&self) -> bool {
        !self.policy_version.is_zero()
    }

    pub fn policy_evidence_binding(&self) -> AndromedaResult<PrincipalPolicyEvidenceBinding> {
        self.policy_version.evidence_binding()
    }

    pub fn matches_policy_evidence_binding(
        &self,
        binding: &PrincipalPolicyEvidenceBinding,
    ) -> bool {
        self.policy_evidence_binding()
            .is_ok_and(|expected| expected == *binding)
    }

    pub fn matches_policy_version_and_digest(
        &self,
        binding: &PrincipalPolicyEvidenceBinding,
    ) -> bool {
        self.matches_policy_evidence_binding(binding)
    }

    pub fn reason_code(&self) -> &str {
        self.reason.as_str()
    }

    pub const fn evaluation_stage(&self) -> PrincipalAuthorizationEvaluationStage {
        self.evaluation_stage
    }

    pub fn is_audit_ready(&self) -> bool {
        self.has_identity_evidence() && self.has_reason() && self.policy_evidence_binding().is_ok()
    }
}

/// Result of a core IAM authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrincipalAuthorizationDecision {
    pub outcome: PrincipalAuthorizationOutcome,
    pub principal_id: Option<PrincipalId>,
    pub required_permission: Permission,
    pub denial_reason: Option<PrincipalAuthorizationDenialReason>,
    pub evidence: PrincipalAuthorizationEvidence,
}

impl PrincipalAuthorizationDecision {
    fn allowed_from_binding(
        surface_scope: SurfaceScope,
        policy_version: PrincipalPolicyVersion,
        binding: &PrincipalBinding,
        required_permission: Permission,
        checks: AuthorizationCheckState,
    ) -> Self {
        let seed = AuthorizationDecisionSeed::from_binding(
            surface_scope,
            policy_version,
            binding,
            required_permission,
            checks,
        );
        Self {
            outcome: PrincipalAuthorizationOutcome::Allowed,
            principal_id: Some(binding.principal().id),
            required_permission: seed.required_permission.clone(),
            denial_reason: None,
            evidence: seed.evidence(
                PrincipalAuthorizationOutcome::Allowed,
                "allowed",
                PrincipalAuthorizationEvaluationStage::Allowed,
            ),
        }
    }

    fn denied_from_binding(
        surface_scope: SurfaceScope,
        policy_version: PrincipalPolicyVersion,
        binding: &PrincipalBinding,
        required_permission: Permission,
        reason: PrincipalAuthorizationDenialReason,
        checks: AuthorizationCheckState,
    ) -> Self {
        Self::denied(
            AuthorizationDecisionSeed::from_binding(
                surface_scope,
                policy_version,
                binding,
                required_permission,
                checks,
            ),
            reason,
        )
    }

    fn denied(seed: AuthorizationDecisionSeed, reason: PrincipalAuthorizationDenialReason) -> Self {
        Self {
            outcome: PrincipalAuthorizationOutcome::Denied,
            principal_id: seed.principal_id,
            required_permission: seed.required_permission.clone(),
            denial_reason: Some(reason),
            evidence: seed.evidence(
                PrincipalAuthorizationOutcome::Denied,
                reason.as_str(),
                reason.evaluation_stage(),
            ),
        }
    }

    pub const fn is_allowed(&self) -> bool {
        matches!(self.outcome, PrincipalAuthorizationOutcome::Allowed)
    }

    pub const fn is_denied(&self) -> bool {
        matches!(self.outcome, PrincipalAuthorizationOutcome::Denied)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthorizationDecisionSeed {
    policy_version: PrincipalPolicyVersion,
    surface_scope: SurfaceScope,
    certificate_surface_scope: Option<SurfaceScope>,
    certificate_fingerprint: String,
    certificate_subject: String,
    certificate_status: Option<CertificateIdentityStatus>,
    principal_id: Option<PrincipalId>,
    principal_status: Option<PrincipalStatus>,
    required_permission: Permission,
    checks: AuthorizationCheckState,
}

impl AuthorizationDecisionSeed {
    fn from_binding(
        surface_scope: SurfaceScope,
        policy_version: PrincipalPolicyVersion,
        binding: &PrincipalBinding,
        required_permission: Permission,
        checks: AuthorizationCheckState,
    ) -> Self {
        Self {
            policy_version,
            surface_scope,
            certificate_surface_scope: Some(binding.certificate().surface_scope()),
            certificate_fingerprint: binding.certificate().fingerprint().as_str().to_string(),
            certificate_subject: binding.certificate().subject().to_string(),
            certificate_status: Some(binding.certificate().status()),
            principal_id: Some(binding.principal().id),
            principal_status: Some(binding.principal().status),
            required_permission,
            checks,
        }
    }

    fn unknown(
        policy_version: PrincipalPolicyVersion,
        surface_scope: SurfaceScope,
        presented_fingerprint: &str,
        required_permission: Permission,
    ) -> Self {
        Self {
            policy_version,
            surface_scope,
            certificate_surface_scope: None,
            certificate_fingerprint: fingerprint_for_audit(presented_fingerprint),
            certificate_subject: "subject:unknown".to_string(),
            certificate_status: None,
            principal_id: None,
            principal_status: None,
            required_permission,
            checks: AuthorizationCheckState::not_evaluated(),
        }
    }

    fn evidence(
        &self,
        outcome: PrincipalAuthorizationOutcome,
        reason: &str,
        evaluation_stage: PrincipalAuthorizationEvaluationStage,
    ) -> PrincipalAuthorizationEvidence {
        PrincipalAuthorizationEvidence {
            policy_version: self.policy_version,
            surface_scope: self.surface_scope,
            certificate_surface_scope: self.certificate_surface_scope,
            certificate_fingerprint: self.certificate_fingerprint.clone(),
            certificate_subject: self.certificate_subject.clone(),
            certificate_status: self.certificate_status,
            principal_id: self.principal_id,
            principal_status: self.principal_status,
            required_permission: self.required_permission.clone(),
            outcome,
            reason: reason.to_string(),
            evaluation_stage,
            surface_policy_evaluated: self.checks.surface_policy_evaluated,
            surface_policy_allowed: self.checks.surface_policy_allowed,
            role_permission_evaluated: self.checks.role_permission_evaluated,
            role_permission_granted: self.checks.role_permission_granted,
            direct_permission_evaluated: self.checks.direct_permission_evaluated,
            direct_permission_granted: self.checks.direct_permission_granted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuthorizationCheckState {
    surface_policy_evaluated: bool,
    surface_policy_allowed: bool,
    role_permission_evaluated: bool,
    role_permission_granted: bool,
    direct_permission_evaluated: bool,
    direct_permission_granted: bool,
}

impl AuthorizationCheckState {
    const fn not_evaluated() -> Self {
        Self {
            surface_policy_evaluated: false,
            surface_policy_allowed: false,
            role_permission_evaluated: false,
            role_permission_granted: false,
            direct_permission_evaluated: false,
            direct_permission_granted: false,
        }
    }

    const fn surface_policy(surface_policy_allowed: bool) -> Self {
        Self {
            surface_policy_evaluated: true,
            surface_policy_allowed,
            role_permission_evaluated: false,
            role_permission_granted: false,
            direct_permission_evaluated: false,
            direct_permission_granted: false,
        }
    }

    const fn permission_grants(
        role_permission_granted: bool,
        direct_permission_granted: bool,
    ) -> Self {
        Self {
            surface_policy_evaluated: true,
            surface_policy_allowed: true,
            role_permission_evaluated: true,
            role_permission_granted,
            direct_permission_evaluated: true,
            direct_permission_granted,
        }
    }
}

fn fingerprint_for_audit(presented: &str) -> String {
    let trimmed = presented.trim();
    if trimmed.is_empty() {
        "fingerprint:empty".to_string()
    } else {
        format!(
            "fingerprint:{}",
            trimmed.chars().take(64).collect::<String>()
        )
    }
}
