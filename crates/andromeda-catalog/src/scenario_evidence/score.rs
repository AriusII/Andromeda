use super::ScenarioEvidenceError;

/// Maximum admissible score/confidence value on the fixed permille scale.
const EVIDENCE_PERMILLE_MAX: u16 = 1_000;

const fn validate_evidence_permille(
    value: u16,
    error: ScenarioEvidenceError,
) -> Result<u16, ScenarioEvidenceError> {
    if value > EVIDENCE_PERMILLE_MAX {
        Err(error)
    } else {
        Ok(value)
    }
}

/// Bounded score on a fixed `0..=1000` integer scale ("permille").
///
/// The interpretation is intentionally left abstract at this layer: callers
/// decide whether higher means "faster", "preferred", or "cheaper". The
/// bounded integer representation guarantees byte-identical digests on every
/// node and forbids float drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EvidenceScore(u16);

impl EvidenceScore {
    /// Maximum admissible raw value. `1000` corresponds to "1.000".
    pub const MAX_RAW: u16 = EVIDENCE_PERMILLE_MAX;

    /// The zero score. Always valid.
    pub const ZERO: Self = Self(0);

    /// Construct from a raw permille value. Rejects anything strictly greater
    /// than [`Self::MAX_RAW`]. No clamping: out-of-range callers are bugs and
    /// must be reported as such.
    pub const fn from_permille(value: u16) -> Result<Self, ScenarioEvidenceError> {
        match validate_evidence_permille(value, ScenarioEvidenceError::ScoreOutOfRange) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    pub const fn permille(self) -> u16 {
        self.0
    }
}

/// Bounded confidence on a fixed `0..=1000` integer scale ("permille").
///
/// Independent of [`EvidenceScore`] so a high-magnitude signal with low
/// statistical confidence is representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EvidenceConfidence(u16);

impl EvidenceConfidence {
    pub const MAX_RAW: u16 = EVIDENCE_PERMILLE_MAX;

    pub const ZERO: Self = Self(0);

    pub const fn from_permille(value: u16) -> Result<Self, ScenarioEvidenceError> {
        match validate_evidence_permille(value, ScenarioEvidenceError::ConfidenceOutOfRange) {
            Ok(value) => Ok(Self(value)),
            Err(error) => Err(error),
        }
    }

    pub const fn permille(self) -> u16 {
        self.0
    }
}
