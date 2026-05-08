use andromeda_plan_cache::{
    AdvisoryEvidenceStatus, AdvisoryEvidenceSummary, AdvisoryEvidenceSummaryBuilder, PlanCacheKey,
    PlanSelectionError,
};
use andromeda_time::EngineTimestamp;

use crate::scenario_evidence::{
    ScenarioEvidence, ScenarioEvidenceAdvisoryUse, ScenarioEvidenceError,
};

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
/// `PlanClass`. Evidence carries no policy version, so a cache hit remains
/// governed by the full [`PlanCacheKey`] rather than by benchmark output.
pub fn classify_advisory_evidence_for_key(
    key: &PlanCacheKey,
    evidence: &ScenarioEvidence,
    now: EngineTimestamp,
) -> AdvisoryEvidenceStatus {
    classify_advisory_evidence_for_key_with_token(key, evidence, now).0
}

pub(super) fn summarize_advisory_evidence(
    key: &PlanCacheKey,
    evidence: &[ScenarioEvidence],
    now: EngineTimestamp,
) -> Result<AdvisoryEvidenceSummary, PlanSelectionError> {
    let mut summary = AdvisoryEvidenceSummaryBuilder::new(evidence.len())?;

    for item in evidence {
        let (status, advisory) = classify_advisory_evidence_for_key_with_token(key, item, now);
        match (status, advisory) {
            (AdvisoryEvidenceStatus::AcceptedAdvisory, Some(advisory)) => {
                summary.record_accepted_advisory(
                    advisory.score().permille(),
                    advisory.confidence().permille(),
                    advisory.digest(),
                );
            }
            (AdvisoryEvidenceStatus::AcceptedAdvisory, None) => {
                summary.record_rejected(AdvisoryEvidenceStatus::AuthoritativeRejected);
            }
            (status, _) => summary.record_rejected(status),
        }
    }

    Ok(summary.finish())
}
