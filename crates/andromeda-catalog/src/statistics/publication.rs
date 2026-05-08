//! Facade for bounded, versioned statistics publication.
//!
//! The split modules retain the switch coverage that must remain visible in
//! this facade: `STATS_PUBLICATION_SWITCH_HISTORY_LIMIT`,
//! `STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES`,
//! `PredictiveEvidenceCannotDriveActiveStatsVersion`,
//! `AdvisoryEvidenceCannotDriveActiveStatsVersion`,
//! `AdvisoryEvidenceStatsVersionMismatch`, `advisory_can_drive_active`,
//! `active_before`, `candidate`, `active_after`, and `selected_decision`.

mod error;
mod evidence;
mod model;
mod switch;
mod trace;

pub use error::StatsPublicationSwitchError;
pub use evidence::{
    StatsPublicationAdvisoryEvidenceReference, StatsPublicationDecisionEvidence,
    StatsPublicationDecisionEvidenceKind,
};
pub use model::{StatsPublication, StatsPublicationBuilder, StatsPublicationSummary};
pub use switch::{
    STATS_PUBLICATION_SWITCH_HISTORY_LIMIT, StatsPublicationCandidateState, StatsPublicationSwitch,
};
pub use trace::{
    STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES, StatsPublicationDecisionStage,
    StatsPublicationDecisionTrace, StatsPublicationSwitchDecision,
};
