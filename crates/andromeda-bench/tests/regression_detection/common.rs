use andromeda_bench::{BenchmarkPlanClass, BenchmarkScenarioTarget, BenchmarkStatsVersion};
use andromeda_core::{CatalogVersion, ContractHash, ProcedureId};

pub(crate) fn scenario_target(stats_version: u64) -> BenchmarkScenarioTarget {
    BenchmarkScenarioTarget::new(
        ProcedureId::new(0x5253),
        CatalogVersion::new(9),
        ContractHash::test_vector(0x52),
        BenchmarkStatsVersion::new(stats_version),
        BenchmarkPlanClass::StatsAdaptive,
    )
    .unwrap()
}
