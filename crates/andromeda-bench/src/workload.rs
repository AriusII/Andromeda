use crate::PerformanceBudget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub workload_class: BenchmarkWorkloadClass,
    pub max_duration_ms: u64,
    pub max_samples: u32,
    pub budget: PerformanceBudget,
}

pub const WORKLOADS: &[BenchmarkWorkload] = &[
    BenchmarkWorkload {
        id: "vertical-v0-smoke",
        description: "synthetic-diagnostic scenario for vertical V0 invocation plus WAL recovery accounting",
        workload_class: BenchmarkWorkloadClass::SyntheticDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 30,
        budget: PerformanceBudget {
            max_p50_latency_us: 50_000,
            max_p95_latency_us: 150_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "protocol-smoke-contract",
        description: "synthetic-diagnostic scenario for protocol contract inspection without network sockets",
        workload_class: BenchmarkWorkloadClass::SyntheticDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
        budget: PerformanceBudget {
            max_p50_latency_us: 10_000,
            max_p95_latency_us: 50_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "wal-append-smoke",
        description: "synthetic-diagnostic scenario for WAL append accounting without file IO",
        workload_class: BenchmarkWorkloadClass::SyntheticDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 30,
        budget: PerformanceBudget {
            max_p50_latency_us: 20_000,
            max_p95_latency_us: 75_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "btree-lookup-smoke",
        description: "harness-diagnostic scenario for B-Tree single-key lookup over a mock read-only key set",
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
        budget: PerformanceBudget {
            max_p50_latency_us: 50,
            max_p95_latency_us: 500,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "btree-range-scan-smoke",
        description: "harness-diagnostic scenario for B-Tree range scan over a mock read-only key subset",
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
        budget: PerformanceBudget {
            max_p50_latency_us: 500,
            max_p95_latency_us: 5_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "btree-node-codec-smoke",
        description: "harness-diagnostic scenario for B-Tree durable node V1 encode/decode over page images",
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
        budget: PerformanceBudget {
            max_p50_latency_us: 2_500,
            max_p95_latency_us: 10_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "storage-page-store-smoke",
        description: "harness-diagnostic scenario for DiskPageStore and BufferPool page write/flush/readback",
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 20,
        budget: PerformanceBudget {
            max_p50_latency_us: 5_000_000,
            max_p95_latency_us: 10_000_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "wal-append-file-smoke",
        description: "harness-diagnostic scenario for file-backed FileWal append and durable flush",
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 20,
        budget: PerformanceBudget {
            max_p50_latency_us: 5_000_000,
            max_p95_latency_us: 10_000_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "recovery-replay-wal-smoke",
        description: "harness-diagnostic scenario for file-backed WAL scan and recovery replay planning",
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 20,
        budget: PerformanceBudget {
            max_p50_latency_us: 5_000_000,
            max_p95_latency_us: 10_000_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "audit-append-file-sink-smoke",
        description: "harness-diagnostic scenario for file-backed durable audit sink append and replay",
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 10_000,
        max_samples: 20,
        budget: PerformanceBudget {
            max_p50_latency_us: 5_000_000,
            max_p95_latency_us: 10_000_000,
            max_error_rate_ppm: 0,
        },
    },
    BenchmarkWorkload {
        id: "srpl-compile-optimize-smoke",
        description: "harness-diagnostic scenario for SRPL parse, lower, and optimize compiler pipeline",
        workload_class: BenchmarkWorkloadClass::HarnessDiagnostic,
        max_duration_ms: 5_000,
        max_samples: 20,
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
            assert!(workload.max_duration_ms <= MAX_DURATION_MS);
            assert!(workload.max_samples <= MAX_SAMPLES);
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
        }
    }
}
