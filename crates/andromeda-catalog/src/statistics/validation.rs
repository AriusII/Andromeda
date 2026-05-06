#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsValidationError {
    StatsVersionZero,
    ZeroObjectId,
    DuplicateTarget,
    PublicationExceedsHistogramCap,
    HistogramHasNoBuckets,
    HistogramExceedsBucketCap,
    BucketBoundsInverted,
    BucketDistinctExceedsRows,
    BucketsNotMonotonic,
    CatalogVersionZero,
    CorrelationTooFewColumns,
    CorrelationExceedsColumnCap,
    CorrelationDuplicateColumn,
    CorrelationStrengthOutOfRange,
    CorrelationConfidenceOutOfRange,
    CorrelationSampleRowsZero,
    CorrelationPopulationBoundsInverted,
    CorrelationSampleExceedsPopulationUpper,
    CorrelationVersionMismatch,
    CorrelationPublicationExceedsCap,
    DuplicateCorrelationId,
}

impl core::fmt::Display for StatsValidationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            StatsValidationError::StatsVersionZero => {
                "StatsVersion 0 is reserved and cannot be published"
            }
            StatsValidationError::ZeroObjectId => "stats target CatalogObjectId must be non-zero",
            StatsValidationError::DuplicateTarget => {
                "duplicate (object, column) target in stats publication"
            }
            StatsValidationError::PublicationExceedsHistogramCap => {
                "stats publication exceeds MAX_HISTOGRAMS_PER_PUBLICATION"
            }
            StatsValidationError::HistogramHasNoBuckets => {
                "histogram placeholder must contain at least one bucket"
            }
            StatsValidationError::HistogramExceedsBucketCap => {
                "histogram placeholder exceeds MAX_BUCKETS_PER_HISTOGRAM"
            }
            StatsValidationError::BucketBoundsInverted => {
                "histogram bucket lower_inclusive exceeds upper_inclusive"
            }
            StatsValidationError::BucketDistinctExceedsRows => {
                "histogram bucket distinct_estimate exceeds row_estimate"
            }
            StatsValidationError::BucketsNotMonotonic => {
                "histogram buckets must be strictly increasing and non-overlapping"
            }
            StatsValidationError::CatalogVersionZero => {
                "CatalogVersion 0 cannot bind statistics correlation metadata"
            }
            StatsValidationError::CorrelationTooFewColumns => {
                "statistics correlation metadata must reference at least two columns"
            }
            StatsValidationError::CorrelationExceedsColumnCap => {
                "statistics correlation metadata exceeds MAX_COLUMNS_PER_CORRELATION"
            }
            StatsValidationError::CorrelationDuplicateColumn => {
                "statistics correlation metadata repeats an (object, column) reference"
            }
            StatsValidationError::CorrelationStrengthOutOfRange => {
                "statistics correlation strength exceeds the 0..=1000 permille scale"
            }
            StatsValidationError::CorrelationConfidenceOutOfRange => {
                "statistics correlation confidence exceeds the 0..=1000 permille scale"
            }
            StatsValidationError::CorrelationSampleRowsZero => {
                "statistics correlation evidence sample_rows must be non-zero"
            }
            StatsValidationError::CorrelationPopulationBoundsInverted => {
                "statistics correlation population lower bound exceeds upper bound"
            }
            StatsValidationError::CorrelationSampleExceedsPopulationUpper => {
                "statistics correlation sample_rows exceeds population upper bound"
            }
            StatsValidationError::CorrelationVersionMismatch => {
                "statistics correlation entry does not match publication versions"
            }
            StatsValidationError::CorrelationPublicationExceedsCap => {
                "statistics correlation publication exceeds MAX_CORRELATIONS_PER_PUBLICATION"
            }
            StatsValidationError::DuplicateCorrelationId => {
                "duplicate statistics correlation id in publication"
            }
        })
    }
}

impl std::error::Error for StatsValidationError {}
