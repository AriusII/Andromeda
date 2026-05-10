use andromeda_procedure_contract::PolicyVersion;

use crate::StatisticsError;

/// Policy switch for advisory statistics consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatisticsUsePolicy {
    enabled: bool,
    allow_stale: bool,
    policy_version: PolicyVersion,
}

impl StatisticsUsePolicy {
    pub const fn enabled(policy_version: PolicyVersion) -> Self {
        Self {
            enabled: true,
            allow_stale: false,
            policy_version,
        }
    }

    pub const fn disabled(policy_version: PolicyVersion) -> Self {
        Self {
            enabled: false,
            allow_stale: false,
            policy_version,
        }
    }

    pub const fn with_stale_allowed(mut self) -> Self {
        self.allow_stale = true;
        self
    }

    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    pub const fn allow_stale(self) -> bool {
        self.allow_stale
    }

    pub const fn policy_version(self) -> PolicyVersion {
        self.policy_version
    }

    pub fn validate(self) -> Result<(), StatisticsError> {
        if self.policy_version.is_zero() {
            return Err(StatisticsError::PolicyVersionZero);
        }
        Ok(())
    }
}
