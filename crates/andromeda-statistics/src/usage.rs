use andromeda_decision_trace::{
    AdaptiveControl, AdaptiveFeature, DecisionFamily, DecisionOutcome, DecisionReasonCode,
    DecisionTrace, DecisionTraceError, DecisionTraceId, EvidenceDigest, EvidenceLabel,
    TraceEvidence, VersionBinding,
};

use crate::{StatisticsError, StatisticsUsePolicy, StatsObjectDescriptor, StatsPublicationState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatisticsUseReason {
    AcceptedPublished,
    AcceptedStaleByPolicy,
    DisabledByPolicy,
    MissingStatistics,
    CandidateNotPublished,
    ValidationInProgress,
    RejectedPublication,
    ExpiredPublication,
    SupersededPublication,
    StaleRejected,
}

impl StatisticsUseReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AcceptedPublished => "accepted-published",
            Self::AcceptedStaleByPolicy => "accepted-stale-by-policy",
            Self::DisabledByPolicy => "statistics-disabled-by-policy",
            Self::MissingStatistics => "missing-statistics",
            Self::CandidateNotPublished => "candidate-not-published",
            Self::ValidationInProgress => "validation-in-progress",
            Self::RejectedPublication => "rejected-publication",
            Self::ExpiredPublication => "expired-publication",
            Self::SupersededPublication => "superseded-publication",
            Self::StaleRejected => "stale-rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticsUseDecision {
    descriptor: Option<StatsObjectDescriptor>,
    accepted: bool,
    reason: StatisticsUseReason,
    trace: DecisionTrace,
}

impl StatisticsUseDecision {
    pub const fn descriptor(&self) -> Option<StatsObjectDescriptor> {
        self.descriptor
    }

    pub const fn accepted(&self) -> bool {
        self.accepted
    }

    pub const fn reason(&self) -> StatisticsUseReason {
        self.reason
    }

    pub const fn trace(&self) -> &DecisionTrace {
        &self.trace
    }
}

pub fn evaluate_statistics_for_optimizer(
    descriptor: Option<StatsObjectDescriptor>,
    policy: StatisticsUsePolicy,
    trace_id: DecisionTraceId,
) -> Result<StatisticsUseDecision, StatisticsError> {
    policy.validate()?;

    let reason = classify_reason(descriptor, policy);
    let accepted = matches!(
        reason,
        StatisticsUseReason::AcceptedPublished | StatisticsUseReason::AcceptedStaleByPolicy
    );
    let outcome = match reason {
        StatisticsUseReason::AcceptedPublished | StatisticsUseReason::AcceptedStaleByPolicy => {
            DecisionOutcome::Accepted
        }
        StatisticsUseReason::DisabledByPolicy => DecisionOutcome::Disabled,
        StatisticsUseReason::MissingStatistics | StatisticsUseReason::StaleRejected => {
            DecisionOutcome::Fallback
        }
        _ => DecisionOutcome::Rejected,
    };

    let binding = descriptor
        .map(|descriptor| {
            VersionBinding::for_statistics(
                descriptor.catalog_version(),
                descriptor.stats_version(),
                Some(policy.policy_version()),
            )
        })
        .unwrap_or_else(|| VersionBinding::empty().with_policy_version(policy.policy_version()));

    let mut trace = DecisionTrace::new_with_control(
        trace_id,
        DecisionFamily::StatisticsUse,
        outcome,
        DecisionReasonCode::new(reason.as_str()).map_err(map_trace_error)?,
        trace_explanation(reason, descriptor),
        binding,
        Some(control_for_policy(policy)),
    )
    .map_err(map_trace_error)?;

    if let Some(descriptor) = descriptor {
        trace = trace
            .with_evidence(TraceEvidence::new(
                EvidenceLabel::new("stats-set").map_err(map_trace_error)?,
                EvidenceDigest::new(descriptor.digest().as_bytes()).map_err(map_trace_error)?,
            ))
            .map_err(map_trace_error)?;
    }

    Ok(StatisticsUseDecision {
        descriptor,
        accepted,
        reason,
        trace,
    })
}

