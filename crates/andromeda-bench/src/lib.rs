#![forbid(unsafe_code)]

//! Bounded benchmark contracts for Andromeda operational diagnostics.
//!
//! This crate executes bounded benchmark smoke runs. Workload and evidence
//! contracts are owned by `andromeda-bench-workload` and
//! `andromeda-scenario-evidence`.

mod audit_file_benchmark;
mod btree_benchmark;
mod btree_node_codec_benchmark;
mod srpl_compiler_benchmark;
mod wal_file_benchmark;

mod runner;
mod storage_runtime_benchmark;

pub use audit_file_benchmark::{
    AUDIT_APPEND_FILE_SINK_HARNESS_NAME, AUDIT_APPEND_FILE_SINK_HARNESS_SOURCE,
    AUDIT_APPEND_FILE_SINK_WORKLOAD_ID, AuditAppendFileSinkSmokeBenchmark,
    run_audit_append_file_sink_smoke_benchmark,
};

pub use btree_benchmark::{
    BTreeBenchmarkConfig, BTreeBenchmarkContext, BTreeBenchmarkError, benchmark_btree_lookup,
    benchmark_btree_range_scan, setup_btree_lookup_harness, setup_btree_range_scan_harness,
};
pub use btree_node_codec_benchmark::{
    BTREE_NODE_CODEC_HARNESS_NAME, BTREE_NODE_CODEC_HARNESS_SOURCE, BTREE_NODE_CODEC_WORKLOAD_ID,
    BTreeNodeCodecSmokeBenchmark, run_btree_node_codec_smoke_benchmark,
};

pub use runner::run_bounded_benchmark;
pub use srpl_compiler_benchmark::{
    SRPL_COMPILE_OPTIMIZE_HARNESS_NAME, SRPL_COMPILE_OPTIMIZE_HARNESS_SOURCE,
    SRPL_COMPILE_OPTIMIZE_WORKLOAD_ID, SrplCompileOptimizeSmokeBenchmark,
    run_srpl_compile_optimize_smoke_benchmark,
};
pub use storage_runtime_benchmark::{
    STORAGE_PAGE_STORE_HARNESS_NAME, STORAGE_PAGE_STORE_HARNESS_SOURCE,
    STORAGE_PAGE_STORE_WORKLOAD_ID, StoragePageStoreSmokeBenchmark,
    run_storage_page_store_smoke_benchmark,
};
pub use wal_file_benchmark::{
    RECOVERY_REPLAY_WAL_HARNESS_NAME, RECOVERY_REPLAY_WAL_HARNESS_SOURCE,
    RECOVERY_REPLAY_WAL_WORKLOAD_ID, RecoveryReplayWalSmokeBenchmark, WAL_APPEND_FILE_HARNESS_NAME,
    WAL_APPEND_FILE_HARNESS_SOURCE, WAL_APPEND_FILE_WORKLOAD_ID, WalAppendFileSmokeBenchmark,
    run_recovery_replay_wal_smoke_benchmark, run_wal_append_file_smoke_benchmark,
};
