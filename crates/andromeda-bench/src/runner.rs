use crate::{
    AUDIT_APPEND_FILE_SINK_HARNESS_NAME, AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE,
    AUDIT_APPEND_FILE_SINK_WORKLOAD_ID, BTREE_NODE_CODEC_HARNESS_NAME,
    BTREE_NODE_CODEC_HARNESS_SOURCE, BTREE_NODE_CODEC_WORKLOAD_ID, BTreeBenchmarkConfig,
    BenchmarkError, BenchmarkEvidence, BenchmarkHardwareProfile, BenchmarkMeasurementMode,
    BenchmarkRunRequest, RECOVERY_REPLAY_WAL_HARNESS_NAME, RECOVERY_REPLAY_WAL_HARNESS_SOURCE,
    RECOVERY_REPLAY_WAL_WORKLOAD_ID, SRPL_COMPILE_OPTIMIZE_HARNESS_NAME,
    SRPL_COMPILE_OPTIMIZE_HARNESS_SOURCE, SRPL_COMPILE_OPTIMIZE_WORKLOAD_ID,
    STORAGE_PAGE_STORE_HARNESS_NAME, STORAGE_PAGE_STORE_HARNESS_SOURCE,
    STORAGE_PAGE_STORE_WORKLOAD_ID, WAL_APPEND_FILE_HARNESS_NAME, WAL_APPEND_FILE_HARNESS_SOURCE,
    WAL_APPEND_FILE_WORKLOAD_ID, benchmark_btree_lookup, benchmark_btree_range_scan,
    compute_percentile, evaluate_budget, run_audit_append_file_sink_smoke_benchmark,
    run_btree_node_codec_smoke_benchmark, run_recovery_replay_wal_smoke_benchmark,
    run_srpl_compile_optimize_smoke_benchmark, run_storage_page_store_smoke_benchmark,
    run_wal_append_file_smoke_benchmark, setup_btree_lookup_harness,
    setup_btree_range_scan_harness,
};

const SYNTHETIC_LATENCY_SOURCE: &str = "deterministic-latency-model";
const SYNTHETIC_MODEL_VERSION: &str = "bounded-diagnostic-v1";
const BTREE_HARNESS_SOURCE: &str = "in-memory-btree-read-harness";
const BTREE_HARNESS_NAME: &str = "MockBTreeIndex";

pub fn run_bounded_benchmark(
    request: &BenchmarkRunRequest,
) -> Result<BenchmarkEvidence, BenchmarkError> {
    let workload = request.validate()?;
    let _profile = request.hardware_profile.materialize();

    let sample_count = request.samples;
    let latency_evidence = match workload.id {
        "btree-lookup-smoke" => run_btree_lookup_harness(request)?,
        "btree-range-scan-smoke" => run_btree_range_scan_harness(request)?,
        BTREE_NODE_CODEC_WORKLOAD_ID => run_btree_node_codec_harness(request)?,
        STORAGE_PAGE_STORE_WORKLOAD_ID => run_storage_page_store_harness(request)?,
        WAL_APPEND_FILE_WORKLOAD_ID => run_wal_append_file_harness(request)?,
        RECOVERY_REPLAY_WAL_WORKLOAD_ID => run_recovery_replay_wal_harness(request)?,
        AUDIT_APPEND_FILE_SINK_WORKLOAD_ID => run_audit_append_file_sink_harness(request)?,
        SRPL_COMPILE_OPTIMIZE_WORKLOAD_ID => run_srpl_compile_optimize_harness(request)?,
        _ => synthetic_latency_evidence(workload.id, request)?,
    };
    let error_count = 0;
    let budget_status = evaluate_budget(
        workload,
        latency_evidence.p50_latency_us,
        latency_evidence.p95_latency_us,
        error_count,
        sample_count,
    )?;
    let requested_iterations = u64::from(request.samples) + u64::from(request.warmups);
    let elapsed_ms = request.duration_ms.min(requested_iterations.max(1));

    Ok(BenchmarkEvidence {
        workload_id: workload.id.to_string(),
        workload_hypothesis: workload.hypothesis,
        workload_shape_version: workload.workload_shape_version,
        workload_size: workload.workload_size,
        primary_metric: workload.primary_metric,
        baseline_ref: workload.baseline_ref,
        budget_origin: workload.budget_origin,
        decision_linkage: workload.decision_linkage,
        hardware_profile: request.hardware_profile,
        duration_ms: request.duration_ms,
        samples: request.samples,
        warmups: request.warmups,
        temp_budget_bytes: request.temp_budget_bytes,
        started_at_unix_ms: 0,
        elapsed_ms,
        sample_count,
        p50_latency_us: latency_evidence.p50_latency_us,
        p95_latency_us: latency_evidence.p95_latency_us,
        error_count,
        budget_status,
        diagnostic_only: true,
        measurement_mode: latency_evidence.measurement_mode,
        latency_source: latency_evidence.latency_source,
        engine_harness: latency_evidence.engine_harness,
        synthetic_model_version: latency_evidence.synthetic_model_version,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LatencyEvidence {
    p50_latency_us: u64,
    p95_latency_us: u64,
    measurement_mode: BenchmarkMeasurementMode,
    latency_source: &'static str,
    engine_harness: Option<&'static str>,
    synthetic_model_version: Option<&'static str>,
}

fn synthetic_latency_evidence(
    workload_id: &str,
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let profile_adjustment_us = match request.hardware_profile {
        BenchmarkHardwareProfile::Conservative => 0,
        BenchmarkHardwareProfile::DeclaredLocal => 1,
    };
    let workload_base_latency_us = match workload_id {
        "vertical-v0-smoke" => 2_500,
        "protocol-smoke-contract" => 1_000,
        "wal-append-smoke" => 1_500,
        _ => return Err(BenchmarkError::UnknownWorkload),
    };

    let p50_latency_us = workload_base_latency_us
        + u64::from(request.samples)
        + u64::from(request.warmups)
        + profile_adjustment_us;
    let p95_latency_us = p50_latency_us * 2 + request.duration_ms / 1_000;

    Ok(LatencyEvidence {
        p50_latency_us,
        p95_latency_us,
        measurement_mode: BenchmarkMeasurementMode::SyntheticDiagnostic,
        latency_source: SYNTHETIC_LATENCY_SOURCE,
        engine_harness: None,
        synthetic_model_version: Some(SYNTHETIC_MODEL_VERSION),
    })
}

fn run_btree_lookup_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let ctx = setup_btree_lookup_harness(BTreeBenchmarkConfig::default())
        .map_err(|_| BenchmarkError::HarnessFailed)?;
    let mut latencies = Vec::with_capacity(request.samples as usize);
    for index in 0..request.samples {
        let key = btree_lookup_key(index);
        latencies
            .push(benchmark_btree_lookup(&ctx, &key).map_err(|_| BenchmarkError::HarnessFailed)?);
    }
    harness_latency_evidence(latencies, BTREE_HARNESS_SOURCE, BTREE_HARNESS_NAME)
}

fn run_btree_range_scan_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let ctx = setup_btree_range_scan_harness(BTreeBenchmarkConfig::default())
        .map_err(|_| BenchmarkError::HarnessFailed)?;
    let mut latencies = Vec::with_capacity(request.samples as usize);
    for index in 0..request.samples {
        let start = btree_range_start_key(index);
        let end = (u64::from(index) + 1_000).to_le_bytes();
        latencies.push(
            benchmark_btree_range_scan(&ctx, &start, &end)
                .map_err(|_| BenchmarkError::HarnessFailed)?,
        );
    }
    harness_latency_evidence(latencies, BTREE_HARNESS_SOURCE, BTREE_HARNESS_NAME)
}

fn run_storage_page_store_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_storage_page_store_smoke_benchmark(request.samples)?;
    harness_latency_evidence(
        result.latencies_us,
        STORAGE_PAGE_STORE_HARNESS_SOURCE,
        STORAGE_PAGE_STORE_HARNESS_NAME,
    )
}

