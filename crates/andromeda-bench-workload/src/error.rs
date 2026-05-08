use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkError {
    EmptyWorkloadId,
    UnknownWorkload,
    ZeroDuration,
    ZeroSamples,
    DurationExceedsGlobalLimit,
    SamplesExceedsGlobalLimit,
    WarmupsExceedsGlobalLimit,
    ZeroTempBudget,
    TempBudgetExceedsGlobalLimit,
    DurationExceedsWorkloadLimit,
    SamplesExceedsWorkloadLimit,
    TempBudgetExceedsWorkloadLimit,
    InsufficientSamplesForStatistics,
    ErrorCountExceedsSamples,
    HarnessFailed,
}

impl fmt::Display for BenchmarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyWorkloadId => "benchmark workload id must not be empty",
            Self::UnknownWorkload => "benchmark workload is not registered",
            Self::ZeroDuration => "benchmark duration must be greater than zero",
            Self::ZeroSamples => "benchmark sample count must be greater than zero",
            Self::DurationExceedsGlobalLimit => "benchmark duration exceeds the global limit",
            Self::SamplesExceedsGlobalLimit => "benchmark sample count exceeds the global limit",
            Self::WarmupsExceedsGlobalLimit => "benchmark warmup count exceeds the global limit",
            Self::ZeroTempBudget => "benchmark temp budget must be greater than zero",
            Self::TempBudgetExceedsGlobalLimit => "benchmark temp budget exceeds the global limit",
            Self::DurationExceedsWorkloadLimit => "benchmark duration exceeds the workload limit",
            Self::SamplesExceedsWorkloadLimit => {
                "benchmark sample count exceeds the workload limit"
            }
            Self::TempBudgetExceedsWorkloadLimit => {
                "benchmark temp budget exceeds the workload limit"
            }
            Self::InsufficientSamplesForStatistics => {
                "benchmark statistics require at least one sample"
            }
            Self::ErrorCountExceedsSamples => "benchmark error count exceeds sample count",
            Self::HarnessFailed => "benchmark harness failed to produce evidence",
        };
        f.write_str(message)
    }
}

impl std::error::Error for BenchmarkError {}
