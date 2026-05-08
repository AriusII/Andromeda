use super::{PLAN_SELECTION_MAX_SCENARIO_EVIDENCE, PlanSelectionError};

const ADVISORY_EVIDENCE_STATUS_COUNT: usize = 11;

/// Closed status for each advisory evidence record evaluated by a plan
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

/// Bounded summary of advisory evidence considered for a plan decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvisoryEvidenceSummary {
    pub(crate) supplied_count: u8,
    pub(crate) accepted_count: u8,
    pub(crate) rejected_count: u8,
    status_counts: [u8; ADVISORY_EVIDENCE_STATUS_COUNT],
    pub(crate) best_score_permille: Option<u16>,
    pub(crate) best_confidence_permille: Option<u16>,
    pub(crate) best_evidence_digest: Option<[u8; 32]>,
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

/// Builder used by catalog adapters to summarize their local evidence types
/// without making this crate depend on the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisoryEvidenceSummaryBuilder {
    summary: AdvisoryEvidenceSummary,
}

impl AdvisoryEvidenceSummaryBuilder {
    pub fn new(supplied_count: usize) -> Result<Self, PlanSelectionError> {
        if supplied_count > PLAN_SELECTION_MAX_SCENARIO_EVIDENCE {
            return Err(PlanSelectionError::TooManyScenarioEvidence);
        }

        Ok(Self {
            summary: AdvisoryEvidenceSummary {
                supplied_count: supplied_count as u8,
                ..AdvisoryEvidenceSummary::empty()
            },
        })
    }

    pub fn record_accepted_advisory(
        &mut self,
        score_permille: u16,
        confidence_permille: u16,
        digest: [u8; 32],
    ) {
        self.record_status(AdvisoryEvidenceStatus::AcceptedAdvisory);
        self.summary.accepted_count = self.summary.accepted_count.saturating_add(1);

        let replace_best = match (
            self.summary.best_score_permille,
            self.summary.best_confidence_permille,
            self.summary.best_evidence_digest,
        ) {
            (None, _, _) => true,
            (Some(best_score), Some(best_confidence), Some(best_digest)) => {
                (score_permille, confidence_permille, digest)
                    > (best_score, best_confidence, best_digest)
            }
            _ => true,
        };

        if replace_best {
            self.summary.best_score_permille = Some(score_permille);
            self.summary.best_confidence_permille = Some(confidence_permille);
            self.summary.best_evidence_digest = Some(digest);
        }
    }

    pub fn record_rejected(&mut self, status: AdvisoryEvidenceStatus) {
        self.record_status(status);
        self.summary.rejected_count = self.summary.rejected_count.saturating_add(1);
    }

    pub const fn finish(self) -> AdvisoryEvidenceSummary {
        self.summary
    }

    fn record_status(&mut self, status: AdvisoryEvidenceStatus) {
        self.summary.status_counts[status.as_index()] =
            self.summary.status_counts[status.as_index()].saturating_add(1);
    }
}

pub(crate) fn advisory_status_counts_reason(summary: &AdvisoryEvidenceSummary) -> String {
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
