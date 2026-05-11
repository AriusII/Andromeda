use crate::{
    AUDIT_APPEND_FILE_SINK_HARNESS_NAME, AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE,
    AUDIT_APPEND_FILE_SINK_WORKLOAD_ID, BTREE_NODE_CODEC_HARNESS_NAME,
    BTREE_NODE_CODEC_HARNESS_SOURCE, BTREE_NODE_CODEC_WORKLOAD_ID,
    RECOVERY_REPLAY_WAL_HARNESS_NAME, RECOVERY_REPLAY_WAL_HARNESS_SOURCE,
    RECOVERY_REPLAY_WAL_WORKLOAD_ID, SRPL_COMPILE_OPTIMIZE_HARNESS_NAME,
    SRPL_COMPILE_OPTIMIZE_HARNESS_SOURCE, SRPL_COMPILE_OPTIMIZE_WORKLOAD_ID,
    STORAGE_PAGE_STORE_HARNESS_NAME, STORAGE_PAGE_STORE_HARNESS_SOURCE,
    STORAGE_PAGE_STORE_WORKLOAD_ID, WAL_APPEND_FILE_HARNESS_NAME, WAL_APPEND_FILE_HARNESS_SOURCE,
    WAL_APPEND_FILE_WORKLOAD_ID,
};
use andromeda_bench_harness::{SYNTHETIC_LATENCY_SOURCE, SYNTHETIC_MODEL_VERSION};
use andromeda_bench_workload::{BenchmarkHardwareProfile, BenchmarkRunRequest, BudgetStatus};
use andromeda_scenario_evidence::{
    BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER, BenchmarkMeasurementMode,
};

use super::harnesses::{BTREE_HARNESS_NAME, BTREE_HARNESS_SOURCE};
use super::*;

#[test]
fn bounded_runner_produces_deterministic_diagnostic_evidence() {
    let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
    request.duration_ms = 1_000;
    request.samples = 5;
    request.warmups = 1;
    request.hardware_profile = BenchmarkHardwareProfile::DeclaredLocal;

    let first = run_bounded_benchmark(&request).unwrap();
    let second = run_bounded_benchmark(&request).unwrap();

    // Evidence should have the same structure and values for deterministic fields
    assert_eq!(first.workload_id, second.workload_id);
    assert_eq!(first.elapsed_ms, second.elapsed_ms);
    assert_eq!(first.sample_count, second.sample_count);
    assert_eq!(first.p50_latency_us, second.p50_latency_us);
    assert_eq!(first.p95_latency_us, second.p95_latency_us);

    // Timestamps will be different across runs (now captured at runtime)
    assert!(first.started_at_unix_ms > 0);
    assert!(second.started_at_unix_ms > 0);
    // Andromeda project creation baseline: 2023-11-15 00:00:00 UTC = 1700000000000 ms
    assert!(first.started_at_unix_ms > 1700000000000);

    assert_eq!(first.elapsed_ms, 6);
    assert_eq!(first.sample_count, 5);
    assert!(first.workload_hypothesis.contains("protocol contract"));
    assert_eq!(
        first.workload_shape_version,
        "protocol-smoke-contract.synthetic.v1"
    );
    assert!(first.workload_size.contains("samples<=20"));
    assert_eq!(
        first.primary_metric,
        "p50_latency_us,p95_latency_us,error_rate_ppm"
    );
    assert_eq!(
        first.baseline_ref,
        "history.protocol-smoke-contract.synthetic.v1"
    );
    assert_eq!(first.budget_origin, "static-workload-registry-v1");
    assert!(first.decision_linkage.contains("CatalogVersion"));
    assert_eq!(first.temp_budget_bytes, request.temp_budget_bytes);
    assert_eq!(first.budget_status, BudgetStatus::Passed);
    assert!(first.diagnostic_only);
    assert_eq!(
        first.measurement_mode,
        BenchmarkMeasurementMode::SyntheticDiagnostic
    );
    assert_eq!(first.latency_source, SYNTHETIC_LATENCY_SOURCE);
    assert_eq!(
        first.timing_source,
        BENCHMARK_EVIDENCE_TIMING_SOURCE_DETERMINISTIC_PLACEHOLDER
    );
    assert_eq!(first.engine_harness, None);
    assert_eq!(first.synthetic_model_version, Some(SYNTHETIC_MODEL_VERSION));
    assert_eq!(first.workload_counters.len(), 2);
    assert_eq!(first.workload_counters[0].name, "requested_samples");
    assert_eq!(first.workload_counters[0].value, 5);
}

