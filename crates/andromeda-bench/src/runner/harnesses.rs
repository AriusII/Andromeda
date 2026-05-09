use crate::{
    AUDIT_APPEND_FILE_SINK_HARNESS_NAME, AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE,
    BTREE_NODE_CODEC_HARNESS_NAME, BTREE_NODE_CODEC_HARNESS_SOURCE, BTreeBenchmarkConfig,
    BenchmarkError, BenchmarkRunRequest, BenchmarkWorkloadCounter,
    RECOVERY_REPLAY_WAL_HARNESS_NAME, RECOVERY_REPLAY_WAL_HARNESS_SOURCE,
    SRPL_COMPILE_OPTIMIZE_HARNESS_NAME, SRPL_COMPILE_OPTIMIZE_HARNESS_SOURCE,
    STORAGE_PAGE_STORE_HARNESS_NAME, STORAGE_PAGE_STORE_HARNESS_SOURCE,
    WAL_APPEND_FILE_HARNESS_NAME, WAL_APPEND_FILE_HARNESS_SOURCE, benchmark_btree_lookup,
    benchmark_btree_range_scan, run_audit_append_file_sink_smoke_benchmark,
    run_btree_node_codec_smoke_benchmark, run_recovery_replay_wal_smoke_benchmark,
    run_srpl_compile_optimize_smoke_benchmark, run_storage_page_store_smoke_benchmark,
    run_wal_append_file_smoke_benchmark, setup_btree_lookup_harness,
    setup_btree_range_scan_harness,
};
use andromeda_bench_harness::{
    LatencyEvidence, counter_from_usize, counters_from_usize, harness_latency_evidence,
};

pub(super) const BTREE_HARNESS_SOURCE: &str = "in-memory-btree-read-harness";
pub(super) const BTREE_HARNESS_NAME: &str = "MockBTreeIndex";

pub(super) fn run_btree_lookup_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let ctx = setup_btree_lookup_harness(BTreeBenchmarkConfig::default())
        .map_err(|_| BenchmarkError::HarnessFailed)?;
    let dataset_size =
        u64::try_from(ctx.config().dataset_size).map_err(|_| BenchmarkError::HarnessFailed)?;
    let mut latencies = Vec::with_capacity(request.samples as usize);
    for index in 0..request.samples {
        let key = btree_lookup_key(index);
        latencies
            .push(benchmark_btree_lookup(&ctx, &key).map_err(|_| BenchmarkError::HarnessFailed)?);
    }
    harness_latency_evidence(
        latencies,
        BTREE_HARNESS_SOURCE,
        BTREE_HARNESS_NAME,
        vec![
            BenchmarkWorkloadCounter::new("dataset_keys", dataset_size, "keys"),
            BenchmarkWorkloadCounter::new("lookup_operations", u64::from(request.samples), "ops"),
        ],
    )
}

pub(super) fn run_btree_range_scan_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let ctx = setup_btree_range_scan_harness(BTreeBenchmarkConfig::default())
        .map_err(|_| BenchmarkError::HarnessFailed)?;
    let dataset_size =
        u64::try_from(ctx.config().dataset_size).map_err(|_| BenchmarkError::HarnessFailed)?;
    let mut latencies = Vec::with_capacity(request.samples as usize);
    for index in 0..request.samples {
        let start = btree_range_start_key(index);
        let end = (u64::from(index) + 1_000).to_le_bytes();
        latencies.push(
            benchmark_btree_range_scan(&ctx, &start, &end)
                .map_err(|_| BenchmarkError::HarnessFailed)?,
        );
    }
    harness_latency_evidence(
        latencies,
        BTREE_HARNESS_SOURCE,
        BTREE_HARNESS_NAME,
        vec![
            BenchmarkWorkloadCounter::new("dataset_keys", dataset_size, "keys"),
            BenchmarkWorkloadCounter::new(
                "range_scan_operations",
                u64::from(request.samples),
                "ops",
            ),
        ],
    )
}

pub(super) fn run_storage_page_store_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_storage_page_store_smoke_benchmark(request.samples)?;
    let counters = counters_from_usize(&[
        ("flushed_pages", result.flushed_pages, "pages"),
        ("readback_pages", result.readback_pages, "pages"),
    ])?;
    harness_latency_evidence(
        result.latencies_us,
        STORAGE_PAGE_STORE_HARNESS_SOURCE,
        STORAGE_PAGE_STORE_HARNESS_NAME,
        counters,
    )
}

pub(super) fn run_btree_node_codec_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_btree_node_codec_smoke_benchmark(request.samples)?;
    let counters = counters_from_usize(&[
        ("encoded_pages", result.encoded_pages, "pages"),
        ("decoded_pages", result.decoded_pages, "pages"),
    ])?;
    harness_latency_evidence(
        result.latencies_us,
        BTREE_NODE_CODEC_HARNESS_SOURCE,
        BTREE_NODE_CODEC_HARNESS_NAME,
        counters,
    )
}

pub(super) fn run_wal_append_file_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_wal_append_file_smoke_benchmark(request.samples)?;
    let counters = vec![
        counter_from_usize("appended_records", result.appended_records, "records")?,
        BenchmarkWorkloadCounter::new("durable_lsn", result.durable_lsn.get(), "lsn"),
    ];
    harness_latency_evidence(
        result.latencies_us,
        WAL_APPEND_FILE_HARNESS_SOURCE,
        WAL_APPEND_FILE_HARNESS_NAME,
        counters,
    )
}

pub(super) fn run_recovery_replay_wal_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_recovery_replay_wal_smoke_benchmark(request.samples)?;
    let counters = vec![
        counter_from_usize("replay_records", result.replay_records, "records")?,
        BenchmarkWorkloadCounter::new(
            "recovered_transaction_id_floor",
            result.recovered_transaction_id_floor,
            "transaction-id",
        ),
    ];
    harness_latency_evidence(
        result.latencies_us,
        RECOVERY_REPLAY_WAL_HARNESS_SOURCE,
        RECOVERY_REPLAY_WAL_HARNESS_NAME,
        counters,
    )
}

pub(super) fn run_audit_append_file_sink_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_audit_append_file_sink_smoke_benchmark(request.samples)?;
    let counters = vec![
        counter_from_usize("appended_records", result.appended_records, "records")?,
        counter_from_usize("replayed_records", result.replayed_records, "records")?,
        BenchmarkWorkloadCounter::new("durable_lsn", result.durable_lsn, "lsn"),
    ];
    harness_latency_evidence(
        result.latencies_us,
        AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE,
        AUDIT_APPEND_FILE_SINK_HARNESS_NAME,
        counters,
    )
}

pub(super) fn run_srpl_compile_optimize_harness(
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    let result = run_srpl_compile_optimize_smoke_benchmark(request.samples)?;
    let counters = counters_from_usize(&[
        (
            "compiled_procedures",
            result.compiled_procedures,
            "procedures",
        ),
        (
            "optimized_procedures",
            result.optimized_procedures,
            "procedures",
        ),
        (
            "optimizer_diagnostics",
            result.optimizer_diagnostics,
            "diagnostics",
        ),
    ])?;
    harness_latency_evidence(
        result.latencies_us,
        SRPL_COMPILE_OPTIMIZE_HARNESS_SOURCE,
        SRPL_COMPILE_OPTIMIZE_HARNESS_NAME,
        counters,
    )
}

fn btree_lookup_key(index: u32) -> [u8; 8] {
    u64::from(index).to_le_bytes()
}

fn btree_range_start_key(index: u32) -> [u8; 8] {
    (u64::from(index) * 10).to_le_bytes()
}
