use crate::{contracts::StatsVersion, statistics::StatsValidationError};
use andromeda_scenario_evidence::ScenarioEvidenceError;

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
            StatsPublicationSwitchError::TraceIdZero => {
                f.write_str("stats publication switch trace_id must be non-zero")
            }
            StatsPublicationSwitchError::DecisionEvidenceTraceIdZero => {
                f.write_str("stats publication decision evidence trace_id must be non-zero")
            }
            StatsPublicationSwitchError::EmptyReason => {
                f.write_str("stats publication switch reason must not be empty")
            }
            StatsPublicationSwitchError::ReasonTooLong => {
                f.write_str("stats publication switch reason exceeds the bounded byte limit")
            }
            StatsPublicationSwitchError::PredictiveEvidenceCannotDriveActiveStatsVersion => f
                .write_str(
                    "predictive ScenarioEvidence cannot drive an active StatsVersion transition",
                ),
            StatsPublicationSwitchError::PredictiveEvidenceRequiresAdvisoryIdentity => f.write_str(
                "predictive ScenarioEvidence decision evidence requires explicit advisory identity",
            ),
            StatsPublicationSwitchError::AdvisoryEvidenceCannotDriveActiveStatsVersion => f
                .write_str(
                    "advisory ScenarioEvidence cannot drive an active StatsVersion transition",
                ),
            StatsPublicationSwitchError::AdvisoryEvidenceDigestZero => {
                f.write_str("stats publication advisory evidence digest must be non-zero")
            }
            StatsPublicationSwitchError::AdvisoryEvidenceTargetRejected(err) => {
                write!(f, "stats publication advisory evidence target rejected: {err}")
            }
            StatsPublicationSwitchError::AdvisoryEvidenceIdentityMismatch => f.write_str(
                "stats publication advisory evidence identity does not match the validated advisory token",
            ),
            StatsPublicationSwitchError::AdvisoryEvidenceStatsVersionMismatch {
                expected,
                actual,
            } => write!(
                f,
                "stats publication advisory evidence targets StatsVersion {}, expected {}",
                actual.get(),
                expected.get()
            ),
            StatsPublicationSwitchError::PendingCandidateExists => {
                f.write_str("stats publication switch already has a pending candidate")
            }
            StatsPublicationSwitchError::NoPendingCandidate => {
                f.write_str("stats publication switch has no pending candidate")
            }
            StatsPublicationSwitchError::PendingCandidateNotValidated => {
                f.write_str("stats publication candidate must be validated before publish")
            }
            StatsPublicationSwitchError::CandidateVersionNotAdvanced { active, candidate } => {
                write!(
                    f,
                    "candidate StatsVersion {} must advance active StatsVersion {}",
                    candidate.get(),
                    active.get()
                )
            }
            StatsPublicationSwitchError::NoPreviousPublication => {
                f.write_str("stats publication switch has no previous active publication")
            }
            StatsPublicationSwitchError::ValidationRejected(err) => {
                write!(f, "stats publication candidate validation rejected: {err}")
            }
        }
    }
}

impl std::error::Error for StatsPublicationSwitchError {}
