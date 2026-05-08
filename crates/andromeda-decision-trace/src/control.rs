use andromeda_contract::PolicyVersion;

/// Closed adaptive feature set that may be enabled or disabled by policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptiveFeature {
    Optimizer,
    Statistics,
    PlanCache,
    ScenarioEvidence,
    GpuAdvisory,
}

impl AdaptiveFeature {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Optimizer => "optimizer",
            Self::Statistics => "statistics",
            Self::PlanCache => "plan-cache",
            Self::ScenarioEvidence => "scenario-evidence",
            Self::GpuAdvisory => "gpu-advisory",
        }
    }
}

/// Runtime-free control evidence for an adaptive decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveControl {
    feature: AdaptiveFeature,
    enabled: bool,
    policy_version: PolicyVersion,
}

impl AdaptiveControl {
    pub const fn enabled(feature: AdaptiveFeature, policy_version: PolicyVersion) -> Self {
        Self {
            feature,
            enabled: true,
            policy_version,
        }
    }

    pub const fn disabled(feature: AdaptiveFeature, policy_version: PolicyVersion) -> Self {
        Self {
            feature,
            enabled: false,
            policy_version,
        }
    }

    pub const fn feature(self) -> AdaptiveFeature {
        self.feature
    }

    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    pub const fn is_disabled(self) -> bool {
        !self.enabled
    }

    pub const fn policy_version(self) -> PolicyVersion {
        self.policy_version
    }
}
