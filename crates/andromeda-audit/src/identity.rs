use andromeda_error::AndromedaResult;
use andromeda_security_contract::{
    SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION, SecurityPolicyEvidence, SecurityPolicyVersion,
};
use std::fmt::Write;

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
        let policy_evidence = andromeda_core::PrincipalPolicyVersion::current()
            .to_security_policy_evidence()
            .map_err(|error| observe_error(error.to_string()))?;
        Self::from_security_policy_evidence(&policy_evidence)
    }

    /// Compatibility helper for tests and static bootstrap fixtures.
    ///
    /// Request-handling audit paths must prefer [`Self::try_bootstrap_v0`] so
    /// policy-evidence regressions return typed observability errors.
    pub fn bootstrap_v0() -> Self {
        match Self::try_bootstrap_v0() {
            Ok(evidence) => evidence,
            Err(_) => Self {
                policy_version: SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION,
                policy_digest: policy_version_to_digest(SecurityPolicyVersion::default()),
            },
        }
    }

    pub fn from_security_policy_evidence(
        evidence: &SecurityPolicyEvidence,
    ) -> AndromedaResult<Self> {
        Self::new(
            evidence.schema_version(),
            policy_version_to_digest(evidence.policy_version()),
        )
    }

    pub fn from_security_policy_version(
        policy_version: SecurityPolicyVersion,
    ) -> AndromedaResult<Self> {
        let policy_evidence = SecurityPolicyEvidence::for_policy_version(policy_version)
            .map_err(|error| observe_error(error.to_string()))?;
        Self::from_security_policy_evidence(&policy_evidence)
    }

    pub fn matches_security_policy_evidence(&self, evidence: &SecurityPolicyEvidence) -> bool {
        self.matches_security_policy_version_and_digest(
            evidence.schema_version(),
            &policy_version_to_digest(evidence.policy_version()),
        )
    }

    pub fn matches_security_policy_version_and_digest(
        &self,
        policy_version: u64,
        policy_digest: &str,
    ) -> bool {
        policy_version == self.policy_version
            && policy_digest == self.policy_digest
            && Self::has_version_evidence_parts(policy_version, policy_digest)
            && !contains_sensitive_marker(policy_digest)
    }

    pub fn matches_security_policy_version(&self, policy_version: SecurityPolicyVersion) -> bool {
        let policy_digest = policy_version_to_digest(policy_version);
        self.matches_security_policy_version_and_digest(
            SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION,
            &policy_digest,
        )
    }

    pub fn has_version_evidence(&self) -> bool {
        Self::has_version_evidence_parts(self.policy_version, &self.policy_digest)
            && !contains_sensitive_marker(&self.policy_digest)
    }

    fn has_version_evidence_parts(policy_version: u64, policy_digest: &str) -> bool {
        policy_version != 0 && is_canonical_sha256_digest(policy_digest)
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        contains_sensitive_marker(&self.policy_digest)
    }
}

fn policy_version_to_digest(policy_version: SecurityPolicyVersion) -> String {
    let mut digest = String::with_capacity(7 + SecurityPolicyVersion::LEN * 2);
    digest.push_str("sha256:");
    for byte in policy_version.as_bytes() {
        let _ = write!(&mut digest, "{byte:02x}");
    }
    digest
}

fn is_canonical_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    })
}
