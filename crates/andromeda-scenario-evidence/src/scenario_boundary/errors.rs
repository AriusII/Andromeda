/// Closed reject reasons for benchmark-side evidence boundary validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkScenarioEvidenceError {
    EmptyWorkloadId,
    UnknownWorkloadId,
    EmptyCommitId,
    EmptyObservedAt,
    ZeroDurationBudget,
    DurationBudgetExceedsGlobalLimit,
    DurationBudgetExceedsWorkloadLimit,
    DurationBudgetExceedsHistoryCap,
    ZeroSampleBudget,
    SampleBudgetExceedsGlobalLimit,
    SampleBudgetExceedsWorkloadLimit,
    SampleBudgetExceedsHistoryCap,
    ZeroTempBudget,
    TempBudgetExceedsGlobalLimit,
    TempBudgetExceedsWorkloadLimit,
    TempBudgetExceedsHistoryCap,
    RecordSampleCountZero,
    RecordSampleCountExceedsBudget,
    ErrorCountExceedsSampleCount,
    LatencyPercentileOrderInvalid,
    ConfidenceOutOfRange,
    ExpiryMustBeNonZero,
    IssuedNotBeforeExpiry,
    ValidityWindowExceedsLimit,
    TargetProcedureIdZero,
    TargetCatalogVersionZero,
    TargetContractHashZero,
    TargetStatsVersionZero,
    TargetProcedureIdMismatch,
    TargetCatalogVersionMismatch,
    TargetContractHashMismatch,
    TargetStatsVersionMismatch,
    TargetPlanClassMismatch,
    EmptyLatencySource,
    EmptyTimingSource,
    MissingEngineHarness,
    MissingSyntheticModelVersion,
    ConflictingSyntheticAndHarnessContext,
    WorkloadMeasurementModeMismatch,
    HistoryAdvisoryBoundaryInvalid,
    HistoryAuthoritative,
    HistoryCanSelectPlanAlone,
    HistoryOptimizerBoundaryInvalid,
    HistoryDurationCapInvalid,
    HistorySampleCapInvalid,
    HistoryTempCapInvalid,
    NotYetValid,
    Expired,
}

