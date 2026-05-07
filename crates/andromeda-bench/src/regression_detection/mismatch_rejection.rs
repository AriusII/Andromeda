use super::errors::BenchmarkBaselineComparisonError;

pub(super) fn compare_optional_str(
    actual: Option<&str>,
    expected: &str,
    mismatch: BenchmarkBaselineComparisonError,
) -> Result<(), BenchmarkBaselineComparisonError> {
    match actual {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(mismatch),
        None => Err(BenchmarkBaselineComparisonError::MissingBaselineBinding),
    }
}

pub(super) fn compare_optional_u64(
    actual: Option<u64>,
    expected: u64,
    mismatch: BenchmarkBaselineComparisonError,
) -> Result<(), BenchmarkBaselineComparisonError> {
    match actual {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(mismatch),
        None => Err(BenchmarkBaselineComparisonError::MissingBaselineBinding),
    }
}

pub(super) fn compare_optional_u32(
    actual: Option<u32>,
    expected: u32,
    mismatch: BenchmarkBaselineComparisonError,
) -> Result<(), BenchmarkBaselineComparisonError> {
    match actual {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(mismatch),
        None => Err(BenchmarkBaselineComparisonError::MissingBaselineBinding),
    }
}
