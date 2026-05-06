#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Simulates a complete regression detection workflow.
    /// This test demonstrates how Wave 14+ will use this module in CI.
    #[test]
    fn end_to_end_regression_detection_workflow() {
        // Step 1: Establish baseline from golden commit
        let baseline = BenchmarkBaseline::from_evidence(
            "protocol-smoke-contract".to_string(),
            10_000, // P50: 10ms
            50_000, // P95: 50ms
            0,      // errors
            20,     // samples
            "2026-01-15T14:30:45Z".to_string(),
        );

        // Step 2: Serialize baseline for artifact storage (Wave 14 CI workflow)
        let baseline_json = baseline.to_json();
        assert!(baseline_json.contains("protocol-smoke-contract"));

        // Step 3: Deserialize baseline for comparison (Wave 14 CI gate)
        let recovered_baseline = BenchmarkBaseline::from_json(&baseline_json).unwrap();
        assert_eq!(baseline, recovered_baseline);

        // Step 4: Run new benchmark (produces BenchmarkEvidence)
        // In real workflow, this comes from run_bounded_benchmark()
        let current_p50 = 10_300; // 3% degradation
        let current_p95 = 51_000; // 2% degradation
        let current_errors = 0;
        let current_samples = 20;

        // Step 5: Analyze regression (Wave 14 CI gate logic)
        let analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            current_p50,
            baseline.p50_latency_us,
            current_p95,
            baseline.p95_latency_us,
            current_errors,
            baseline.error_count,
        );

        // Step 6: Make CI decision
        if analysis.is_regressed {
            // This would fail the PR check in Wave 14
            match analysis.severity {
                0 => {} // Green: all pass
                1 => {
                    // Yellow: comment on PR
                    println!(
                        "Minor regression detected: {}",
                        analysis.primary_reason.as_str()
                    );
                }
                2 => {
                    // Orange: warning
                    println!("Moderate regression: {}", analysis.primary_reason.as_str());
                }
                3 => {
                    // Red: block PR
                    println!("SEVERE regression: {}", analysis.primary_reason.as_str());
                    // return Err("regression detected");
                }
                _ => {}
            }
        }

        // Verify the analysis
        assert!(analysis.is_regressed);
        assert_eq!(analysis.p50_regression_pct, 3.0);
        assert_eq!(analysis.primary_reason, RegressionReason::P50Degradation);
        assert_eq!(analysis.severity, 1); // Minor

        // Step 7: Serialize for audit trail
        let analysis_json = analysis.to_json();
        assert!(analysis_json.contains("is_regressed"));
        assert!(analysis_json.contains("true"));
    }

    /// Tests that baseline can be updated after regression is fixed.
    #[test]
    fn baseline_update_workflow() {
        // Initial baseline (golden commit)
        let baseline_v1 = BenchmarkBaseline::from_evidence(
            "btree-lookup-smoke".to_string(),
            25,  // P50
            500, // P95
            0,   // errors
            20,  // samples
            "2026-01-15T00:00:00Z".to_string(),
        );

        // Analysis detects regression
        let analysis_v1 = RegressionAnalysis::new(
            "btree-lookup-smoke".to_string(),
            30, // Current P50 (+20% degradation)
            baseline_v1.p50_latency_us,
            530, // Current P95 (+6% degradation)
            baseline_v1.p95_latency_us,
            0,
            baseline_v1.error_count,
        );

        assert!(analysis_v1.is_regressed);
        assert_eq!(analysis_v1.severity, 1); // Minor (but crosses 2.5% threshold)

        // Wave 14: Perform code fix and re-test
        // New run shows improvement back to baseline
        let baseline_v2 = BenchmarkBaseline::from_evidence(
            "btree-lookup-smoke".to_string(),
            26,  // P50 (slightly improved)
            495, // P95 (slightly improved)
            0,   // errors
            20,  // samples
            "2026-01-16T00:00:00Z".to_string(),
        );

        // Analysis with new baseline
        let analysis_v2 = RegressionAnalysis::new(
            "btree-lookup-smoke".to_string(),
            26, // Current P50 (matches new baseline)
            baseline_v2.p50_latency_us,
            495, // Current P95 (matches new baseline)
            baseline_v2.p95_latency_us,
            0,
            baseline_v2.error_count,
        );

        assert!(!analysis_v2.is_regressed);
        assert_eq!(analysis_v2.severity, 0); // Green
    }

    /// Tests multi-workload baseline and regression tracking.
    #[test]
    fn multi_workload_regression_tracking() {
        // In Wave 14, CI would manage multiple baselines, one per workload
        let workloads = vec![
            "btree-lookup-smoke",
            "btree-range-scan-smoke",
            "vertical-v0-smoke",
            "protocol-smoke-contract",
            "wal-append-smoke",
        ];

        let mut baselines = Vec::new();
        let mut analyses = Vec::new();

        for workload_id in &workloads {
            // Create baseline
            let baseline = BenchmarkBaseline::from_evidence(
                workload_id.to_string(),
                1000, // Dummy P50
                5000, // Dummy P95
                0,    // No errors
                20,
                "2026-01-15T00:00:00Z".to_string(),
            );
            baselines.push(baseline.clone());

            // Simulate slightly worse current run (but still under threshold)
            let current_p50 = 1010; // +1% (under 2.5% threshold)
            let current_p95 = 5040; // +0.8% (under threshold)

            let analysis = RegressionAnalysis::new(
                workload_id.to_string(),
                current_p50,
                baseline.p50_latency_us,
                current_p95,
                baseline.p95_latency_us,
                0,
                baseline.error_count,
            );
            analyses.push(analysis);
        }

        // All should pass (no regression)
        for analysis in &analyses {
            assert!(
                !analysis.is_regressed,
                "workload {} regressed",
                analysis.workload_id
            );
        }

        // Now simulate one workload having a real regression
        let bad_analysis = RegressionAnalysis::new(
            "protocol-smoke-contract".to_string(),
            10_300, // +3% (over 2.5% threshold)
            10_000,
            50_000,
            50_000,
            0,
            0,
        );

        assert!(bad_analysis.is_regressed);
        assert_eq!(bad_analysis.severity, 1);

        // In Wave 14 CI gate:
        // - If ANY workload regresses with severity >= 1: add comment
        // - If ANY workload regresses with severity >= 3: block PR
    }

    /// Tests error rate regression (critical path).
    #[test]
    fn error_rate_regression_critical() {
        let baseline = BenchmarkBaseline::from_evidence(
            "inventory-reserve-stock".to_string(),
            15_000, // P50
            50_000, // P95
            0,      // No errors expected
            100,    // samples
            "2026-01-15T00:00:00Z".to_string(),
        );

        // Catastrophic: 5 errors appeared
        let analysis = RegressionAnalysis::new(
            "inventory-reserve-stock".to_string(),
            15_100, // P50 slightly worse
            baseline.p50_latency_us,
            50_200, // P95 slightly worse
            baseline.p95_latency_us,
            5, // ERRORS!
            baseline.error_count,
        );

        assert!(analysis.is_regressed);
        assert_eq!(analysis.error_rate_regression_pct, 100.0);
        assert_eq!(analysis.primary_reason, RegressionReason::ErrorRateIncrease);
        assert_eq!(analysis.severity, 3); // Severe!

        // In Wave 14: This would block PR immediately
    }

    /// Tests that Wave 14 can parse and re-emit regression analysis.
    #[test]
    fn regression_analysis_for_observability() {
        let analysis = RegressionAnalysis::new(
            "wal-append-smoke".to_string(),
            20_500, // P50
            20_000, // baseline
            75_000, // P95
            75_000, // baseline
            0,      // errors
            0,      // baseline errors
        );

        // Wave 14 would serialize this for emission to observability system
        let json = analysis.to_json();

        // Verify structure for observability ingestion
        assert!(json.contains("\"workload_id\":\"wal-append-smoke\""));
        assert!(json.contains("\"p50_regression_pct\":2.50")); // Exactly at threshold
        assert!(json.contains("\"is_regressed\":true"));
        assert!(json.contains("\"severity\":1"));
        assert!(json.contains("\"primary_reason\":\"p50-latency-degradation\""));

        // Example emission to DEC-033:
        // POST /events {
        //   "type": "benchmark.regression",
        //   "workload_id": "wal-append-smoke",
        //   "severity": 1,
        //   "reason": "p50-latency-degradation",
        //   "metrics": { ... }
        // }
    }

    /// Tests false positive prevention (valid changes shouldn't trigger regression).
    #[test]
    fn false_positive_prevention() {
        let baseline = BenchmarkBaseline::from_evidence(
            "catalog-resolve-procedure".to_string(),
            0_800, // P50: 800 microseconds
            4_200, // P95: 4.2ms
            0,
            20,
            "2026-01-15T00:00:00Z".to_string(),
        );

        // Run shows 1.5% improvement (should NOT regress)
        let analysis = RegressionAnalysis::new(
            "catalog-resolve-procedure".to_string(),
            787, // P50: 787µs (1.6% improvement)
            baseline.p50_latency_us,
            4_080, // P95: 4.08ms (2.8% improvement)
            baseline.p95_latency_us,
            0,
            baseline.error_count,
        );

        assert!(!analysis.is_regressed);
        assert_eq!(analysis.severity, 0);
        assert_eq!(analysis.primary_reason, RegressionReason::NoRegression);

        // Verify negative regression percentage
        assert!(analysis.p50_regression_pct < 0.0); // Improved!
    }

    /// Tests that baseline JSON is robust to missing fields.
    #[test]
    fn baseline_json_validation() {
        let valid_json = r#"{
            "workload_id": "test",
            "p50_latency_us": 100,
            "p95_latency_us": 500,
            "error_count": 0,
            "sample_count": 20,
            "established_at": "2026-01-15T00:00:00Z"
        }"#;

        let baseline = BenchmarkBaseline::from_json(valid_json).unwrap();
        assert_eq!(baseline.workload_id, "test");
        assert_eq!(baseline.p50_latency_us, 100);

        // Invalid JSON should fail gracefully
        let invalid_json = r#"{"workload_id": "test"}"#;
        let result = BenchmarkBaseline::from_json(invalid_json);
        assert!(result.is_err());
    }

    /// Tests that regression analysis is reproducible across runs.
    #[test]
    fn reproducibility_across_ci_runs() {
        // Simulate two CI runs with identical evidence
        let run1 =
            RegressionAnalysis::new("btree-lookup-smoke".to_string(), 30, 25, 550, 500, 1, 0);

        let run2 =
            RegressionAnalysis::new("btree-lookup-smoke".to_string(), 30, 25, 550, 500, 1, 0);

        // Must be identical
        assert_eq!(run1, run2);
        assert_eq!(run1.is_regressed, run2.is_regressed);
        assert_eq!(run1.severity, run2.severity);
        assert_eq!(run1.primary_reason, run2.primary_reason);

        // JSON should also be identical
        assert_eq!(run1.to_json(), run2.to_json());
    }
}
