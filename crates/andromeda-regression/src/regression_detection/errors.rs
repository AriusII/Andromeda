#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkBaselineComparisonError {
    WorkloadIdMismatch,
    MissingBaselineBinding,
    WorkloadShapeVersionMismatch,
    WorkloadClassMismatch,
    BaselineRefMismatch,
    HardwareProfileMismatch,
    MeasurementModeMismatch,
    DurationBudgetMismatch,
    SampleBudgetMismatch,
    TempBudgetMismatch,
    TargetProcedureIdMismatch,
    TargetCatalogVersionMismatch,
    TargetContractHashMismatch,
    TargetStatsVersionMismatch,
    TargetPlanClassMismatch,
}

impl core::fmt::Display for BenchmarkBaselineComparisonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WorkloadIdMismatch => f.write_str("baseline workload id does not match evidence"),
            Self::MissingBaselineBinding => {
                f.write_str("baseline is missing scenario, budget, hardware, or target binding")
            },
            Self::WorkloadShapeVersionMismatch => {
                f.write_str("baseline workload shape version does not match evidence")
            },
            Self::WorkloadClassMismatch => {
                f.write_str("baseline workload class does not match evidence")
            },
            Self::BaselineRefMismatch => f.write_str("baseline reference does not match evidence"),
            Self::HardwareProfileMismatch => {
                f.write_str("baseline hardware profile does not match evidence")
            },
            Self::MeasurementModeMismatch => {
                f.write_str("baseline measurement mode does not match evidence")
            },
            Self::DurationBudgetMismatch => {
                f.write_str("baseline duration budget does not match evidence")
            },
            Self::SampleBudgetMismatch => {
                f.write_str("baseline sample budget does not match evidence")
            },
            Self::TempBudgetMismatch => f.write_str("baseline temp budget does not match evidence"),
            Self::TargetProcedureIdMismatch => {
                f.write_str("baseline target ProcedureId does not match evidence")
            },
            Self::TargetCatalogVersionMismatch => {
                f.write_str("baseline target CatalogVersion does not match evidence")
            },
            Self::TargetContractHashMismatch => {
                f.write_str("baseline target ContractHash does not match evidence")
            },
            Self::TargetStatsVersionMismatch => {
                f.write_str("baseline target StatsVersion does not match evidence")
            },
            Self::TargetPlanClassMismatch => {
                f.write_str("baseline target PlanClass does not match evidence")
            },
        }
    }
}

impl std::error::Error for BenchmarkBaselineComparisonError {}
