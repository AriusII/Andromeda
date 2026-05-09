use crate::metric_math::{error_rate_ppm, percent_change};
use crate::{
    BENCHMARK_EVIDENCE_AUTHORITATIVE, BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE,
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY, BenchmarkEvidence, BenchmarkScenarioTarget,
};
use andromeda_scenario_evidence::flat_json::escape_json_string;

use super::baseline::BenchmarkBaseline;
use super::errors::BenchmarkBaselineComparisonError;
use super::reason::RegressionReason;
use super::threshold_evaluation::evaluate_thresholds;

/// Result of comparing current evidence against baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct RegressionAnalysis {
    /// Workload being analyzed
    pub workload_id: String,
    /// Current P50 latency in microseconds
    pub current_p50_us: u64,
    /// Baseline P50 latency in microseconds
    pub baseline_p50_us: u64,
    /// P50 regression percentage (0.0 = no change, 100.0 = doubled)
    pub p50_regression_pct: f64,
    /// Current P95 latency in microseconds
    pub current_p95_us: u64,
    /// Baseline P95 latency in microseconds
    pub baseline_p95_us: u64,
    /// P95 regression percentage
    pub p95_regression_pct: f64,
    /// Current error count
    pub current_error_count: u32,
    /// Current sample count used to compute error-rate evidence
    pub current_sample_count: u32,
    /// Baseline error count
    pub baseline_error_count: u32,
    /// Baseline sample count used to compute error-rate evidence
    pub baseline_sample_count: u32,
    /// Current error rate in parts-per-million
    pub current_error_rate_ppm: u64,
    /// Baseline error rate in parts-per-million
    pub baseline_error_rate_ppm: u64,
    /// Error rate regression percentage
    pub error_rate_regression_pct: f64,
    /// Primary reason for regression
    pub primary_reason: RegressionReason,
    /// Is regressed (true if any metric degraded beyond threshold)
    pub is_regressed: bool,
    /// Severity: 0 (none), 1 (minor <5%), 2 (moderate 5% to <20%), 3 (severe >=20%)
    pub severity: u8,
}

impl RegressionAnalysis {
    /// Create regression analysis from current and baseline metrics.
    ///
    /// Regression thresholds:
    /// - P50/P95 latency: degradation > 2.5% is flagged
    /// - Error rate: any increase is flagged
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workload_id: String,
        current_p50_us: u64,
        baseline_p50_us: u64,
        current_p95_us: u64,
        baseline_p95_us: u64,
        current_error_count: u32,
        current_sample_count: u32,
        baseline_error_count: u32,
        baseline_sample_count: u32,
    ) -> Self {
        // Calculate regression percentages
        let p50_regression_pct = percent_change(current_p50_us, baseline_p50_us);
        let p95_regression_pct = percent_change(current_p95_us, baseline_p95_us);
        let current_error_rate_ppm = error_rate_ppm(current_error_count, current_sample_count);
        let baseline_error_rate_ppm = error_rate_ppm(baseline_error_count, baseline_sample_count);
        let error_rate_regression_pct =
            percent_change(current_error_rate_ppm, baseline_error_rate_ppm);
        let threshold_evaluation = evaluate_thresholds(
            p50_regression_pct,
            p95_regression_pct,
            error_rate_regression_pct,
        );
        let is_regressed = threshold_evaluation.regressed_count > 0;

        Self {
            workload_id,
            current_p50_us,
            baseline_p50_us,
            p50_regression_pct,
            current_p95_us,
            baseline_p95_us,
            p95_regression_pct,
            current_error_count,
            current_sample_count,
            baseline_error_count,
            baseline_sample_count,
            current_error_rate_ppm,
            baseline_error_rate_ppm,
            error_rate_regression_pct,
            primary_reason: threshold_evaluation.primary_reason,
            is_regressed,
            severity: threshold_evaluation.severity,
        }
    }

    pub fn from_baseline_and_evidence(
        baseline: &BenchmarkBaseline,
        evidence: &BenchmarkEvidence,
        target: BenchmarkScenarioTarget,
    ) -> Result<Self, BenchmarkBaselineComparisonError> {
        baseline.validate_comparable_to_evidence(evidence, target)?;
        Ok(Self::new(
            evidence.workload_id.clone(),
            evidence.p50_latency_us,
            baseline.p50_latency_us,
            evidence.p95_latency_us,
            baseline.p95_latency_us,
            evidence.error_count,
            evidence.sample_count,
            baseline.error_count,
            baseline.sample_count,
        ))
    }

    /// Regression analysis is CI and diagnostic evidence, never production truth.
    pub const fn is_production_truth(&self) -> bool {
        false
    }

    /// Regression analysis is never authoritative for optimizer or runtime decisions.
    pub const fn is_authoritative(&self) -> bool {
        BENCHMARK_EVIDENCE_AUTHORITATIVE
    }

    /// Regression analysis cannot select an execution plan by itself.
    pub const fn can_select_plan_alone(&self) -> bool {
        BENCHMARK_EVIDENCE_CAN_SELECT_PLAN_ALONE
    }

    pub const fn optimizer_consumption_role(&self) -> &'static str {
        BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY
    }

    /// Serialize analysis to JSON for CI reporting.
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"workload_id":"{}","current_p50_us":{},"baseline_p50_us":{},"p50_regression_pct":{:.2},"current_p95_us":{},"baseline_p95_us":{},"p95_regression_pct":{:.2},"current_error_count":{},"current_sample_count":{},"baseline_error_count":{},"baseline_sample_count":{},"current_error_rate_ppm":{},"baseline_error_rate_ppm":{},"error_rate_regression_pct":{:.2},"primary_reason":"{}","is_regressed":{},"severity":{},"production_truth":{},"authoritative":{},"can_select_plan_alone":{},"optimizer_boundary":"{}"}}"#,
            escape_json_string(&self.workload_id),
            self.current_p50_us,
            self.baseline_p50_us,
            self.p50_regression_pct,
            self.current_p95_us,
            self.baseline_p95_us,
            self.p95_regression_pct,
            self.current_error_count,
            self.current_sample_count,
            self.baseline_error_count,
            self.baseline_sample_count,
            self.current_error_rate_ppm,
            self.baseline_error_rate_ppm,
            self.error_rate_regression_pct,
            self.primary_reason.as_str(),
            self.is_regressed,
            self.severity,
            self.is_production_truth(),
            self.is_authoritative(),
            self.can_select_plan_alone(),
            escape_json_string(self.optimizer_consumption_role())
        )
    }
}
