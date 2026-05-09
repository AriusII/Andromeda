use andromeda_plan_cache::{
    AdvisoryEvidenceIdentity, AdvisoryEvidenceStatus, AdvisoryEvidenceSummary,
    AdvisoryEvidenceSummaryBuilder, PlanCacheKey, PlanSelectionError,
    classify_advisory_identity_for_key,
};
use andromeda_time::EngineTimestamp;

use crate::{ScenarioEvidence, ScenarioEvidenceAdvisoryUse, ScenarioEvidenceError};

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
        },
        Err(ScenarioEvidenceError::Expired) => return (AdvisoryEvidenceStatus::Expired, None),
        Err(_) => return (AdvisoryEvidenceStatus::AuthoritativeRejected, None),
    };
    let target = advisory.target();
    let status = classify_advisory_identity_for_key(
        key,
        AdvisoryEvidenceIdentity {
            procedure_id: target.procedure_id,
            catalog_version: target.catalog_version,
            stats_version: target.stats_version,
            contract_hash: target.contract_hash,
            plan_class: target.plan_class,
        },
        advisory.is_authoritative(),
        advisory.can_select_plan_alone(),
    );
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
            },
            (AdvisoryEvidenceStatus::AcceptedAdvisory, None) => {
                summary.record_rejected(AdvisoryEvidenceStatus::AuthoritativeRejected);
            },
            (status, _) => summary.record_rejected(status),
        }
    }

    Ok(summary.finish())
}
