use super::errors::BenchmarkScenarioEvidenceError;

const EVIDENCE_PERMILLE_MAX: u16 = 1_000;

/// Fixed-scale confidence value for benchmark-derived evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BenchmarkEvidenceConfidence(u16);

impl BenchmarkEvidenceConfidence {
    pub const MAX_RAW: u16 = EVIDENCE_PERMILLE_MAX;

    pub const fn from_permille(value: u16) -> Result<Self, BenchmarkScenarioEvidenceError> {
        if value > Self::MAX_RAW {
            Err(BenchmarkScenarioEvidenceError::ConfidenceOutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn permille(self) -> u16 {
        self.0
    }
}
