#![forbid(unsafe_code)]

//! Benchmark Regression Detection — Wave 13 Implementation
//!
//! This module implements performance regression detection for benchmarks.
//! It compares current evidence against baseline evidence and determines:
//! 1. If any metric exceeded its budget (absolute failure)
//! 2. If any metric degraded vs. baseline (relative regression)
//!
//! ## Wave 13 Scope
//!
//! ✅ Implemented:
//! - Baseline storage and loading (JSON serialization)
//! - Regression detection (latency, throughput, error rate)
//! - Regression reasoning (why a workload regressed)
//! - Deterministic regression decision
//! - Unit tests for all scenarios
//!
//! ## CI Integration Status
//!
//! TECH-DEBT:
//! - Context: this module can serialize/deserialize baselines, but CI artifact
//!   load/store and regression-gate wiring are not implemented in Rust yet.
//! - Risk: regressions are detected only when this module is invoked manually.
//! - Closure: wire baseline artifact I/O and call the analysis path from the
//!   performance workflow entrypoint.

use crate::flat_json::{escape_json_string, parse_flat_json_object, required_string, required_u64};

/// Baseline evidence for regression comparison.
///
/// A baseline captures the "good" performance of a workload for future regression detection.
/// This structure is serialized to JSON and can be archived as a CI artifact.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkBaseline {
    /// Workload identifier (must match current run)
    pub workload_id: String,
    /// Baseline P50 latency in microseconds
    pub p50_latency_us: u64,
    /// Baseline P95 latency in microseconds
    pub p95_latency_us: u64,
    /// Baseline error count (typically 0)
    pub error_count: u32,
    /// Baseline sample count
    pub sample_count: u32,
    /// ISO 8601 timestamp when baseline was established
    pub established_at: String,
}

impl BenchmarkBaseline {
    /// Create a new baseline from current evidence.
    pub fn from_evidence(
        workload_id: String,
        p50_latency_us: u64,
        p95_latency_us: u64,
        error_count: u32,
        sample_count: u32,
        timestamp: String,
    ) -> Self {
        Self {
            workload_id,
            p50_latency_us,
            p95_latency_us,
            error_count,
            sample_count,
            established_at: timestamp,
        }
    }

    /// Serialize baseline to JSON for artifact storage.
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"workload_id":"{}","p50_latency_us":{},"p95_latency_us":{},"error_count":{},"sample_count":{},"established_at":"{}"}}"#,
            escape_json_string(&self.workload_id),
            self.p50_latency_us,
            self.p95_latency_us,
            self.error_count,
            self.sample_count,
            escape_json_string(&self.established_at)
        )
    }

    /// Deserialize baseline from JSON.
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        let value = parse_flat_json_object(json_str)
            .map_err(|error| format!("invalid baseline JSON: {error}"))?;

        let workload_id = required_string(&value, "workload_id")?;
        let p50_latency_us = required_u64(&value, "p50_latency_us")?;
        let p95_latency_us = required_u64(&value, "p95_latency_us")?;
        let error_count = u32::try_from(required_u64(&value, "error_count")?)
            .map_err(|_| "error_count exceeds u32".to_string())?;
        let sample_count = u32::try_from(required_u64(&value, "sample_count")?)
            .map_err(|_| "sample_count exceeds u32".to_string())?;
        let established_at = required_string(&value, "established_at")?;

        Ok(Self {
            workload_id,
            p50_latency_us,
            p95_latency_us,
            error_count,
            sample_count,
            established_at,
        })
    }
}

/// Reason why a workload regressed (or didn't).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegressionReason {
    /// P50 latency increased beyond baseline
    P50Degradation,
    /// P95 latency increased beyond baseline
    P95Degradation,
    /// Error rate increased
    ErrorRateIncrease,
    /// Multiple metrics degraded
    MultipleMetrics,
    /// No regression detected
    NoRegression,
}

impl RegressionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::P50Degradation => "p50-latency-degradation",
            Self::P95Degradation => "p95-latency-degradation",
            Self::ErrorRateIncrease => "error-rate-increase",
            Self::MultipleMetrics => "multiple-metrics",
            Self::NoRegression => "no-regression",
        }
    }
}

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
    /// Baseline error count
    pub baseline_error_count: u32,
    /// Error rate regression percentage
    pub error_rate_regression_pct: f64,
    /// Primary reason for regression
    pub primary_reason: RegressionReason,
    /// Is regressed (true if any metric degraded beyond threshold)
    pub is_regressed: bool,
    /// Severity: 0 (none), 1 (minor <5%), 2 (moderate 5-20%), 3 (severe >20%)
    pub severity: u8,
}