impl core::fmt::Display for BenchmarkScenarioEvidenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyWorkloadId => {
                f.write_str("benchmark evidence workload id must not be empty")
            }
            Self::UnknownWorkloadId => {
                f.write_str("benchmark evidence workload id is not registered")
            }
            Self::EmptyCommitId => f.write_str("benchmark evidence commit id must not be empty"),
            Self::EmptyObservedAt => {
                f.write_str("benchmark evidence observed timestamp must not be empty")
            }
            Self::ZeroDurationBudget => {
                f.write_str("benchmark evidence duration budget must be greater than zero")
            }
            Self::DurationBudgetExceedsGlobalLimit => {
                f.write_str("benchmark evidence duration budget exceeds the global limit")
            }
            Self::DurationBudgetExceedsWorkloadLimit => {
                f.write_str("benchmark evidence duration budget exceeds the workload limit")
            }
            Self::DurationBudgetExceedsHistoryCap => {
                f.write_str("benchmark evidence duration budget exceeds the history record cap")
            }
            Self::ZeroSampleBudget => {
                f.write_str("benchmark evidence sample budget must be greater than zero")
            }
            Self::SampleBudgetExceedsGlobalLimit => {
                f.write_str("benchmark evidence sample budget exceeds the global limit")
            }
            Self::SampleBudgetExceedsWorkloadLimit => {
                f.write_str("benchmark evidence sample budget exceeds the workload limit")
            }
            Self::SampleBudgetExceedsHistoryCap => {
                f.write_str("benchmark evidence sample budget exceeds the history record cap")
            }
            Self::ZeroTempBudget => {
                f.write_str("benchmark evidence temp budget must be greater than zero")
            }
            Self::TempBudgetExceedsGlobalLimit => {
                f.write_str("benchmark evidence temp budget exceeds the global limit")
            }
            Self::TempBudgetExceedsWorkloadLimit => {
                f.write_str("benchmark evidence temp budget exceeds the workload limit")
            }
            Self::TempBudgetExceedsHistoryCap => {
                f.write_str("benchmark evidence temp budget exceeds the history record cap")
            }
            Self::RecordSampleCountZero => {
                f.write_str("benchmark evidence record must carry at least one sample")
            }
            Self::RecordSampleCountExceedsBudget => {
                f.write_str("benchmark evidence record sample count exceeds the sample budget")
            }
            Self::ErrorCountExceedsSampleCount => {
                f.write_str("benchmark evidence error count exceeds sample count")
            }
            Self::LatencyPercentileOrderInvalid => {
                f.write_str("benchmark evidence p95 latency must be >= p50 latency")
            }
            Self::ConfidenceOutOfRange => {
                f.write_str("benchmark evidence confidence must be in 0..=1000")
            }
            Self::ExpiryMustBeNonZero => {
                f.write_str("benchmark evidence validity expiry must be non-zero")
            }
            Self::IssuedNotBeforeExpiry => {
                f.write_str("benchmark evidence validity requires issued_at < expires_at")
            }
            Self::ValidityWindowExceedsLimit => {
                f.write_str("benchmark evidence validity window exceeds the configured limit")
            }
            Self::TargetProcedureIdZero => {
                f.write_str("benchmark evidence target ProcedureId must be non-zero")
            }
            Self::TargetCatalogVersionZero => {
                f.write_str("benchmark evidence target CatalogVersion must be non-zero")
            }
            Self::TargetContractHashZero => {
                f.write_str("benchmark evidence target ContractHash must be non-zero")
            }
            Self::TargetStatsVersionZero => {
                f.write_str("benchmark evidence target StatsVersion must be non-zero")
            }
            Self::TargetProcedureIdMismatch => {
                f.write_str("benchmark evidence target ProcedureId is not current")
            }
            Self::TargetCatalogVersionMismatch => {
                f.write_str("benchmark evidence target CatalogVersion is not current")
            }
            Self::TargetContractHashMismatch => {
                f.write_str("benchmark evidence target ContractHash is not current")
            }
            Self::TargetStatsVersionMismatch => {
                f.write_str("benchmark evidence target StatsVersion is not current")
            }
            Self::TargetPlanClassMismatch => {
                f.write_str("benchmark evidence target PlanClass is not current")
            }
            Self::EmptyLatencySource => {
                f.write_str("benchmark evidence latency source must not be empty")
            }
            Self::EmptyTimingSource => {
                f.write_str("benchmark evidence timing source must not be empty")
            }
            Self::MissingEngineHarness => {
                f.write_str("benchmark evidence harness mode requires an engine harness")
            }
            Self::MissingSyntheticModelVersion => {
                f.write_str("benchmark evidence synthetic mode requires a model version")
            }
            Self::ConflictingSyntheticAndHarnessContext => {
                f.write_str("benchmark evidence cannot be synthetic and harness-backed")
            }
            Self::WorkloadMeasurementModeMismatch => {
                f.write_str("benchmark evidence workload class does not match measurement mode")
            }
            Self::HistoryAdvisoryBoundaryInvalid => {
                f.write_str("benchmark history advisory boundary must be advisory-only")
            }
            Self::HistoryAuthoritative => {
                f.write_str("benchmark history records must not be authoritative")
            }
            Self::HistoryCanSelectPlanAlone => {
                f.write_str("benchmark history records must not select plans alone")
            }
            Self::HistoryOptimizerBoundaryInvalid => {
                f.write_str("benchmark history optimizer boundary must be advisory-only")
            }
            Self::HistoryDurationCapInvalid => {
                f.write_str("benchmark history duration cap is outside global limits")
            }
            Self::HistorySampleCapInvalid => {
                f.write_str("benchmark history sample cap is outside global limits")
            }
            Self::HistoryTempCapInvalid => {
                f.write_str("benchmark history temp cap is outside global limits")
            }
            Self::NotYetValid => f.write_str("benchmark evidence is not yet valid"),
            Self::Expired => f.write_str("benchmark evidence has expired"),
        }
    }
}

impl std::error::Error for BenchmarkScenarioEvidenceError {}
