use super::registry::PrincipalPolicyVersion;
use andromeda_security_contract as security_contract;

impl PrincipalPolicyVersion {
    /// Projects the core principal policy version onto the public security contract version shape.
    pub const fn to_security_policy_version(self) -> security_contract::SecurityPolicyVersion {
        security_contract::SecurityPolicyVersion::new(self.as_bytes())
    }

    /// Projects the core principal policy version to validated security contract evidence.
    pub fn to_security_policy_evidence(
        self,
    ) -> Result<security_contract::SecurityPolicyEvidence, security_contract::SecurityContractError>
    {
        security_contract::SecurityPolicyEvidence::for_policy_version(
            self.to_security_policy_version(),
        )
    }
}