#[test]
fn btree_workload_uses_read_harness_metadata() {
    let mut request = BenchmarkRunRequest::new("btree-lookup-smoke");
    request.samples = 3;
    request.warmups = 0;

    let evidence = run_bounded_benchmark(&request).unwrap();

    assert!(evidence.diagnostic_only);
    assert_eq!(
        evidence.measurement_mode,
        BenchmarkMeasurementMode::HarnessDiagnostic
    );
    assert_eq!(evidence.latency_source, BTREE_HARNESS_SOURCE);
    assert_eq!(evidence.engine_harness, Some(BTREE_HARNESS_NAME));
    assert_eq!(evidence.synthetic_model_version, None);
    assert_eq!(evidence.workload_counters[0].name, "dataset_keys");
    assert_eq!(evidence.workload_counters[1].name, "lookup_operations");
    assert_eq!(evidence.workload_counters[1].value, 3);
    assert!(evidence.p50_latency_us >= 1);
    assert!(evidence.p95_latency_us >= evidence.p50_latency_us);
}

#[test]
fn storage_workload_uses_disk_page_store_harness_metadata() {
    let mut request = BenchmarkRunRequest::new(STORAGE_PAGE_STORE_WORKLOAD_ID);
    request.samples = 2;
    request.warmups = 0;

    let evidence = run_bounded_benchmark(&request).unwrap();

    assert!(evidence.diagnostic_only);
    assert_eq!(
        evidence.measurement_mode,
        BenchmarkMeasurementMode::HarnessDiagnostic
    );
    assert_eq!(evidence.latency_source, STORAGE_PAGE_STORE_HARNESS_SOURCE);
    assert_eq!(
        evidence.engine_harness,
        Some(STORAGE_PAGE_STORE_HARNESS_NAME)
    );
    assert_eq!(evidence.synthetic_model_version, None);
    assert_eq!(evidence.workload_counters[0].name, "flushed_pages");
    assert_eq!(evidence.workload_counters[0].value, 2);
    assert_eq!(evidence.workload_counters[1].name, "readback_pages");
    assert_eq!(evidence.workload_counters[1].value, 2);
    assert!(evidence.p50_latency_us >= 1);
    assert!(evidence.p95_latency_us >= evidence.p50_latency_us);
}

#[test]
fn btree_node_codec_workload_uses_storage_codec_harness_metadata() {
    let mut request = BenchmarkRunRequest::new(BTREE_NODE_CODEC_WORKLOAD_ID);
    request.samples = 2;
    request.warmups = 0;

    let evidence = run_bounded_benchmark(&request).unwrap();

    assert!(evidence.diagnostic_only);
    assert_eq!(
        evidence.measurement_mode,
        BenchmarkMeasurementMode::HarnessDiagnostic
    );
    assert_eq!(evidence.latency_source, BTREE_NODE_CODEC_HARNESS_SOURCE);
    assert_eq!(evidence.engine_harness, Some(BTREE_NODE_CODEC_HARNESS_NAME));
    assert_eq!(evidence.synthetic_model_version, None);
    assert_eq!(evidence.workload_counters[0].name, "encoded_pages");
    assert_eq!(evidence.workload_counters[0].value, 2);
    assert_eq!(evidence.workload_counters[1].name, "decoded_pages");
    assert_eq!(evidence.workload_counters[1].value, 2);
    assert!(evidence.p50_latency_us >= 1);
    assert!(evidence.p95_latency_us >= evidence.p50_latency_us);
}

#[test]
fn wal_append_file_workload_uses_file_wal_harness_metadata() {
    let mut request = BenchmarkRunRequest::new(WAL_APPEND_FILE_WORKLOAD_ID);
    request.samples = 2;
    request.warmups = 0;

    let evidence = run_bounded_benchmark(&request).unwrap();

    assert!(evidence.diagnostic_only);
    assert_eq!(
        evidence.measurement_mode,
        BenchmarkMeasurementMode::HarnessDiagnostic
    );
    assert_eq!(evidence.latency_source, WAL_APPEND_FILE_HARNESS_SOURCE);
    assert_eq!(evidence.engine_harness, Some(WAL_APPEND_FILE_HARNESS_NAME));
    assert_eq!(evidence.synthetic_model_version, None);
    assert_eq!(evidence.workload_counters[0].name, "appended_records");
    assert_eq!(evidence.workload_counters[0].value, 6);
    assert_eq!(evidence.workload_counters[1].name, "durable_lsn");
    assert_eq!(evidence.workload_counters[1].value, 6);
    assert!(evidence.p50_latency_us >= 1);
    assert!(evidence.p95_latency_us >= evidence.p50_latency_us);
}

