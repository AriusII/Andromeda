use crate::{DEFAULT_TEMP_BYTES, MAX_TEMP_BYTES, PerformanceBudget};

pub const BENCHMARK_PRIMARY_METRIC: &str = "p50_latency_us,p95_latency_us,error_rate_ppm";
pub const BENCHMARK_BUDGET_ORIGIN: &str = "static-workload-registry-v1";
pub const BENCHMARK_DECISION_LINKAGE: &str =
    "advisory-only; requires ProcedureId+CatalogVersion+ContractHash+StatsVersion+PlanClass";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BenchmarkWorkloadClass {
    SyntheticDiagnostic,
    HarnessDiagnostic,
    RealRuntime,
}

impl BenchmarkWorkloadClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SyntheticDiagnostic => "synthetic-diagnostic",
            Self::HarnessDiagnostic => "harness-diagnostic",
            Self::RealRuntime => "real-runtime",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchmarkWorkload {
    pub id: &'static str,
    pub description: &'static str,
    pub hypothesis: &'static str,
    pub workload_shape_version: &'static str,
    pub workload_size: &'static str,
    pub primary_metric: &'static str,
    pub baseline_ref: &'static str,
    pub budget_origin: &'static str,
    pub decision_linkage: &'static str,
    pub workload_class: BenchmarkWorkloadClass,
    pub max_duration_ms: u64,
    pub max_samples: u32,
    pub max_temp_bytes: u64,
    pub budget: PerformanceBudget,
}

pub const WORKLOADS: &[BenchmarkWorkload] = &[
    BenchmarkWorkload {
        id: "vertical-v0-smoke",
        description: "synthetic-diagnostic scenario for vertical V0 invocation plus WAL recovery accounting",
        hypothesis: "bounded vertical invocation accounting should remain under diagnostic latency and error budgets",
        workload_shape_version: "vertical-v0-smoke.synthetic.v1",
        workload_size: "single synthetic invocation path, samples<=30, duration_ms<=10000",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.vertical-v0-smoke.synthetic.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::SyntheticDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 30,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 50_000,
            max_p95_latency_us: 150_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "protocol-smoke-contract",
        description: "synthetic-diagnostic scenario for protocol contract inspection without network sockets",
        hypothesis: "typed protocol contract inspection should stay bounded without opening a network surface",
        workload_shape_version: "protocol-smoke-contract.synthetic.v1",
        workload_size: "contract-only synthetic inspection, samples<=20, duration_ms<=5000",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.protocol-smoke-contract.synthetic.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::SyntheticDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
        max_temp_bytes: DEFAULT_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 10_000,
            max_p95_latency_us: 50_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "wal-append-smoke",
        description: "synthetic-diagnostic scenario for WAL append accounting without file IO",
        hypothesis: "synthetic WAL append accounting should remain bounded before file-backed durability checks run",
        workload_shape_version: "wal-append-smoke.synthetic.v1",
        workload_size: "synthetic WAL accounting path, samples<=30, duration_ms<=10000",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.wal-append-smoke.synthetic.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::SyntheticDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 30,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 20_000,
            max_p95_latency_us: 75_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "btree-lookup-smoke",
        description: "harness-diagnostic scenario for B-Tree single-key lookup over a mock read-only key set",
        hypothesis: "mock B-Tree single-key lookup latency should remain stable over the bounded read-only key set",
        workload_shape_version: "btree-lookup-smoke.harness.v1",
        workload_size: "mock read-only key set, single-key lookup, samples<=20",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.btree-lookup-smoke.harness.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 50,
            max_p95_latency_us: 500,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "btree-range-scan-smoke",
        description: "harness-diagnostic scenario for B-Tree range scan over a mock read-only key subset",
        hypothesis: "mock B-Tree range scan latency should remain stable over bounded key ranges",
        workload_shape_version: "btree-range-scan-smoke.harness.v1",
        workload_size: "mock read-only key subset, bounded range scan, samples<=20",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.btree-range-scan-smoke.harness.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 500,
            max_p95_latency_us: 5_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "btree-node-codec-smoke",
        description: "harness-diagnostic scenario for B-Tree durable node V1 encode/decode over page images",
        hypothesis: "B-Tree durable node V1 encode/decode should remain stable for bounded page images",
        workload_shape_version: "btree-node-codec-smoke.harness.v1",
        workload_size: "durable node V1 page images, samples<=20, duration_ms<=5000",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.btree-node-codec-smoke.harness.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 2_500,
            max_p95_latency_us: 10_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "storage-page-store-smoke",
        description: "harness-diagnostic scenario for DiskPageStore and BufferPool page write/flush/readback",
        hypothesis: "page write, flush, and readback should remain stable for bounded page-store smoke runs",
        workload_shape_version: "storage-page-store-smoke.harness.v1",
        workload_size: "DiskPageStore and BufferPool page write/flush/readback, samples<=20",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.storage-page-store-smoke.harness.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 20,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 5_000_000,
            max_p95_latency_us: 10_000_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "wal-append-file-smoke",
        description: "harness-diagnostic scenario for file-backed FileWal append and durable flush",
        hypothesis: "file-backed WAL append plus durable flush should remain stable under smoke budgets",
        workload_shape_version: "wal-append-file-smoke.harness.v1",
        workload_size: "file-backed WAL append and flush, samples<=20, duration_ms<=10000",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.wal-append-file-smoke.harness.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 20,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 5_000_000,
            max_p95_latency_us: 10_000_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "recovery-replay-wal-smoke",
        description: "harness-diagnostic scenario for file-backed WAL scan and recovery replay planning",
        hypothesis: "file-backed WAL scan and recovery replay planning should remain stable under smoke budgets",
        workload_shape_version: "recovery-replay-wal-smoke.harness.v1",
        workload_size: "file-backed WAL scan and recovery replay planning, samples<=20",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.recovery-replay-wal-smoke.harness.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 20,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 5_000_000,
            max_p95_latency_us: 10_000_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "audit-append-file-sink-smoke",
        description: "harness-diagnostic scenario for file-backed durable audit sink append and replay",
        hypothesis: "durable audit sink append and replay should remain stable and observable under smoke budgets",
        workload_shape_version: "audit-append-file-sink-smoke.harness.v1",
        workload_size: "file-backed durable audit sink append and replay, samples<=20",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.audit-append-file-sink-smoke.harness.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 20,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 5_000_000,
            max_p95_latency_us: 10_000_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "srpl-compile-optimize-smoke",
        description: "harness-diagnostic scenario for SRPL parse, lower, and optimize compiler pipeline",
        hypothesis: "SRPL parse, lower, and optimize pipeline latency should remain stable for bounded Procedure signatures",
        workload_shape_version: "srpl-compile-optimize-smoke.harness.v1",
        workload_size: "bounded SRPL Procedure compiler pipeline, samples<=20, duration_ms<=5000",
        primary_metric: BENCHMARK_PRIMARY_METRIC,
        baseline_ref: "history.srpl-compile-optimize-smoke.harness.v1",
        budget_origin: BENCHMARK_BUDGET_ORIGIN,
        decision_linkage: BENCHMARK_DECISION_LINKAGE,
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
        max_temp_bytes: MAX_TEMP_BYTES,
        budget: PerformanceBudget {
            max_p50_latency_us: 1_000_000,
            max_p95_latency_us: 5_000_000,
            max_error_rate_ppm: 0,
        },
    },
];

