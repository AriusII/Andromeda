use crate::SecurityContractError;

pub const SECURITY_POLICY_VERSION_LEN: usize = 32;
pub const SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION: u64 = 1;

/// Version proof for the public security policy matrix.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecurityPolicyVersion([u8; SECURITY_POLICY_VERSION_LEN]);

impl SecurityPolicyVersion {
    pub const LEN: usize = SECURITY_POLICY_VERSION_LEN;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; Self::LEN])
    }

    pub const fn test_vector(byte: u8) -> Self {
        Self([byte; Self::LEN])
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

impl Default for SecurityPolicyVersion {
    fn default() -> Self {
        Self::zero()
    }
}

impl core::fmt::Debug for SecurityPolicyVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SecurityPolicyVersion(")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ")")
    }
}

/// Audit-ready evidence binding for the policy matrix used by an authorization decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecurityPolicyEvidence {
    schema_version: u64,
    policy_version: SecurityPolicyVersion,
}

impl SecurityPolicyEvidence {
    pub fn new(
        schema_version: u64,
        policy_version: SecurityPolicyVersion,
    ) -> Result<Self, SecurityContractError> {
        let evidence = Self {
            schema_version,
            policy_version,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn for_policy_version(
        policy_version: SecurityPolicyVersion,
    ) -> Result<Self, SecurityContractError> {
        Self::new(SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION, policy_version)
    }

    pub const fn schema_version(self) -> u64 {
        self.schema_version
    }

    pub const fn policy_version(self) -> SecurityPolicyVersion {
        self.policy_version
    }

    pub fn has_version_evidence(self) -> bool {
        self.schema_version != 0 && !self.policy_version.is_zero()
    }

    pub fn validate(self) -> Result<(), SecurityContractError> {
        if self.schema_version == 0 {
            return Err(SecurityContractError::InvalidEvidenceSchemaVersion);
        }

        if self.policy_version.is_zero() {
            return Err(SecurityContractError::MissingPolicyVersion);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_evidence_requires_non_zero_schema_version() {
        let version = SecurityPolicyVersion::test_vector(0x5a);

        assert_eq!(
            SecurityPolicyEvidence::new(0, version),
            Err(SecurityContractError::InvalidEvidenceSchemaVersion)
        );
    }

    #[test]
    fn test_policy_evidence_requires_non_zero_policy_version() {
        assert_eq!(
            SecurityPolicyEvidence::for_policy_version(SecurityPolicyVersion::zero()),
            Err(SecurityContractError::MissingPolicyVersion)
        );
    }

    #[test]
    fn test_policy_evidence_is_versioned_and_audit_ready() {
        let version = SecurityPolicyVersion::test_vector(0x6b);
        let evidence = SecurityPolicyEvidence::for_policy_version(version);

        assert_eq!(
            evidence,
            Ok(SecurityPolicyEvidence {
                schema_version: SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION,
                policy_version: version,
            })
        );
        assert!(evidence.is_ok_and(SecurityPolicyEvidence::has_version_evidence));
    }
}