impl RegressionAnalysis {
    /// Create regression analysis from current and baseline metrics.
    ///
    /// Regression thresholds:
    /// - P50/P95 latency: degradation > 2.5% is flagged
    /// - Error rate: any increase is flagged
    pub fn new(
        workload_id: String,
        current_p50_us: u64,
        baseline_p50_us: u64,
        current_p95_us: u64,
        baseline_p95_us: u64,
        current_error_count: u32,
        baseline_error_count: u32,
    ) -> Self {
        // Calculate regression percentages
        let p50_regression_pct = compute_regression_percentage(current_p50_us, baseline_p50_us);
        let p95_regression_pct = compute_regression_percentage(current_p95_us, baseline_p95_us);
        let error_rate_regression_pct = if baseline_error_count == 0 {
            if current_error_count > 0 { 100.0 } else { 0.0 }
        } else {
            compute_regression_percentage(current_error_count as u64, baseline_error_count as u64)
        };

        // Determine regression status
        const REGRESSION_THRESHOLD_PCT: f64 = 2.5;
        let p50_regressed = p50_regression_pct > REGRESSION_THRESHOLD_PCT;
        let p95_regressed = p95_regression_pct > REGRESSION_THRESHOLD_PCT;
        let error_regressed = error_rate_regression_pct > 0.0;

        // Determine primary reason
        let (primary_reason, regressed_count) = if p50_regressed || p95_regressed || error_regressed
        {
            let mut count = 0;
            if p50_regressed {
                count += 1;
            }
            if p95_regressed {
                count += 1;
            }
            if error_regressed {
                count += 1;
            }

            let reason = if count > 1 {
                RegressionReason::MultipleMetrics
            } else if p50_regressed {
                RegressionReason::P50Degradation
            } else if p95_regressed {
                RegressionReason::P95Degradation
            } else {
                RegressionReason::ErrorRateIncrease
            };
            (reason, count)
        } else {
            (RegressionReason::NoRegression, 0)
        };

        // Determine severity
        let max_regression = p50_regression_pct
            .max(p95_regression_pct)
            .max(error_rate_regression_pct);
        let severity = if max_regression == 0.0 {
            0
        } else if max_regression < 5.0 {
            1
        } else if max_regression < 20.0 {
            2
        } else {
            3
        };

        Self {
            workload_id,
            current_p50_us,
            baseline_p50_us,
            p50_regression_pct,
            current_p95_us,
            baseline_p95_us,
            p95_regression_pct,
            current_error_count,
            baseline_error_count,
            error_rate_regression_pct,
            primary_reason,
            is_regressed: regressed_count > 0,
            severity,
        }
    }

    /// Serialize analysis to JSON for CI reporting.
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"workload_id":"{}","current_p50_us":{},"baseline_p50_us":{},"p50_regression_pct":{:.2},"current_p95_us":{},"baseline_p95_us":{},"p95_regression_pct":{:.2},"current_error_count":{},"baseline_error_count":{},"error_rate_regression_pct":{:.2},"primary_reason":"{}","is_regressed":{},"severity":{}}}"#,
            escape_json_string(&self.workload_id),
            self.current_p50_us,
            self.baseline_p50_us,
            self.p50_regression_pct,
            self.current_p95_us,
            self.baseline_p95_us,
            self.p95_regression_pct,
            self.current_error_count,
            self.baseline_error_count,
            self.error_rate_regression_pct,
            self.primary_reason.as_str(),
            self.is_regressed,
            self.severity
        )
    }
}

