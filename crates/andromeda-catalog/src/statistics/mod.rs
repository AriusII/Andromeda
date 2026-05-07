mod builders;
mod correlation;
mod correlation_publication;
mod digest;
mod engine;
mod feedback;
mod histogram;
mod ndv;
mod publication;
mod validation;

#[cfg(test)]
mod tests_support;

pub use builders::{
    Datum, EquiDepthHistogramBuilder, EquiWidthHistogramBuilder, HistogramBuilderTrait,
};
pub use correlation::{
    CorrelationEvidenceBounds, CorrelationStrengthPermille, MAX_COLUMNS_PER_CORRELATION,
    StatsCorrelation, StatsCorrelationId, StatsCorrelationKind,
};
pub use correlation_publication::{
    MAX_CORRELATIONS_PER_PUBLICATION, StatsCorrelationPublication,
    StatsCorrelationPublicationBuilder,
};
pub(crate) use digest::{
    STATS_CORRELATION_DOMAIN, STATS_CORRELATION_PUBLICATION_DOMAIN, STATS_PUBLICATION_DOMAIN,
};
pub use digest::{StatsCorrelationDigest, StatsPublicationDigest};
pub use engine::{
    AdaptiveHistogram, ColumnStatistics, DEFAULT_BUCKET_COUNT, EquiDepthHistogram,
    EquiWidthHistogram, Histogram, HistogramAlgorithm, STATS_FULL_SCAN_THRESHOLD,
    STATS_INVALIDATION_MUTATION_PCT, STATS_LSN_DELTA_THRESHOLD, STATS_MAX_AGE_HOURS,
    STATS_SAMPLE_SIZE, StatsInvalidationPolicy, TableStatistics,
};
pub use feedback::{FeedbackStatistics, PlanFeedback};
pub use histogram::{
    HistogramBucket, HistogramPlaceholder, MAX_BUCKETS_PER_HISTOGRAM,
    MAX_HISTOGRAMS_PER_PUBLICATION, SkewMarker, StatsColumnTarget,
};
pub use ndv::{
    DEFAULT_HLL_PRECISION, ExactNdvCounter, HyperLogLog, NDV_EXACT_THRESHOLD, NdvEstimator,
};
pub use publication::{
    STATS_PUBLICATION_SWITCH_HISTORY_LIMIT, STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES,
    StatsPublication, StatsPublicationAdvisoryEvidenceReference, StatsPublicationBuilder,
    StatsPublicationCandidateState, StatsPublicationDecisionEvidence,
    StatsPublicationDecisionEvidenceKind, StatsPublicationDecisionStage,
    StatsPublicationDecisionTrace, StatsPublicationSummary, StatsPublicationSwitch,
    StatsPublicationSwitchDecision, StatsPublicationSwitchError,
};
pub use validation::StatsValidationError;
