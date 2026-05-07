use andromeda_time::EngineTimestamp;

use crate::scenario_evidence::{
    ScenarioEvidence, ScenarioEvidenceAdvisoryUse, ScenarioEvidenceError,
};

use super::{PLAN_SELECTION_MAX_SCENARIO_EVIDENCE, PlanCacheKey, PlanSelectionError};

const ADVISORY_EVIDENCE_STATUS_COUNT: usize = 11;

/// Closed status for each ScenarioEvidence record evaluated by a plan
/// selection decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvisoryEvidenceStatus {
    /// Evidence matches the key and is fresh, but remains advisory-only.
    AcceptedAdvisory,
    /// Evidence unexpectedly reported itself as authoritative.
    AuthoritativeRejected,
    /// Evidence was issued in the future relative to the supplied clock.
    NotYetValid,
    /// Evidence expired before the supplied clock.
    Expired,
    /// Evidence targets a different Procedure.
    ProcedureIdMismatch,
    /// Evidence targets a different catalog publication.
    CatalogVersionMismatch,
    /// Evidence targets a different statistics publication.
    StatsVersionMismatch,
    /// Evidence does not bind the ContractHash, so it cannot be consumed for
    /// a contract-keyed plan decision.
    ContractHashMissing,
    /// Evidence binds a different ContractHash.
    ContractHashMismatch,
    /// Evidence does not bind the PlanClass.
    PlanClassMissing,
    /// Evidence binds a different PlanClass.
    PlanClassMismatch,
}

impl AdvisoryEvidenceStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            AdvisoryEvidenceStatus::AcceptedAdvisory => "accepted-advisory",
            AdvisoryEvidenceStatus::AuthoritativeRejected => "authoritative-rejected",
            AdvisoryEvidenceStatus::NotYetValid => "not-yet-valid",
            AdvisoryEvidenceStatus::Expired => "expired",
            AdvisoryEvidenceStatus::ProcedureIdMismatch => "procedure-id-mismatch",
            AdvisoryEvidenceStatus::CatalogVersionMismatch => "catalog-version-mismatch",
            AdvisoryEvidenceStatus::StatsVersionMismatch => "stats-version-mismatch",
            AdvisoryEvidenceStatus::ContractHashMissing => "contract-hash-missing",
            AdvisoryEvidenceStatus::ContractHashMismatch => "contract-hash-mismatch",
            AdvisoryEvidenceStatus::PlanClassMissing => "plan-class-missing",
            AdvisoryEvidenceStatus::PlanClassMismatch => "plan-class-mismatch",
        }
    }

    pub const fn as_index(self) -> usize {
        match self {
            AdvisoryEvidenceStatus::AcceptedAdvisory => 0,
            AdvisoryEvidenceStatus::AuthoritativeRejected => 1,
            AdvisoryEvidenceStatus::NotYetValid => 2,
            AdvisoryEvidenceStatus::Expired => 3,
            AdvisoryEvidenceStatus::ProcedureIdMismatch => 4,
            AdvisoryEvidenceStatus::CatalogVersionMismatch => 5,
            AdvisoryEvidenceStatus::StatsVersionMismatch => 6,
            AdvisoryEvidenceStatus::ContractHashMissing => 7,
            AdvisoryEvidenceStatus::ContractHashMismatch => 8,
            AdvisoryEvidenceStatus::PlanClassMissing => 9,
            AdvisoryEvidenceStatus::PlanClassMismatch => 10,
        }
    }
}

const ADVISORY_EVIDENCE_STATUSES: [AdvisoryEvidenceStatus; ADVISORY_EVIDENCE_STATUS_COUNT] = [
    AdvisoryEvidenceStatus::AcceptedAdvisory,
    AdvisoryEvidenceStatus::AuthoritativeRejected,
    AdvisoryEvidenceStatus::NotYetValid,
    AdvisoryEvidenceStatus::Expired,
    AdvisoryEvidenceStatus::ProcedureIdMismatch,
    AdvisoryEvidenceStatus::CatalogVersionMismatch,
    AdvisoryEvidenceStatus::StatsVersionMismatch,
    AdvisoryEvidenceStatus::ContractHashMissing,
    AdvisoryEvidenceStatus::ContractHashMismatch,
    AdvisoryEvidenceStatus::PlanClassMissing,
    AdvisoryEvidenceStatus::PlanClassMismatch,
];

fn classify_validated_advisory_evidence_for_key(
    key: &PlanCacheKey,
    evidence: &ScenarioEvidenceAdvisoryUse,
) -> AdvisoryEvidenceStatus {
    if evidence.is_authoritative() || evidence.can_select_plan_alone() {
        return AdvisoryEvidenceStatus::AuthoritativeRejected;
    }

    let target = evidence.target();
    if target.procedure_id != key.procedure_id {
        return AdvisoryEvidenceStatus::ProcedureIdMismatch;
    }
    if target.catalog_version != key.catalog_version {
        return AdvisoryEvidenceStatus::CatalogVersionMismatch;
    }
    if target.stats_version != key.stats_version {
        return AdvisoryEvidenceStatus::StatsVersionMismatch;
    }
    match target.contract_hash {
        Some(hash) if hash == key.contract_hash => {}
        Some(_) => return AdvisoryEvidenceStatus::ContractHashMismatch,
        None => return AdvisoryEvidenceStatus::ContractHashMissing,
    }
    match target.plan_class {
        Some(plan_class) if plan_class == key.plan_class => {}
        Some(_) => return AdvisoryEvidenceStatus::PlanClassMismatch,
        None => return AdvisoryEvidenceStatus::PlanClassMissing,
    }
    AdvisoryEvidenceStatus::AcceptedAdvisory
}