pub fn find_workload(id: &str) -> Option<&'static BenchmarkWorkload> {
    WORKLOADS.iter().find(|workload| workload.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MAX_DURATION_MS, MAX_SAMPLES};

    #[test]
    fn workload_ids_are_unique_and_bounded() {
        for (index, workload) in WORKLOADS.iter().enumerate() {
            assert!(!workload.id.is_empty());
            assert!(!workload.hypothesis.is_empty());
            assert!(!workload.workload_shape_version.is_empty());
            assert!(!workload.workload_size.is_empty());
            assert_eq!(workload.primary_metric, BENCHMARK_PRIMARY_METRIC);
            assert!(!workload.baseline_ref.is_empty());
            assert_eq!(workload.budget_origin, BENCHMARK_BUDGET_ORIGIN);
            assert_eq!(workload.decision_linkage, BENCHMARK_DECISION_LINKAGE);
            assert!(workload.max_duration_ms <= MAX_DURATION_MS);
            assert!(workload.max_samples <= MAX_SAMPLES);
            assert!(workload.max_temp_bytes <= MAX_TEMP_BYTES);
            assert!(
                WORKLOADS[index + 1..]
                    .iter()
                    .all(|other| other.id != workload.id)
            );
        }
    }

    #[test]
    fn representative_harness_workloads_are_registered() {
        assert!(find_workload("wal-append-file-smoke").is_some());
        assert!(find_workload("recovery-replay-wal-smoke").is_some());
        assert!(find_workload("audit-append-file-sink-smoke").is_some());
        assert!(find_workload("srpl-compile-optimize-smoke").is_some());
        assert!(find_workload("btree-node-codec-smoke").is_some());
        assert!(find_workload("storage-page-store-smoke").is_some());
    }

    #[test]
    fn workload_descriptions_expose_measurement_class() {
        for workload in WORKLOADS {
            assert!(
                workload
                    .description
                    .contains(workload.workload_class.as_str()),
                "workload {} description must include {}",
                workload.id,
                workload.workload_class.as_str()
            );
            assert!(
                workload
                    .workload_shape_version
                    .contains(workload.workload_class.as_str().split('-').next().unwrap()),
                "workload {} shape version must expose its measurement class family",
                workload.id
            );
        }
    }
}
