#![forbid(unsafe_code)]

//! Advisory ScenarioEvidence boundary models for Andromeda benchmarks.
//!
//! This crate owns benchmark evidence, history records, and the bounded
//! ScenarioEvidence boundary. Evidence is expirable, version-bound, disableable,
//! and cannot select plans or publish statistics by itself.

mod advisory_boundary;
mod benchmark_history;
mod evidence;
mod flat_json;
mod history_store;
mod plan_cache_bridge;
mod scenario_boundary;
mod scenario_evidence;

mod metric_math {
    pub(crate) use andromeda_bench_workload::percent_change;
}

pub use andromeda_bench_workload::{
    BenchmarkHardwareProfile, BenchmarkRunRequest, BenchmarkWorkloadClass, BudgetStatus,
    DEFAULT_TEMP_BYTES, MAX_DURATION_MS, MAX_EVIDENCE_TTL_MS, MAX_SAMPLES, MAX_TEMP_BYTES,
    find_workload,
};
pub use benchmark_history::{
    BenchmarkHistoryAdvisoryMetadata, BenchmarkHistoryRecord, HistoryQuery, HistoryQueryResult,
    TimeRange,
};
pub use evidence::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY,
    BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER, BenchmarkEvidence,
    BenchmarkMeasurementMode, BenchmarkWorkloadCounter, MAX_BENCHMARK_WORKLOAD_COUNTERS,
};
pub use history_store::BenchmarkHistoryStore;
pub use plan_cache_bridge::{classify_advisory_evidence_for_key, select_minimal_plan};
pub use scenario_boundary::{
    BenchmarkEvidenceBudgets, BenchmarkEvidenceConfidence, BenchmarkEvidenceContext,
    BenchmarkEvidenceValidity, BenchmarkPlanClass, BenchmarkScenarioEvidence,
    BenchmarkScenarioEvidenceError, BenchmarkScenarioTarget, BenchmarkStatsVersion,
};
pub use scenario_evidence::{
    EvidenceConfidence, EvidenceScore, ScenarioEvidence, ScenarioEvidenceAdvisoryUse,
    ScenarioEvidenceError, ScenarioEvidenceOptimizerBoundary, ScenarioId, ScenarioKind,
    ScenarioTarget, ValidityWindow,
};
