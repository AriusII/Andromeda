mod publication;

#[cfg(test)]
mod tests_support;

pub use andromeda_statistics::{
    AdaptiveHistogram, ColumnStatistics, CorrelationEvidenceBounds, CorrelationStrengthPermille,
    DEFAULT_BUCKET_COUNT, DEFAULT_HLL_PRECISION, Datum, EquiDepthHistogram,
    EquiDepthHistogramBuilder, EquiWidthHistogram, EquiWidthHistogramBuilder, ExactNdvCounter,
    FeedbackStatistics, Histogram, HistogramAlgorithm, HistogramBucket, HistogramBuilderTrait,
    HistogramPlaceholder, HyperLogLog, MAX_BUCKETS_PER_HISTOGRAM, MAX_COLUMNS_PER_CORRELATION,
    MAX_CORRELATIONS_PER_PUBLICATION, MAX_HISTOGRAMS_PER_PUBLICATION, NDV_EXACT_THRESHOLD,
    NdvEstimator, PlanFeedback, STATS_CORRELATION_DOMAIN, STATS_CORRELATION_PUBLICATION_DOMAIN,
    STATS_FULL_SCAN_THRESHOLD, STATS_INVALIDATION_MUTATION_PCT, STATS_LSN_DELTA_THRESHOLD,
    STATS_MAX_AGE_HOURS, STATS_PUBLICATION_DOMAIN, STATS_SAMPLE_SIZE, SkewMarker, StatisticsError,
    StatisticsUseDecision, StatisticsUsePolicy, StatisticsUseReason, StatsColumnTarget,
    StatsCorrelation, StatsCorrelationDigest, StatsCorrelationId, StatsCorrelationKind,
    StatsCorrelationPublication, StatsCorrelationPublicationBuilder, StatsInvalidationPolicy,
    StatsObjectDescriptor, StatsPublicationDigest, StatsPublicationState, StatsSetDigest,
    StatsValidationError, TableStatistics, evaluate_statistics_for_optimizer,
};
pub use publication::{
    STATS_PUBLICATION_SWITCH_HISTORY_LIMIT, STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES,
    StatsPublication, StatsPublicationAdvisoryEvidenceReference, StatsPublicationBuilder,
    StatsPublicationCandidateState, StatsPublicationDecisionEvidence,
    StatsPublicationDecisionEvidenceKind, StatsPublicationDecisionStage,
    StatsPublicationDecisionTrace, StatsPublicationSummary, StatsPublicationSwitch,
    StatsPublicationSwitchDecision, StatsPublicationSwitchError,
};
