use crate::{BenchmarkHardwareProfile, BudgetStatus};

use super::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY, BenchmarkMeasurementMode, BenchmarkWorkloadCounter,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkEvidence {
    pub workload_id: String,
    pub workload_hypothesis: &'static str,
    pub workload_shape_version: &'static str,
    pub workload_size: &'static str,
    pub primary_metric: &'static str,
    pub baseline_ref: &'static str,
    pub budget_origin: &'static str,
    pub decision_linkage: &'static str,
    pub hardware_profile: BenchmarkHardwareProfile,
    pub duration_ms: u64,
    pub samples: u32,
    pub warmups: u32,
    pub temp_budget_bytes: u64,
    pub started_at_unix_ms: u64,
    pub elapsed_ms: u64,
    pub sample_count: u32,
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub error_count: u32,
    pub budget_status: BudgetStatus,
    pub diagnostic_only: bool,
    pub measurement_mode: BenchmarkMeasurementMode,
    pub latency_source: &'static str,
    pub timing_source: &'static str,
    pub engine_harness: Option<&'static str>,
    pub synthetic_model_version: Option<&'static str>,
    pub workload_counters: Vec<BenchmarkWorkloadCounter>,
    pub commit_sha: Option<String>,
    pub rustc_version: &'static str,
    pub process_pid: u32,
}

impl BenchmarkEvidence {
    /// Benchmark evidence is operational evidence only, never an optimizer decision.
    pub const fn is_authoritative(&self) -> bool {
        BENCHMARK_EVIDENCE_AUTHORITATIVE
    }

    /// Benchmark evidence can inform later catalog integration, but cannot select a plan alone.
    pub const fn can_select_plan_alone(&self) -> bool {
        BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE
    }

    pub const fn optimizer_consumption_role(&self) -> &'static str {
        BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY
    }
}
