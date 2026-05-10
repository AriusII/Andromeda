use andromeda_procedure_contract::StatsVersion;
use andromeda_scenario_evidence::ScenarioEvidenceError;

use crate::StatsValidationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatsPublicationSwitchError {
    TraceIdZero,
    DecisionEvidenceTraceIdZero,
    EmptyReason,
    ReasonTooLong,
    PredictiveEvidenceCannotDriveActiveStatsVersion,
    PredictiveEvidenceRequiresAdvisoryIdentity,
    AdvisoryEvidenceCannotDriveActiveStatsVersion,
    AdvisoryEvidenceDigestZero,
    AdvisoryEvidenceTargetRejected(ScenarioEvidenceError),
    AdvisoryEvidenceIdentityMismatch,
    AdvisoryEvidenceStatsVersionMismatch {
        expected: StatsVersion,
        actual: StatsVersion,
    },
    PendingCandidateExists,
    NoPendingCandidate,
    PendingCandidateNotValidated,
    CandidateVersionNotAdvanced {
        active: StatsVersion,
        candidate: StatsVersion,
    },
    NoPreviousPublication,
    ValidationRejected(StatsValidationError),
}

impl core::fmt::Display for StatsPublicationSwitchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TraceIdZero => {
                f.write_str("stats publication switch trace_id must be non-zero")
            },
            Self::DecisionEvidenceTraceIdZero => {
                f.write_str("stats publication decision evidence trace_id must be non-zero")
            },
            Self::EmptyReason => {
                f.write_str("stats publication switch reason must not be empty")
            },
            Self::ReasonTooLong => {
                f.write_str("stats publication switch reason exceeds the bounded byte limit")
            },
            Self::PredictiveEvidenceCannotDriveActiveStatsVersion => f.write_str(
                "predictive ScenarioEvidence cannot drive an active StatsVersion transition",
            ),
            Self::PredictiveEvidenceRequiresAdvisoryIdentity => f.write_str(
                "predictive ScenarioEvidence decision evidence requires explicit advisory identity",
            ),
            Self::AdvisoryEvidenceCannotDriveActiveStatsVersion => f.write_str(
                "advisory ScenarioEvidence cannot drive an active StatsVersion transition",
            ),
            Self::AdvisoryEvidenceDigestZero => {
                f.write_str("stats publication advisory evidence digest must be non-zero")
            },
            Self::AdvisoryEvidenceTargetRejected(err) => {
                write!(f, "stats publication advisory evidence target rejected: {err}")
            },
            Self::AdvisoryEvidenceIdentityMismatch => f.write_str(
                "stats publication advisory evidence identity does not match the validated advisory token",
            ),
            Self::AdvisoryEvidenceStatsVersionMismatch { expected, actual } => write!(
                f,
                "stats publication advisory evidence targets StatsVersion {}, expected {}",
                actual.get(),
                expected.get()
            ),
            Self::PendingCandidateExists => {
                f.write_str("stats publication switch already has a pending candidate")
            },
            Self::NoPendingCandidate => {
                f.write_str("stats publication switch has no pending candidate")
            },
            Self::PendingCandidateNotValidated => {
                f.write_str("stats publication candidate must be validated before publish")
            },
            Self::CandidateVersionNotAdvanced { active, candidate } => write!(
                f,
                "candidate StatsVersion {} must advance active StatsVersion {}",
                candidate.get(),
                active.get()
            ),
            Self::NoPreviousPublication => {
                f.write_str("stats publication switch has no previous active publication")
            },
            Self::ValidationRejected(err) => {
                write!(f, "stats publication candidate validation rejected: {err}")
            },
        }
    }
}

impl std::error::Error for StatsPublicationSwitchError {}
