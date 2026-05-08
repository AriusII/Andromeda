use andromeda_contract::PolicyVersion;

use crate::OptimizerError;

/// Policy switch for adaptive optimizer inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptimizerPolicy {
    adaptive_enabled: bool,
    policy_version: PolicyVersion,
}

impl OptimizerPolicy {
    pub const fn adaptive_enabled(policy_version: PolicyVersion) -> Self {
        Self {
            adaptive_enabled: true,
            policy_version,
        }
    }

    pub const fn adaptive_disabled(policy_version: PolicyVersion) -> Self {
        Self {
            adaptive_enabled: false,
            policy_version,
        }
    }

    pub const fn is_adaptive_enabled(self) -> bool {
        self.adaptive_enabled
    }

    pub const fn policy_version(self) -> PolicyVersion {
        self.policy_version
    }

    pub fn validate(self) -> Result<(), OptimizerError> {
        if self.policy_version.is_zero() {
            return Err(OptimizerError::PolicyVersionZero);
        }
        Ok(())
    }
}
