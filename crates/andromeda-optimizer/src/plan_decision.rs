use andromeda_decision_trace::{
    AdaptiveControl, AdaptiveFeature, DecisionFamily, DecisionOutcome, DecisionReasonCode,
    DecisionTrace, DecisionTraceError, DecisionTraceId, EvidenceDigest, EvidenceLabel,
    TraceEvidence,
};
use andromeda_plan_cache::PlanCacheKey;
use andromeda_statistics::StatsObjectDescriptor;

use crate::{OptimizerError, OptimizerPolicy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerPlanReason {
    AdaptiveDisabled,
    StatisticsAccepted,
    StatisticsMissingFallback,
    StatisticsVersionMismatch,
    StatisticsNotPublished,
    StatisticsStaleFallback,
    PolicyVersionMismatch,
}

impl OptimizerPlanReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AdaptiveDisabled => "optimizer-adaptive-disabled",
            Self::StatisticsAccepted => "statistics-accepted",
            Self::StatisticsMissingFallback => "statistics-missing-fallback",
            Self::StatisticsVersionMismatch => "statistics-version-mismatch",
            Self::StatisticsNotPublished => "statistics-not-published",
            Self::StatisticsStaleFallback => "statistics-stale-fallback",
            Self::PolicyVersionMismatch => "policy-version-mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizerPlanDecision {
    key: PlanCacheKey,
    statistics: Option<StatsObjectDescriptor>,
    reason: OptimizerPlanReason,
    uses_statistics: bool,
    bounded_fallback: bool,
    trace: DecisionTrace,
}

impl OptimizerPlanDecision {
    pub const fn key(&self) -> PlanCacheKey {
        self.key
    }

    pub const fn statistics(&self) -> Option<StatsObjectDescriptor> {
        self.statistics
    }

    pub const fn reason(&self) -> OptimizerPlanReason {
        self.reason
    }

    pub const fn uses_statistics(&self) -> bool {
        self.uses_statistics
    }

    pub const fn bounded_fallback(&self) -> bool {
        self.bounded_fallback
    }

    pub const fn trace(&self) -> &DecisionTrace {
        &self.trace
    }
}

