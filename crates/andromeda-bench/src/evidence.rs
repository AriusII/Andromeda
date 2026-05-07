use crate::{BenchmarkHardwareProfile, BudgetStatus};

pub const BENCHMARK_EVIDENCE_AUTHORITATIVE: bool = false;
pub const BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE: bool = false;
pub const BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY: &str = "advisory-only";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BenchmarkMeasurementMode {
    SyntheticDiagnostic,
    HarnessDiagnostic,
}

impl BenchmarkMeasurementMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SyntheticDiagnostic => "synthetic-diagnostic",
            Self::HarnessDiagnostic => "harness-diagnostic",
        }
    }
}

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
    pub engine_harness: Option<&'static str>,
    pub synthetic_model_version: Option<&'static str>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_evidence_is_explicit() {
        let evidence = BenchmarkEvidence {
            workload_id: "protocol-smoke-contract".to_string(),
            workload_hypothesis: "typed protocol contract inspection should stay bounded without opening a network surface",
            workload_shape_version: "protocol-smoke-contract.synthetic.v1",
            workload_size: "contract-only synthetic inspection, samples<=20, duration_ms<=5000",
            primary_metric: "p50_latency_us,p95_latency_us,error_rate_ppm",
            baseline_ref: "history.protocol-smoke-contract.synthetic.v1",
            budget_origin: "static-workload-registry-v1",
            decision_linkage: "advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass",
            hardware_profile: BenchmarkHardwareProfile::Conservative,
            duration_ms: 1_000,
            samples: 5,
            warmups: 1,
            temp_budget_bytes: 8 * 1024 * 1024,
            started_at_unix_ms: 1,
            elapsed_ms: 950,
            sample_count: 5,
            p50_latency_us: 10,
            p95_latency_us: 20,
            error_count: 0,
            budget_status: BudgetStatus::Passed,
            diagnostic_only: true,
            measurement_mode: BenchmarkMeasurementMode::SyntheticDiagnostic,
            latency_source: "deterministic-latency-model",
            engine_harness: None,
            synthetic_model_version: Some("bounded-diagnostic-v1"),
        };

        assert!(evidence.diagnostic_only);
        assert!(evidence.workload_hypothesis.contains("protocol contract"));
        assert_eq!(
            evidence.workload_shape_version,
            "protocol-smoke-contract.synthetic.v1"
        );
        assert!(evidence.workload_size.contains("samples<=20"));
        assert_eq!(
            evidence.primary_metric,
            "p50_latency_us,p95_latency_us,error_rate_ppm"
        );
        assert_eq!(
            evidence.baseline_ref,
            "history.protocol-smoke-contract.synthetic.v1"
        );
        assert_eq!(evidence.budget_origin, "static-workload-registry-v1");
        assert!(evidence.decision_linkage.contains("ContractHash"));
        assert_eq!(evidence.hardware_profile.as_str(), "conservative");
        assert_eq!(evidence.measurement_mode.as_str(), "synthetic-diagnostic");
        assert_eq!(evidence.engine_harness, None);
        assert_eq!(
            evidence.synthetic_model_version,
            Some("bounded-diagnostic-v1")
        );
        assert!(!evidence.is_authoritative());
        assert!(!evidence.can_select_plan_alone());
        assert_eq!(evidence.optimizer_consumption_role(), "advisory-only");
    }
}