fn classify_reason(
    descriptor: Option<StatsObjectDescriptor>,
    policy: StatisticsUsePolicy,
) -> StatisticsUseReason {
    if !policy.is_enabled() {
        return StatisticsUseReason::DisabledByPolicy;
    }

    let Some(descriptor) = descriptor else {
        return StatisticsUseReason::MissingStatistics;
    };

    match descriptor.state() {
        StatsPublicationState::Candidate => StatisticsUseReason::CandidateNotPublished,
        StatsPublicationState::Validating => StatisticsUseReason::ValidationInProgress,
        StatsPublicationState::Rejected => StatisticsUseReason::RejectedPublication,
        StatsPublicationState::Expired => StatisticsUseReason::ExpiredPublication,
        StatsPublicationState::Superseded => StatisticsUseReason::SupersededPublication,
        StatsPublicationState::Published if descriptor.is_stale() && policy.allow_stale() => {
            StatisticsUseReason::AcceptedStaleByPolicy
        }
        StatsPublicationState::Published if descriptor.is_stale() => {
            StatisticsUseReason::StaleRejected
        }
        StatsPublicationState::Published => StatisticsUseReason::AcceptedPublished,
    }
}

fn control_for_policy(policy: StatisticsUsePolicy) -> AdaptiveControl {
    if policy.is_enabled() {
        AdaptiveControl::enabled(AdaptiveFeature::Statistics, policy.policy_version())
    } else {
        AdaptiveControl::disabled(AdaptiveFeature::Statistics, policy.policy_version())
    }
}

fn trace_explanation(
    reason: StatisticsUseReason,
    descriptor: Option<StatsObjectDescriptor>,
) -> String {
    match descriptor {
        Some(descriptor) => format!(
            "statistics-use reason={} catalog_version={} stats_version={} state={} stale={} advisory_only=true",
            reason.as_str(),
            descriptor.catalog_version().get(),
            descriptor.stats_version().get(),
            descriptor.state().as_str(),
            descriptor.is_stale(),
        ),
        None => format!(
            "statistics-use reason={} no_stats_version_bound=true advisory_only=true",
            reason.as_str()
        ),
    }
}

fn map_trace_error(error: DecisionTraceError) -> StatisticsError {
    match error {
        DecisionTraceError::TraceIdZero => StatisticsError::TraceIdZero,
        _ => StatisticsError::TraceBuildFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StatsSetDigest;
    use andromeda_contract::{PolicyVersion, StatsVersion};
    use andromeda_types::CatalogVersion;

    fn policy(byte: u8) -> PolicyVersion {
        PolicyVersion::new([byte; PolicyVersion::LEN])
    }

    fn descriptor(state: StatsPublicationState) -> StatsObjectDescriptor {
        StatsObjectDescriptor::new(
            CatalogVersion::new(2),
            StatsVersion::new(3),
            StatsSetDigest::new([4; 32]).unwrap(),
            state,
        )
        .unwrap()
    }

    #[test]
    fn published_statistics_are_accepted_with_versioned_trace() {
        let decision = evaluate_statistics_for_optimizer(
            Some(descriptor(StatsPublicationState::Published)),
            StatisticsUsePolicy::enabled(policy(1)),
            DecisionTraceId::new(9).unwrap(),
        )
        .unwrap();

        assert!(decision.accepted());
        assert_eq!(decision.reason(), StatisticsUseReason::AcceptedPublished);
        assert!(decision.trace().is_observable());
        assert!(decision.trace().is_versioned());
        assert_eq!(decision.trace().evidence().len(), 1);
    }

    #[test]
    fn disabled_statistics_do_not_accept_descriptor() {
        let decision = evaluate_statistics_for_optimizer(
            Some(descriptor(StatsPublicationState::Published)),
            StatisticsUsePolicy::disabled(policy(1)),
            DecisionTraceId::new(9).unwrap(),
        )
        .unwrap();

        assert!(!decision.accepted());
        assert_eq!(decision.reason(), StatisticsUseReason::DisabledByPolicy);
        assert!(decision.trace().is_disabled());
    }

    #[test]
    fn stale_statistics_require_explicit_policy() {
        let stale = descriptor(StatsPublicationState::Published).mark_stale();
        let rejected = evaluate_statistics_for_optimizer(
            Some(stale),
            StatisticsUsePolicy::enabled(policy(1)),
            DecisionTraceId::new(9).unwrap(),
        )
        .unwrap();
        assert_eq!(rejected.reason(), StatisticsUseReason::StaleRejected);
        assert!(!rejected.accepted());

        let accepted = evaluate_statistics_for_optimizer(
            Some(stale),
            StatisticsUsePolicy::enabled(policy(1)).with_stale_allowed(),
            DecisionTraceId::new(10).unwrap(),
        )
        .unwrap();
        assert_eq!(
            accepted.reason(),
            StatisticsUseReason::AcceptedStaleByPolicy
        );
        assert!(accepted.accepted());
    }
}
