//! Compatibility facade for bounded, versioned statistics publication.

pub use andromeda_statistics::{
    STATS_PUBLICATION_SWITCH_HISTORY_LIMIT, STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES,
    StatsPublication, StatsPublicationAdvisoryEvidenceReference, StatsPublicationBuilder,
    StatsPublicationCandidateState, StatsPublicationDecisionEvidence,
    StatsPublicationDecisionEvidenceKind, StatsPublicationDecisionStage,
    StatsPublicationDecisionTrace, StatsPublicationSummary, StatsPublicationSwitch,
    StatsPublicationSwitchDecision, StatsPublicationSwitchError,
};