/// Compute regression percentage: ((new - old) / old) * 100.0
/// Returns positive percentage if degraded, negative if improved, 0 if unchanged.
fn compute_regression_percentage(current: u64, baseline: u64) -> f64 {
    if baseline == 0 {
        if current == 0 { 0.0 } else { 100.0 }
    } else {
        ((current as f64 - baseline as f64) / baseline as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_creation_and_json_serialization() {
        let baseline = BenchmarkBaseline::from_evidence(
            "protocol-smoke-contract".to_string(),
            10_000,
            50_000,
            0,
            20,
            "2026-01-15T14:30:45Z".to_string(),
        );

        let json = baseline.to_json();
        let recovered = BenchmarkBaseline::from_json(&json).unwrap();

        assert_eq!(baseline, recovered);
        assert!(json.contains("\"workload_id\":\"protocol-smoke-contract\""));
        assert!(json.contains("\"p50_latency_us\":10000"));
    }

    #[test]
    fn baseline_json_parsing_with_missing_fields() {
        let invalid_json = r#"{"workload_id":"test"}"#;
        let result = BenchmarkBaseline::from_json(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn regression_analysis_no_regression() {
        let analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            10_000, // current P50
            10_000, // baseline P50
            50_000, // current P95
            50_000, // baseline P95
            0,      // current errors
            0,      // baseline errors
        );

        assert!(!analysis.is_regressed);
        assert_eq!(analysis.primary_reason, RegressionReason::NoRegression);
        assert_eq!(analysis.p50_regression_pct, 0.0);
        assert_eq!(analysis.severity, 0);
    }

    #[test]
    fn regression_analysis_minor_p50_degradation() {
        let analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            10_250, // current P50 (2.5% degradation)
            10_000, // baseline P50
            50_000, // current P95
            50_000, // baseline P95
            0,
            0,
        );

        assert!(!analysis.is_regressed); // At threshold, not over
        assert_eq!(analysis.p50_regression_pct, 2.5);
    }

    #[test]
    fn regression_analysis_clear_p50_degradation() {
        let analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            10_300, // current P50 (3% degradation)
            10_000, // baseline P50
            50_000, // current P95
            50_000, // baseline P95
            0,
            0,
        );

        assert!(analysis.is_regressed);
        assert_eq!(analysis.primary_reason, RegressionReason::P50Degradation);
        assert_eq!(analysis.severity, 1); // Minor (1-5%)
    }

    #[test]
    fn regression_analysis_moderate_p95_degradation() {
        let analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            10_000, // current P50
            10_000, // baseline P50
            57_500, // current P95 (15% degradation)
            50_000, // baseline P95
            0,
            0,
        );

        assert!(analysis.is_regressed);
        assert_eq!(analysis.primary_reason, RegressionReason::P95Degradation);
        assert_eq!(analysis.severity, 2); // Moderate (5-20%)
    }

    #[test]
    fn regression_analysis_severe_latency_degradation() {
        let analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            10_000, // current P50
            10_000, // baseline P50
            60_500, // current P95 (21% degradation)
            50_000, // baseline P95
            0,
            0,
        );

        assert!(analysis.is_regressed);
        assert_eq!(analysis.severity, 3); // Severe (>20%)
    }

    #[test]
    fn regression_analysis_error_rate_increase() {
        let analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            10_000, // current P50
            10_000, // baseline P50
            50_000, // current P95
            50_000, // baseline P95
            5,      // current errors
            0,      // baseline errors
        );

        assert!(analysis.is_regressed);
        assert_eq!(analysis.primary_reason, RegressionReason::ErrorRateIncrease);
        assert_eq!(analysis.error_rate_regression_pct, 100.0);
    }

    #[test]
    fn regression_analysis_multiple_metrics_degradation() {
        let analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            10_500, // P50 +5%
            10_000, // baseline P50
            60_000, // P95 +20%
            50_000, // baseline P95
            2,      // errors
            0,      // baseline errors
        );

        assert!(analysis.is_regressed);
        assert_eq!(analysis.primary_reason, RegressionReason::MultipleMetrics);
        assert_eq!(analysis.severity, 3); // Max severity due to severe P95 degradation
    }

    #[test]
    fn regression_analysis_improvement() {
        let analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            9_500,  // current P50 (5% improvement)
            10_000, // baseline P50
            45_000, // current P95 (10% improvement)
            50_000, // baseline P95
            0,
            0,
        );

        assert!(!analysis.is_regressed);
        assert_eq!(analysis.p50_regression_pct, -5.0);
        assert_eq!(analysis.severity, 0);
    }

    #[test]
    fn regression_analysis_json_serialization() {
        let analysis = RegressionAnalysis::new(
            "btree-lookup-smoke".to_string(),
            26,  // P50
            25,  // baseline P50
            510, // P95
            500, // baseline P95
            0,
            0,
        );

        let json = analysis.to_json();
        assert!(json.contains("\"workload_id\":\"btree-lookup-smoke\""));
        assert!(json.contains("\"current_p50_us\":26"));
        assert!(json.contains("\"p50_regression_pct\":4.00")); // 4% over threshold
        assert!(json.contains("\"is_regressed\":true"));
    }

    #[test]
    fn regression_percentage_computation() {
        assert_eq!(compute_regression_percentage(100, 100), 0.0); // No change
        assert_eq!(compute_regression_percentage(150, 100), 50.0); // +50%
        assert_eq!(compute_regression_percentage(50, 100), -50.0); // -50%
        assert_eq!(compute_regression_percentage(1, 0), 100.0); // Division by zero handled
        assert_eq!(compute_regression_percentage(0, 0), 0.0); // Both zero
    }
}
