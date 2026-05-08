/// Closed set of reasons a scenario-evidence record may be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioEvidenceError {
    /// Score raw value exceeded `EvidenceScore::MAX_RAW`.
    ScoreOutOfRange,
    /// Confidence raw value exceeded `EvidenceConfidence::MAX_RAW`.
    ConfidenceOutOfRange,
    /// Validity window had `expires_at == 0`.
    ExpiryMustBeNonZero,
    /// Validity window did not satisfy `issued_at < expires_at`.
    IssuedNotBeforeExpiry,
    /// Target carried a zero `ProcedureId`.
    TargetProcedureIdZero,
    /// Target carried a zero `CatalogVersion`.
    TargetCatalogVersionZero,
    /// Target carried a zero `StatsVersion`.
    TargetStatsVersionZero,
    /// Target carried an explicit but all-zero `ContractHash`.
    TargetContractHashZero,
    /// `validate_for_use_at` was called with `now < issued_at`.
    NotYetValid,
    /// `validate_for_use_at` was called with `now >= expires_at`.
    Expired,
}

impl core::fmt::Display for ScenarioEvidenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ScenarioEvidenceError::ScoreOutOfRange => {
                f.write_str("EvidenceScore raw value must be in 0..=1000")
            }
            ScenarioEvidenceError::ConfidenceOutOfRange => {
                f.write_str("EvidenceConfidence raw value must be in 0..=1000")
            }
            ScenarioEvidenceError::ExpiryMustBeNonZero => {
                f.write_str("ValidityWindow.expires_at must be non-zero")
            }
            ScenarioEvidenceError::IssuedNotBeforeExpiry => {
                f.write_str("ValidityWindow requires issued_at < expires_at")
            }
            ScenarioEvidenceError::TargetProcedureIdZero => {
                f.write_str("ScenarioTarget.procedure_id must be non-zero")
            }
            ScenarioEvidenceError::TargetCatalogVersionZero => {
                f.write_str("ScenarioTarget.catalog_version must be non-zero")
            }
            ScenarioEvidenceError::TargetStatsVersionZero => {
                f.write_str("ScenarioTarget.stats_version must be non-zero")
            }
            ScenarioEvidenceError::TargetContractHashZero => {
                f.write_str("ScenarioTarget.contract_hash, when present, must be non-zero")
            }
            ScenarioEvidenceError::NotYetValid => {
                f.write_str("ScenarioEvidence is not yet valid at the supplied clock reading")
            }
            ScenarioEvidenceError::Expired => {
                f.write_str("ScenarioEvidence has expired at the supplied clock reading")
            }
        }
    }
}

impl std::error::Error for ScenarioEvidenceError {}