fn classify_advisory_evidence_for_key_with_token(
    key: &PlanCacheKey,
    evidence: &ScenarioEvidence,
    now: EngineTimestamp,
) -> (AdvisoryEvidenceStatus, Option<ScenarioEvidenceAdvisoryUse>) {
    if evidence.is_authoritative() {
        return (AdvisoryEvidenceStatus::AuthoritativeRejected, None);
    }
    let advisory = match evidence.advisory_use_at(now) {
        Ok(advisory) => advisory,
        Err(ScenarioEvidenceError::NotYetValid) => {
            return (AdvisoryEvidenceStatus::NotYetValid, None);
        }
        Err(ScenarioEvidenceError::Expired) => return (AdvisoryEvidenceStatus::Expired, None),
        Err(_) => return (AdvisoryEvidenceStatus::AuthoritativeRejected, None),
    };
    let status = classify_validated_advisory_evidence_for_key(key, &advisory);
    let advisory = match status {
        AdvisoryEvidenceStatus::AcceptedAdvisory => Some(advisory),
        _ => None,
    };
    (status, advisory)
}

/// Classify one ScenarioEvidence record against a complete plan-cache key.
///
/// This function deliberately requires evidence to match the key's
/// `ProcedureId`, `CatalogVersion`, `StatsVersion`, `ContractHash`, and
/// `PlanClass`.  Evidence carries no policy version, so a cache hit remains
/// governed by the full [`PlanCacheKey`] rather than by benchmark output.
pub fn classify_advisory_evidence_for_key(
    key: &PlanCacheKey,
    evidence: &ScenarioEvidence,
    now: EngineTimestamp,
) -> AdvisoryEvidenceStatus {
    classify_advisory_evidence_for_key_with_token(key, evidence, now).0
}

/// Bounded summary of advisory evidence considered for a plan decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvisoryEvidenceSummary {
    pub(super) supplied_count: u8,
    pub(super) accepted_count: u8,
    pub(super) rejected_count: u8,
    status_counts: [u8; ADVISORY_EVIDENCE_STATUS_COUNT],
    pub(super) best_score_permille: Option<u16>,
    pub(super) best_confidence_permille: Option<u16>,
    pub(super) best_evidence_digest: Option<[u8; 32]>,
}

impl AdvisoryEvidenceSummary {
    pub const fn empty() -> Self {
        Self {
            supplied_count: 0,
            accepted_count: 0,
            rejected_count: 0,
            status_counts: [0; ADVISORY_EVIDENCE_STATUS_COUNT],
            best_score_permille: None,
            best_confidence_permille: None,
            best_evidence_digest: None,
        }
    }

    pub const fn has_accepted_advisory_evidence(self) -> bool {
        self.accepted_count > 0
    }

    pub const fn supplied_count(self) -> u8 {
        self.supplied_count
    }

    pub const fn accepted_count(self) -> u8 {
        self.accepted_count
    }

    pub const fn rejected_count(self) -> u8 {
        self.rejected_count
    }

    pub const fn best_score_permille(self) -> Option<u16> {
        self.best_score_permille
    }

    pub const fn best_confidence_permille(self) -> Option<u16> {
        self.best_confidence_permille
    }

    pub const fn best_evidence_digest(self) -> Option<[u8; 32]> {
        self.best_evidence_digest
    }

    pub fn status_count(&self, status: AdvisoryEvidenceStatus) -> u8 {
        self.status_counts[status.as_index()]
    }
}

pub(super) fn summarize_advisory_evidence(
    key: &PlanCacheKey,
    evidence: &[ScenarioEvidence],
    now: EngineTimestamp,
) -> Result<AdvisoryEvidenceSummary, PlanSelectionError> {
    if evidence.len() > PLAN_SELECTION_MAX_SCENARIO_EVIDENCE {
        return Err(PlanSelectionError::TooManyScenarioEvidence);
    }

    let mut summary = AdvisoryEvidenceSummary {
        supplied_count: evidence.len() as u8,
        ..AdvisoryEvidenceSummary::empty()
    };

    for item in evidence {
        let (status, advisory) = classify_advisory_evidence_for_key_with_token(key, item, now);
        summary.status_counts[status.as_index()] =
            summary.status_counts[status.as_index()].saturating_add(1);

        match status {
            AdvisoryEvidenceStatus::AcceptedAdvisory => {
                let Some(advisory) = advisory else {
                    summary.rejected_count = summary.rejected_count.saturating_add(1);
                    continue;
                };
                summary.accepted_count = summary.accepted_count.saturating_add(1);
                let score = advisory.score().permille();
                let confidence = advisory.confidence().permille();
                let digest = advisory.digest();

                let replace_best = match (
                    summary.best_score_permille,
                    summary.best_confidence_permille,
                    summary.best_evidence_digest,
                ) {
                    (None, _, _) => true,
                    (Some(best_score), Some(best_confidence), Some(best_digest)) => {
                        (score, confidence, digest) > (best_score, best_confidence, best_digest)
                    }
                    _ => true,
                };

                if replace_best {
                    summary.best_score_permille = Some(score);
                    summary.best_confidence_permille = Some(confidence);
                    summary.best_evidence_digest = Some(digest);
                }
            }
            _ => {
                summary.rejected_count = summary.rejected_count.saturating_add(1);
            }
        }
    }

    Ok(summary)
}

pub(super) fn advisory_status_counts_reason(summary: &AdvisoryEvidenceSummary) -> String {
    if summary.supplied_count == 0 {
        return "none".to_string();
    }

    let mut out = String::new();
    for status in ADVISORY_EVIDENCE_STATUSES {
        let count = summary.status_count(status);
        if count == 0 {
            continue;
        }
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(status.as_str());
        out.push(':');
        out.push_str(&count.to_string());
    }
    out
}
