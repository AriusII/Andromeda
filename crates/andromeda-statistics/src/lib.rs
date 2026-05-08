#![forbid(unsafe_code)]

//! Owner crate for versioned Andromeda statistics primitives.
//!
//! This crate owns statistics builders, histograms, NDV estimation,
//! correlation evidence, publication digests, and in-memory statistics models.
//! Catalog remains responsible for active publication switching while that path
//! depends on catalog-local `ScenarioEvidence`.
//!
//! Ownership constraints:
//! - Statistics evidence must be version-bound and advisory until validated.
//! - Benchmark output, GPU output, RAM state, and temporary files are not truth.
//! - GPU-assisted refresh work must stay outside C5 commit, WAL, rollback,
//!   recovery, MVCC short-visibility, catalog publication, and security-critical
//!   paths.
//! - Optimizer consumers must explain accepted and rejected statistics through
//!   DecisionTrace.

mod builders;
mod correlation;
mod correlation_publication;
mod descriptor;
mod digest;
mod engine;
mod error;
mod feedback;
mod histogram;
mod ndv;
mod policy;
mod usage;
mod validation;

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
pub use descriptor::{StatsObjectDescriptor, StatsPublicationState, StatsSetDigest};
pub use digest::{
    STATS_CORRELATION_DOMAIN, STATS_CORRELATION_PUBLICATION_DOMAIN, STATS_PUBLICATION_DOMAIN,
    StatsCorrelationDigest, StatsPublicationDigest,
};
pub use engine::{
    AdaptiveHistogram, ColumnStatistics, DEFAULT_BUCKET_COUNT, EquiDepthHistogram,
    EquiWidthHistogram, Histogram, HistogramAlgorithm, STATS_FULL_SCAN_THRESHOLD,
    STATS_INVALIDATION_MUTATION_PCT, STATS_LSN_DELTA_THRESHOLD, STATS_MAX_AGE_HOURS,
    STATS_SAMPLE_SIZE, StatsInvalidationPolicy, TableStatistics,
};
pub use error::StatisticsError;
pub use feedback::{FeedbackStatistics, PlanFeedback};
pub use histogram::{
    HistogramBucket, HistogramPlaceholder, MAX_BUCKETS_PER_HISTOGRAM,
    MAX_HISTOGRAMS_PER_PUBLICATION, SkewMarker, StatsColumnTarget,
};
pub use ndv::{
    DEFAULT_HLL_PRECISION, ExactNdvCounter, HyperLogLog, NDV_EXACT_THRESHOLD, NdvEstimator,
};
pub use policy::StatisticsUsePolicy;
pub use usage::{StatisticsUseDecision, StatisticsUseReason, evaluate_statistics_for_optimizer};
pub use validation::StatsValidationError;
