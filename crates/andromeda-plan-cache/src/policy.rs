use andromeda_decision_trace::{
    AdaptiveControl, AdaptiveFeature, DecisionFamily, DecisionOutcome, DecisionReasonCode,
    DecisionTrace, DecisionTraceError, DecisionTraceId, EvidenceDigest, EvidenceLabel,
    TraceEvidence,
};
use andromeda_procedure_contract::PolicyVersion;

use crate::{PlanCacheKey, PlanCachePolicyError, limits::PLAN_CACHE_MAX_ENTRIES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanCachePolicy {
    enabled: bool,
    capacity: usize,
    policy_version: PolicyVersion,
}

impl PlanCachePolicy {
    pub fn enabled(
        capacity: usize,
        policy_version: PolicyVersion,
    ) -> Result<Self, PlanCachePolicyError> {
        let policy = Self {
            enabled: true,
            capacity,
            policy_version,
        };
        policy.validate()?;
        Ok(policy)
    }

    pub const fn disabled(policy_version: PolicyVersion) -> Self {
        Self {
            enabled: false,
            capacity: 0,
            policy_version,
        }
    }

    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    pub const fn capacity(self) -> usize {
        self.capacity
    }

    pub const fn policy_version(self) -> PolicyVersion {
        self.policy_version
    }

    pub fn validate(self) -> Result<(), PlanCachePolicyError> {
        if self.policy_version.is_zero() {
            return Err(PlanCachePolicyError::PolicyVersionZero);
        }
        if self.enabled && self.capacity == 0 {
            return Err(PlanCachePolicyError::CapacityZero);
        }
        if self.enabled && self.capacity > PLAN_CACHE_MAX_ENTRIES {
            return Err(PlanCachePolicyError::CapacityTooLarge);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanCacheReuseReason {
    ReuseAllowed,
    DisabledByPolicy,
}

impl PlanCacheReuseReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReuseAllowed => "plan-cache-reuse-allowed",
            Self::DisabledByPolicy => "plan-cache-disabled-by-policy",
        }
    }
}

// Note: `Eq` is intentionally absent — `DecisionTrace` contains
// `PlanAlternativeCost` with `f64` fields, which satisfy only `PartialEq`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanCacheReuseDecision {
    key: PlanCacheKey,
    may_reuse: bool,
    reason: PlanCacheReuseReason,
    trace: DecisionTrace,
}

impl PlanCacheReuseDecision {
    pub const fn key(&self) -> PlanCacheKey {
        self.key
    }

    pub const fn may_reuse(&self) -> bool {
        self.may_reuse
    }

    pub const fn reason(&self) -> PlanCacheReuseReason {
        self.reason
    }

    pub const fn trace(&self) -> &DecisionTrace {
        &self.trace
    }
}

pub fn evaluate_plan_cache_reuse(
    key: PlanCacheKey,
    policy: PlanCachePolicy,
    trace_id: DecisionTraceId,
) -> Result<PlanCacheReuseDecision, PlanCachePolicyError> {
    policy.validate()?;

    let reason = if policy.is_enabled() {
        PlanCacheReuseReason::ReuseAllowed
    } else {
        PlanCacheReuseReason::DisabledByPolicy
    };
    let may_reuse = policy.is_enabled();
    let outcome = if may_reuse {
        DecisionOutcome::Accepted
    } else {
        DecisionOutcome::Disabled
    };
    let control = if may_reuse {
        AdaptiveControl::enabled(AdaptiveFeature::PlanCache, policy.policy_version())
    } else {
        AdaptiveControl::disabled(AdaptiveFeature::PlanCache, policy.policy_version())
    };

    let mut trace = DecisionTrace::new_with_control(
        trace_id,
        DecisionFamily::PlanCache,
        outcome,
        DecisionReasonCode::new(reason.as_str()).map_err(map_trace_error)?,
        trace_explanation(key, policy, reason),
        key.version_binding(),
        Some(control),
    )
    .map_err(map_trace_error)?;

    trace = trace
        .with_evidence(TraceEvidence::new(
            EvidenceLabel::new("plan-cache-key").map_err(map_trace_error)?,
            EvidenceDigest::new(key.digest()).map_err(map_trace_error)?,
        ))
        .map_err(map_trace_error)?;

    Ok(PlanCacheReuseDecision {
        key,
        may_reuse,
        reason,
        trace,
    })
}

fn trace_explanation(
    key: PlanCacheKey,
    policy: PlanCachePolicy,
    reason: PlanCacheReuseReason,
) -> String {
    format!(
        "plan-cache-policy reason={} enabled={} capacity={} procedure_id={} catalog_version={} stats_version={} plan_class={:?} schema_version={} advisory_only=false",
        reason.as_str(),
        policy.is_enabled(),
        policy.capacity(),
        key.procedure_id().get(),
        key.catalog_version().get(),
        key.stats_version().get(),
        key.plan_class(),
        crate::PLAN_CACHE_KEY_SCHEMA_VERSION,
    )
}

fn map_trace_error(error: DecisionTraceError) -> PlanCachePolicyError {
    match error {
        DecisionTraceError::TraceIdZero => PlanCachePolicyError::TraceIdZero,
        _ => PlanCachePolicyError::TraceBuildFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_procedure_contract::{PolicyVersion, ProcedureContractBinding, StatsVersion};
    use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

    fn policy(byte: u8) -> PolicyVersion {
        PolicyVersion::new([byte; PolicyVersion::LEN])
    }

    fn binding(stats_version: u64) -> ProcedureContractBinding {
        ProcedureContractBinding {
            procedure_id: ProcedureId::new(1),
            catalog_version: CatalogVersion::new(2),
            contract_hash: ContractHash::test_vector(3),
            stats_version: StatsVersion::new(stats_version),
            policy_version: policy(4),
        }
    }

    fn key() -> PlanCacheKey {
        PlanCacheKey::build(
            binding(3),
            crate::PlanClass::Singleton,
            crate::PlanShapeFingerprint::empty(),
        )
        .unwrap()
    }

    #[test]
    fn key_rejects_unversioned_stats() {
        assert_eq!(
            PlanCacheKey::build(
                binding(0),
                crate::PlanClass::Singleton,
                crate::PlanShapeFingerprint::empty(),
            ),
            Err(crate::PlanCacheKeyError::StatsVersionZero)
        );
    }

    #[test]
    fn disabled_policy_emits_disabled_trace() {
        let decision = evaluate_plan_cache_reuse(
            key(),
            PlanCachePolicy::disabled(policy(9)),
            DecisionTraceId::new(7).unwrap(),
        )
        .unwrap();

        assert!(!decision.may_reuse());
        assert_eq!(decision.reason(), PlanCacheReuseReason::DisabledByPolicy);
        assert!(decision.trace().is_observable());
        assert!(decision.trace().is_versioned());
        assert!(decision.trace().is_disabled());
    }

    #[test]
    fn enabled_policy_requires_bounded_capacity() {
        assert_eq!(
            PlanCachePolicy::enabled(0, policy(1)),
            Err(PlanCachePolicyError::CapacityZero)
        );
        assert_eq!(
            PlanCachePolicy::enabled(PLAN_CACHE_MAX_ENTRIES + 1, policy(1)),
            Err(PlanCachePolicyError::CapacityTooLarge)
        );
    }
}
