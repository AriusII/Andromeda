use crate::{BenchmarkHardwareProfile, BudgetStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub hardware_profile: BenchmarkHardwareProfile,
    pub duration_ms: u64,
    pub samples: u32,
    pub warmups: u32,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_evidence_is_explicit() {
        let evidence = BenchmarkEvidence {
            workload_id: "protocol-smoke-contract".to_string(),
            hardware_profile: BenchmarkHardwareProfile::Conservative,
            duration_ms: 1_000,
            samples: 5,
            warmups: 1,
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
        assert_eq!(evidence.hardware_profile.as_str(), "conservative");
        assert_eq!(evidence.measurement_mode.as_str(), "synthetic-diagnostic");
        assert_eq!(evidence.engine_harness, None);
        assert_eq!(
            evidence.synthetic_model_version,
            Some("bounded-diagnostic-v1")
        );
    }
}
