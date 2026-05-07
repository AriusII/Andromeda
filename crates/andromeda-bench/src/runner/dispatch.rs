use crate::{
    AUDIT_APPEND_FILE_SINK_WORKLOAD_ID, BTREE_NODE_CODEC_WORKLOAD_ID, BenchmarkError,
    BenchmarkRunRequest, RECOVERY_REPLAY_WAL_WORKLOAD_ID, SRPL_COMPILE_OPTIMIZE_WORKLOAD_ID,
    STORAGE_PAGE_STORE_WORKLOAD_ID, WAL_APPEND_FILE_WORKLOAD_ID,
};

use super::harnesses::{
    run_audit_append_file_sink_harness, run_btree_lookup_harness, run_btree_node_codec_harness,
    run_btree_range_scan_harness, run_recovery_replay_wal_harness,
    run_srpl_compile_optimize_harness, run_storage_page_store_harness, run_wal_append_file_harness,
};
use super::latency::LatencyEvidence;
use super::synthetic::synthetic_latency_evidence;

pub(super) fn dispatch_latency_evidence(
    workload_id: &str,
    request: &BenchmarkRunRequest,
) -> Result<LatencyEvidence, BenchmarkError> {
    match workload_id {
        "btree-lookup-smoke" => run_btree_lookup_harness(request),
        "btree-range-scan-smoke" => run_btree_range_scan_harness(request),
        BTREE_NODE_CODEC_WORKLOAD_ID => run_btree_node_codec_harness(request),
        STORAGE_PAGE_STORE_WORKLOAD_ID => run_storage_page_store_harness(request),
        WAL_APPEND_FILE_WORKLOAD_ID => run_wal_append_file_harness(request),
        RECOVERY_REPLAY_WAL_WORKLOAD_ID => run_recovery_replay_wal_harness(request),
        AUDIT_APPEND_FILE_SINK_WORKLOAD_ID => run_audit_append_file_sink_harness(request),
        SRPL_COMPILE_OPTIMIZE_WORKLOAD_ID => run_srpl_compile_optimize_harness(request),
        _ => synthetic_latency_evidence(workload_id, request),
    }
}
