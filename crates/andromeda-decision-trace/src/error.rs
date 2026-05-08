/// Errors raised while building bounded runtime-free decision traces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionTraceError {
    TraceIdZero,
    ReasonCodeEmpty,
    ReasonCodeTooLong,
    ExplanationEmpty,
    ExplanationTooLong,
    EvidenceLabelEmpty,
    EvidenceLabelTooLong,
    EvidenceDigestZero,
    TooManyEvidenceReferences,
    ProcedureIdZero,
    CatalogVersionZero,
    ContractHashZero,
    StatsVersionZero,
    PolicyVersionZero,
}

impl core::fmt::Display for DecisionTraceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TraceIdZero => f.write_str("DecisionTrace requires a non-zero trace id"),
            Self::ReasonCodeEmpty => f.write_str("DecisionTrace reason code must not be empty"),
            Self::ReasonCodeTooLong => {
                f.write_str("DecisionTrace reason code exceeds the bounded limit")
            }
            Self::ExplanationEmpty => f.write_str("DecisionTrace explanation must not be empty"),
            Self::ExplanationTooLong => {
                f.write_str("DecisionTrace explanation exceeds the bounded limit")
            }
            Self::EvidenceLabelEmpty => {
                f.write_str("DecisionTrace evidence label must not be empty")
            }
            Self::EvidenceLabelTooLong => {
                f.write_str("DecisionTrace evidence label exceeds the bounded limit")
            }
            Self::EvidenceDigestZero => {
                f.write_str("DecisionTrace evidence digest must not be all zero")
            }
            Self::TooManyEvidenceReferences => {
                f.write_str("DecisionTrace evidence reference count exceeds the bounded limit")
            }
            Self::ProcedureIdZero => f.write_str("DecisionTrace procedure id must not be zero"),
            Self::CatalogVersionZero => {
                f.write_str("DecisionTrace catalog version must not be zero")
            }
            Self::ContractHashZero => f.write_str("DecisionTrace contract hash must not be zero"),
            Self::StatsVersionZero => f.write_str("DecisionTrace stats version must not be zero"),
            Self::PolicyVersionZero => f.write_str("DecisionTrace policy version must not be zero"),
        }
    }
}

impl std::error::Error for DecisionTraceError {}