pub fn evaluate_optimizer_plan_inputs(
    key: PlanCacheKey,
    statistics: Option<StatsObjectDescriptor>,
    policy: OptimizerPolicy,
    trace_id: DecisionTraceId,
) -> Result<OptimizerPlanDecision, OptimizerError> {
    policy.validate()?;

    let reason = classify_reason(key, statistics, policy);
    let uses_statistics = matches!(reason, OptimizerPlanReason::StatisticsAccepted);
    let bounded_fallback = !uses_statistics;
    let outcome = match reason {
        OptimizerPlanReason::StatisticsAccepted => DecisionOutcome::Accepted,
        OptimizerPlanReason::AdaptiveDisabled => DecisionOutcome::Disabled,
        OptimizerPlanReason::StatisticsMissingFallback
        | OptimizerPlanReason::StatisticsVersionMismatch
        | OptimizerPlanReason::StatisticsNotPublished
        | OptimizerPlanReason::StatisticsStaleFallback
        | OptimizerPlanReason::PolicyVersionMismatch => DecisionOutcome::Fallback,
    };
    let control = if policy.is_adaptive_enabled() {
        AdaptiveControl::enabled(AdaptiveFeature::Optimizer, policy.policy_version())
    } else {
        AdaptiveControl::disabled(AdaptiveFeature::Optimizer, policy.policy_version())
    };

    let mut trace = DecisionTrace::new_with_control(
        trace_id,
        DecisionFamily::OptimizerPlan,
        outcome,
        DecisionReasonCode::new(reason.as_str()).map_err(map_trace_error)?,
        trace_explanation(key, statistics, policy, reason),
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

    if let Some(statistics) = statistics {
        trace = trace
            .with_evidence(TraceEvidence::new(
                EvidenceLabel::new("stats-set").map_err(map_trace_error)?,
                EvidenceDigest::new(statistics.digest().as_bytes()).map_err(map_trace_error)?,
            ))
            .map_err(map_trace_error)?;
    }

    Ok(OptimizerPlanDecision {
        key,
        statistics,
        reason,
        uses_statistics,
        bounded_fallback,
        trace,
    })
}

fn classify_reason(
    key: PlanCacheKey,
    statistics: Option<StatsObjectDescriptor>,
    policy: OptimizerPolicy,
) -> OptimizerPlanReason {
    if policy.policy_version() != key.policy_version() {
        return OptimizerPlanReason::PolicyVersionMismatch;
    }
    if !policy.is_adaptive_enabled() {
        return OptimizerPlanReason::AdaptiveDisabled;
    }

    let Some(statistics) = statistics else {
        return OptimizerPlanReason::StatisticsMissingFallback;
    };

    if statistics.catalog_version() != key.catalog_version()
        || statistics.stats_version() != key.stats_version()
    {
        return OptimizerPlanReason::StatisticsVersionMismatch;
    }
    if !statistics.is_published() {
        return OptimizerPlanReason::StatisticsNotPublished;
    }
    if statistics.is_stale() {
        return OptimizerPlanReason::StatisticsStaleFallback;
    }
    OptimizerPlanReason::StatisticsAccepted
}

fn trace_explanation(
    key: PlanCacheKey,
    statistics: Option<StatsObjectDescriptor>,
    policy: OptimizerPolicy,
    reason: OptimizerPlanReason,
) -> String {
    let stats = statistics
        .map(|stats| {
            format!(
                "stats_catalog_version={} stats_version={} stats_state={} stats_stale={}",
                stats.catalog_version().get(),
                stats.stats_version().get(),
                stats.state().as_str(),
                stats.is_stale(),
            )
        })
        .unwrap_or_else(|| "stats=none".to_string());

    format!(
        "optimizer-plan reason={} adaptive_enabled={} procedure_id={} catalog_version={} stats_version={} policy_matches={} plan_class={:?} {} advisory_only=true bounded_fallback={}",
        reason.as_str(),
        policy.is_adaptive_enabled(),
        key.procedure_id().get(),
        key.catalog_version().get(),
        key.stats_version().get(),
        policy.policy_version() == key.policy_version(),
        key.plan_class(),
        stats,
        !matches!(reason, OptimizerPlanReason::StatisticsAccepted),
    )
}

fn map_trace_error(error: DecisionTraceError) -> OptimizerError {
    match error {
        DecisionTraceError::TraceIdZero => OptimizerError::TraceIdZero,
        _ => OptimizerError::TraceBuildFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_contract::{PolicyVersion, ProcedureContractBinding, StatsVersion};
    use andromeda_plan_cache::{PlanClass, PlanShapeFingerprint};
    use andromeda_statistics::{StatsPublicationState, StatsSetDigest};
    use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

    fn policy(byte: u8) -> PolicyVersion {
        PolicyVersion::new([byte; PolicyVersion::LEN])
    }

    fn key(stats_version: u64, policy_version: PolicyVersion) -> PlanCacheKey {
        PlanCacheKey::build(
            ProcedureContractBinding {
                procedure_id: ProcedureId::new(1),
                catalog_version: CatalogVersion::new(2),
                contract_hash: ContractHash::test_vector(3),
                stats_version: StatsVersion::new(stats_version),
                policy_version,
            },
            PlanClass::Singleton,
            PlanShapeFingerprint::empty(),
        )
        .unwrap()
    }

    fn stats(stats_version: u64, state: StatsPublicationState) -> StatsObjectDescriptor {
        StatsObjectDescriptor::new(
            CatalogVersion::new(2),
            StatsVersion::new(stats_version),
            StatsSetDigest::new([5; 32]).unwrap(),
            state,
        )
        .unwrap()
    }

    #[test]
    fn published_matching_stats_are_accepted_and_traced() {
        let policy = policy(8);
        let decision = evaluate_optimizer_plan_inputs(
            key(4, policy),
            Some(stats(4, StatsPublicationState::Published)),
            OptimizerPolicy::adaptive_enabled(policy),
            DecisionTraceId::new(11).unwrap(),
        )
        .unwrap();

        assert!(decision.uses_statistics());
        assert!(!decision.bounded_fallback());
        assert_eq!(decision.reason(), OptimizerPlanReason::StatisticsAccepted);
        assert!(decision.trace().is_observable());
        assert!(decision.trace().is_versioned());
    }

    #[test]
    fn disabled_adaptive_optimizer_uses_bounded_fallback() {
        let policy = policy(8);
        let decision = evaluate_optimizer_plan_inputs(
            key(4, policy),
            Some(stats(4, StatsPublicationState::Published)),
            OptimizerPolicy::adaptive_disabled(policy),
            DecisionTraceId::new(11).unwrap(),
        )
        .unwrap();

        assert!(!decision.uses_statistics());
        assert!(decision.bounded_fallback());
        assert_eq!(decision.reason(), OptimizerPlanReason::AdaptiveDisabled);
        assert!(decision.trace().is_disabled());
    }

    #[test]
    fn mismatched_stats_version_falls_back() {
        let policy = policy(8);
        let decision = evaluate_optimizer_plan_inputs(
            key(4, policy),
            Some(stats(5, StatsPublicationState::Published)),
            OptimizerPolicy::adaptive_enabled(policy),
            DecisionTraceId::new(11).unwrap(),
        )
        .unwrap();

        assert!(!decision.uses_statistics());
        assert!(decision.bounded_fallback());
        assert_eq!(
            decision.reason(),
            OptimizerPlanReason::StatisticsVersionMismatch
        );
    }
}
