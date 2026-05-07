use super::super::{HistogramPlaceholder, MAX_BUCKETS_PER_HISTOGRAM, StatsValidationError};

pub(super) fn validate_histogram(
    histogram: &HistogramPlaceholder,
) -> Result<(), StatsValidationError> {
    if histogram.buckets().is_empty() {
        return Err(StatsValidationError::HistogramHasNoBuckets);
    }
    if histogram.buckets().len() > MAX_BUCKETS_PER_HISTOGRAM {
        return Err(StatsValidationError::HistogramExceedsBucketCap);
    }

    let mut previous_upper = None;
    for bucket in histogram.buckets() {
        bucket.validate()?;
        if let Some(previous) = previous_upper
            && bucket.lower_inclusive <= previous
        {
            return Err(StatsValidationError::BucketsNotMonotonic);
        }
        previous_upper = Some(bucket.upper_inclusive);
    }
    Ok(())
}
