use andromeda_core::{
    PRINCIPAL_POLICY_EVIDENCE_VERSION, PrincipalPolicyEvidenceBinding, PrincipalPolicyVersion,
};
use andromeda_error::AndromedaResult;

use super::{SurfaceScope, contains_sensitive_marker, non_empty_evidence, observe_error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateIdentity {
    pub fingerprint: String,
    pub subject: String,
    pub surface: SurfaceScope,
}

impl CertificateIdentity {
    pub fn new(
        fingerprint: impl Into<String>,
        subject: impl Into<String>,
        surface: SurfaceScope,
    ) -> AndromedaResult<Self> {
        Ok(Self {
            fingerprint: non_empty_evidence("certificate fingerprint", fingerprint)?,
            subject: non_empty_evidence("certificate subject", subject)?,
            surface,
        })
    }

    pub fn has_identity_evidence(&self) -> bool {
        !self.fingerprint.trim().is_empty() && !self.subject.trim().is_empty()
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        contains_sensitive_marker(&self.fingerprint) || contains_sensitive_marker(&self.subject)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserPrincipalKind {
    Human,
    Service,
    BreakGlass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPrincipal {
    pub principal_id: String,
    pub kind: UserPrincipalKind,
}

impl UserPrincipal {
    pub fn new(principal_id: impl Into<String>, kind: UserPrincipalKind) -> AndromedaResult<Self> {
        Ok(Self {
            principal_id: non_empty_evidence("principal id", principal_id)?,
            kind,
        })
    }

    pub fn has_identity_evidence(&self) -> bool {
        !self.principal_id.trim().is_empty()
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        contains_sensitive_marker(&self.principal_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecurityPolicyVersionEvidence {
    pub policy_version: u64,
    pub policy_digest: String,
}

impl SecurityPolicyVersionEvidence {
    pub fn new(policy_version: u64, policy_digest: impl Into<String>) -> AndromedaResult<Self> {
        let evidence = Self {
            policy_version,
            policy_digest: non_empty_evidence("security policy digest", policy_digest)?,
        };
        if !evidence.has_version_evidence() {
            return Err(observe_error(
                "security policy version evidence requires non-zero version and canonical sha256 digest",
            ));
        }
        Ok(evidence)
    }

    pub fn try_bootstrap_v0() -> AndromedaResult<Self> {
        Self::from_core_policy_binding(&PrincipalPolicyEvidenceBinding::current()?)
    }

    /// Compatibility helper for tests and static bootstrap fixtures.
    ///
    /// Request-handling audit paths must prefer [`Self::try_bootstrap_v0`] so
    /// policy-evidence regressions return typed observability errors.
    pub fn bootstrap_v0() -> Self {
        match Self::try_bootstrap_v0() {
            Ok(evidence) => evidence,
            Err(_) => Self {
                policy_version: PRINCIPAL_POLICY_EVIDENCE_VERSION,
                policy_digest: PrincipalPolicyVersion::current().sha256_digest(),
            },
        }
    }

    pub fn from_core_policy_binding(
        binding: &PrincipalPolicyEvidenceBinding,
    ) -> AndromedaResult<Self> {
        Self::new(
            binding.policy_version(),
            binding.policy_digest().to_string(),
        )
    }

    pub fn from_core_policy_version(
        principal_policy_version: PrincipalPolicyVersion,
    ) -> AndromedaResult<Self> {
        Self::from_core_policy_binding(&principal_policy_version.evidence_binding()?)
    }

    pub fn matches_core_policy_binding(&self, binding: &PrincipalPolicyEvidenceBinding) -> bool {
        self.matches_core_policy_version_and_digest(binding)
    }

    pub fn matches_core_policy_version_and_digest(
        &self,
        binding: &PrincipalPolicyEvidenceBinding,
    ) -> bool {
        binding.matches_version_and_digest(self.policy_version, &self.policy_digest)
            && self.has_version_evidence()
    }

    pub fn matches_core_policy_version(
        &self,
        principal_policy_version: PrincipalPolicyVersion,
    ) -> bool {
        principal_policy_version
            .evidence_binding()
            .is_ok_and(|binding| self.matches_core_policy_binding(&binding))
    }

    pub fn has_version_evidence(&self) -> bool {
        PrincipalPolicyEvidenceBinding::has_version_evidence_parts(
            self.policy_version,
            &self.policy_digest,
        ) && !contains_sensitive_marker(&self.policy_digest)
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        contains_sensitive_marker(&self.policy_digest)
    }
}