#[test]
fn recovery_replay_wal_workload_uses_file_wal_harness_metadata() {
    let mut request = BenchmarkRunRequest::new(RECOVERY_REPLAY_WAL_WORKLOAD_ID);
    request.samples = 2;
    request.warmups = 0;

    let evidence = run_bounded_benchmark(&request).unwrap();

    assert!(evidence.diagnostic_only);
    assert_eq!(
        evidence.measurement_mode,
        BenchmarkMeasurementMode::HarnessDiagnostic
    );
    assert_eq!(evidence.latency_source, RECOVERY_REPLAY_WAL_HARNESS_SOURCE);
    assert_eq!(
        evidence.engine_harness,
        Some(RECOVERY_REPLAY_WAL_HARNESS_NAME)
    );
    assert_eq!(evidence.synthetic_model_version, None);
    assert_eq!(evidence.workload_counters[0].name, "replay_records");
    assert_eq!(evidence.workload_counters[0].value, 1);
    assert_eq!(
        evidence.workload_counters[1].name,
        "recovered_transaction_id_floor"
    );
    assert!(evidence.p50_latency_us >= 1);
    assert!(evidence.p95_latency_us >= evidence.p50_latency_us);
}

#[test]
fn audit_append_file_sink_workload_uses_observe_file_sink_harness_metadata() {
    let mut request = BenchmarkRunRequest::new(AUDIT_APPEND_FILE_SINK_WORKLOAD_ID);
    request.samples = 2;
    request.warmups = 0;

    let evidence = run_bounded_benchmark(&request).unwrap();

    assert!(evidence.diagnostic_only);
    assert_eq!(
        evidence.measurement_mode,
        BenchmarkMeasurementMode::HarnessDiagnostic
    );
    assert_eq!(
        evidence.latency_source,
        AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE
    );
    assert_eq!(
        evidence.engine_harness,
        Some(AUDIT_APPEND_FILE_SINK_HARNESS_NAME)
    );
    assert_eq!(evidence.synthetic_model_version, None);
    assert_eq!(evidence.workload_counters[0].name, "appended_records");
    assert_eq!(evidence.workload_counters[0].value, 2);
    assert_eq!(evidence.workload_counters[1].name, "replayed_records");
    assert_eq!(evidence.workload_counters[1].value, 2);
    assert_eq!(evidence.workload_counters[2].name, "durable_lsn");
    assert!(evidence.p50_latency_us >= 1);
    assert!(evidence.p95_latency_us >= evidence.p50_latency_us);
}

#[test]
fn srpl_compile_optimize_workload_uses_compiler_pipeline_metadata() {
    let mut request = BenchmarkRunRequest::new(SRPL_COMPILE_OPTIMIZE_WORKLOAD_ID);
    request.samples = 2;
    request.warmups = 0;

    let evidence = run_bounded_benchmark(&request).unwrap();

    assert!(evidence.diagnostic_only);
    assert_eq!(
        evidence.measurement_mode,
        BenchmarkMeasurementMode::HarnessDiagnostic
    );
    assert_eq!(
        evidence.latency_source,
        SRPL_COMPILE_OPTIMIZE_HARNESS_SOURCE
    );
    assert_eq!(
        evidence.engine_harness,
        Some(SRPL_COMPILE_OPTIMIZE_HARNESS_NAME)
    );
    assert_eq!(evidence.synthetic_model_version, None);
    assert_eq!(evidence.workload_counters[0].name, "compiled_procedures");
    assert_eq!(evidence.workload_counters[0].value, 2);
    assert_eq!(evidence.workload_counters[1].name, "optimized_procedures");
    assert_eq!(evidence.workload_counters[1].value, 2);
    assert_eq!(evidence.workload_counters[2].name, "optimizer_diagnostics");
    assert!(evidence.workload_counters[2].value >= 3);
    assert!(evidence.p50_latency_us >= 1);
    assert!(evidence.p95_latency_us >= evidence.p50_latency_us);
}
