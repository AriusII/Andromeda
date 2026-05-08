use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

use andromeda_decision_trace::digest_prefix_hex;

use super::advisory_evidence::advisory_status_counts_reason;
use super::{AdvisoryEvidenceSummary, PlanCacheKey, PlanCandidateId};

/// Closed outcome for a plan-cache or plan-selection decision trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanDecisionOutcome {
    Selected,
    CacheHit,
    CacheMiss,
    CacheInsert,
    CacheEvict,
}

impl PlanDecisionOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            PlanDecisionOutcome::Selected => "selected",
            PlanDecisionOutcome::CacheHit => "cache-hit",
            PlanDecisionOutcome::CacheMiss => "cache-miss",
            PlanDecisionOutcome::CacheInsert => "cache-insert",
            PlanDecisionOutcome::CacheEvict => "cache-evict",
        }
    }
}

/// Closed reason codes for the minimal PlanCache gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlanDecisionReasonCode {
    MinimalRankSelected,
    ExactKeyHit,
    ExactKeyMiss,
    Inserted,
    CapacityEvictedOldest,
}

/// Bounded explanation for an exact-key PlanCache miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanCacheMissReason {
    CacheEmpty,
    NoProcedureEntry,
    ContractHashMismatch,
    CatalogVersionMismatch,
    StatsVersionMismatch,
    PolicyVersionMismatch,
    PlanClassMismatch,
    ShapeFingerprintMismatch,
}

impl PlanCacheMissReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            PlanCacheMissReason::CacheEmpty => "cache-empty",
            PlanCacheMissReason::NoProcedureEntry => "no-procedure-entry",
            PlanCacheMissReason::ContractHashMismatch => "contract-hash-mismatch",
            PlanCacheMissReason::CatalogVersionMismatch => "catalog-version-mismatch",
            PlanCacheMissReason::StatsVersionMismatch => "stats-version-mismatch",
            PlanCacheMissReason::PolicyVersionMismatch => "policy-version-mismatch",
            PlanCacheMissReason::PlanClassMismatch => "plan-class-mismatch",
            PlanCacheMissReason::ShapeFingerprintMismatch => "shape-fingerprint-mismatch",
        }
    }
}

impl PlanDecisionReasonCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            PlanDecisionReasonCode::MinimalRankSelected => "minimal-rank-selected",
            PlanDecisionReasonCode::ExactKeyHit => "exact-key-hit",
            PlanDecisionReasonCode::ExactKeyMiss => "exact-key-miss",
            PlanDecisionReasonCode::Inserted => "inserted",
            PlanDecisionReasonCode::CapacityEvictedOldest => "capacity-evicted-oldest",
        }
    }
}

/// DecisionTrace-style evidence emitted by the minimal selector and cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDecisionEvidence {
    trace_id: TraceId,
    key: PlanCacheKey,
    key_digest: [u8; 32],
    outcome: PlanDecisionOutcome,
    reason_code: PlanDecisionReasonCode,
    selected_plan_id: Option<PlanCandidateId>,
    candidate_count: u8,
    matching_candidate_count: u8,
    advisory_evidence: AdvisoryEvidenceSummary,
    cache_miss_reason: Option<PlanCacheMissReason>,
}

impl PlanDecisionEvidence {
    #[allow(
        clippy::too_many_arguments,
        reason = "Trace evidence keeps every plan decision input explicit."
    )]
    pub(super) fn new(
        trace_id: TraceId,
        key: PlanCacheKey,
        outcome: PlanDecisionOutcome,
        reason_code: PlanDecisionReasonCode,
        selected_plan_id: Option<PlanCandidateId>,
        candidate_count: u8,
        matching_candidate_count: u8,
        advisory_evidence: AdvisoryEvidenceSummary,
    ) -> Self {
        Self {
            trace_id,
            key,
            key_digest: key.digest(),
            outcome,
            reason_code,
            selected_plan_id,
            candidate_count,
            matching_candidate_count,
            advisory_evidence,
            cache_miss_reason: None,
        }
    }

    pub(super) fn with_cache_miss_reason(mut self, reason: Option<PlanCacheMissReason>) -> Self {
        self.cache_miss_reason = reason;
        self
    }

    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    pub const fn key(&self) -> PlanCacheKey {
        self.key
    }

    pub const fn key_digest(&self) -> [u8; 32] {
        self.key_digest
    }

    pub const fn outcome(&self) -> PlanDecisionOutcome {
        self.outcome
    }

    pub const fn selected_plan_id(&self) -> Option<PlanCandidateId> {
        self.selected_plan_id
    }

    pub const fn candidate_count(&self) -> u8 {
        self.candidate_count
    }

    pub const fn matching_candidate_count(&self) -> u8 {
        self.matching_candidate_count
    }

    pub const fn advisory_evidence(&self) -> AdvisoryEvidenceSummary {
        self.advisory_evidence
    }

    pub const fn cache_miss_reason(&self) -> Option<PlanCacheMissReason> {
        self.cache_miss_reason
    }

    pub fn as_decision_trace(&self) -> DecisionTrace {
        DecisionTrace {
            trace_id: self.trace_id,
            decision: CriticalDecisionKind::PlanSelection,
            reason: self.reason(),
        }
    }

    fn reason(&self) -> String {
        let selected = self
            .selected_plan_id
            .map(|plan_id| plan_id.get().to_string())
            .unwrap_or_else(|| "none".to_string());
        let best_evidence_digest = self
            .advisory_evidence
            .best_evidence_digest
            .map(|digest| digest_prefix_hex(&digest))
            .unwrap_or_else(|| "none".to_string());
        let cache_miss_reason = self
            .cache_miss_reason
            .map(PlanCacheMissReason::as_str)
            .unwrap_or("none");

        format!(
            concat!(
                "plan-decision outcome={} reason={} key_digest={} ",
                "version_binding=ContractHash+CatalogVersion+StatsVersion ",
                "procedure_id={} contract_hash={} catalog_version={} stats_version={} policy_version={} ",
                "plan_class={:?} shape_digest={} selected_plan_id={} ",
                "candidates={} matching_candidates={} scenario_evidence_supplied={} ",
                "scenario_evidence_accepted_advisory={} scenario_evidence_rejected={} ",
                "scenario_evidence_statuses={} best_evidence_digest={} ",
                "cache_miss_reason={} advisory_only=true"
            ),
            self.outcome.as_str(),
            self.reason_code.as_str(),
            digest_prefix_hex(&self.key_digest),
            self.key.procedure_id.get(),
            digest_prefix_hex(&self.key.contract_hash.as_bytes()),
            self.key.catalog_version.get(),
            self.key.stats_version.get(),
            digest_prefix_hex(&self.key.policy_version.as_bytes()),
            self.key.plan_class,
            digest_prefix_hex(&self.key.shape_fingerprint.as_bytes()),
            selected,
            self.candidate_count,
            self.matching_candidate_count,
            self.advisory_evidence.supplied_count,
            self.advisory_evidence.accepted_count,
            self.advisory_evidence.rejected_count,
            advisory_status_counts_reason(&self.advisory_evidence),
            best_evidence_digest,
            cache_miss_reason,
        )
    }
}