fn run_btree_node_codec_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_btree_node_codec_smoke_benchmark(request.samples)?;
    harness_latency_evidence(
        result.latencies_us,
        BTREE_NODE_CODEC_HARNESS_SOURCE,
        BTREE_NODE_CODEC_HARNESS_NAME,
    )
}

fn run_wal_append_file_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_wal_append_file_smoke_benchmark(request.samples)?;
    harness_latency_evidence(
        result.latencies_us,
        WAL_APPEND_FILE_HARNESS_SOURCE,
        WAL_APPEND_FILE_HARNESS_NAME,
    )
}

fn run_recovery_replay_wal_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_recovery_replay_wal_smoke_benchmark(request.samples)?;
    harness_latency_evidence(
        result.latencies_us,
        RECOVERY_REPLAY_WAL_HARNESS_SOURCE,
        RECOVERY_REPLAY_WAL_HARNESS_NAME,
    )
}

fn run_audit_append_file_sink_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_audit_append_file_sink_smoke_benchmark(request.samples)?;
    harness_latency_evidence(
        result.latencies_us,
        AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE,
        AUDIT_APPEND_FILE_SINK_HARNESS_NAME,
    )
}

fn run_srpl_compile_optimize_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_srpl_compile_optimize_smoke_benchmark(request.samples)?;
    harness_latency_evidence(
        result.latencies_us,
        SRPL_COMPILE_OPTIMIZE_HARNESS_SOURCE,
        SRPL_COMPILE_OPTIMIZE_HARNESS_NAME,
    )
}

fn harness_latency_evidence(
    mut latencies: Vec<u64>,
    latency_source: &'static str,
    engine_harness: &'static str,
) -> Result<LatencyEvidence, BenchmarkError> {
    if latencies.is_empty() {
        return Err(BenchmarkError::InsufficientSamplesForStatistics);
    }
    latencies.sort_unstable();
    Ok(LatencyEvidence {
        p50_latency_us: compute_percentile(&latencies, 50.0),
        p95_latency_us: compute_percentile(&latencies, 95.0),
        measurement_mode: BenchmarkMeasurementMode::HarnessDiagnostic,
        latency_source,
        engine_harness: Some(engine_harness),
        synthetic_model_version: None,
    })
}

fn btree_lookup_key(index: u32) -> [u8; 8] {
    u64::from(index).to_le_bytes()
}

fn btree_range_start_key(index: u32) -> [u8; 8] {
    (u64::from(index) * 10).to_le_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BudgetStatus;

    #[test]
    fn bounded_runner_produces_deterministic_diagnostic_evidence() {
        let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
        request.duration_ms = 1_000;
        request.samples = 5;
        request.warmups = 1;
        request.hardware_profile = BenchmarkHardwareProfile::DeclaredLocal;

        let first = run_bounded_benchmark(&request).unwrap();
        let second = run_bounded_benchmark(&request).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.started_at_unix_ms, 0);
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
        assert_eq!(first.engine_harness, None);
        assert_eq!(first.synthetic_model_version, Some(SYNTHETIC_MODEL_VERSION));
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
        assert!(evidence.p50_latency_us >= 1);
        assert!(evidence.p95_latency_us >= evidence.p50_latency_us);
    }
}
