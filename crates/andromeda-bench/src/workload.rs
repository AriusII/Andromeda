use crate::{MAX_DURATION_MS, MAX_SAMPLES, PerformanceBudget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchmarkWorkload {
    pub id: &'static str,
    pub description: &'static str,
    pub max_duration_ms: u64,
    pub max_samples: u32,
    pub budget: PerformanceBudget,
}

pub const WORKLOADS: &[BenchmarkWorkload] = &[
    BenchmarkWorkload {
        id: "vertical-v0-smoke",
        description: "Recoverable vertical-slice invocation and WAL recovery smoke workload",
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
        description: "Local protocol contract inspection workload without opening network sockets",
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
        description: "Bounded WAL append accounting smoke workload",
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
        description: "B-Tree single-key lookup in read-only mode (Wave 13 placeholder; mutations deferred to Wave 18 per DEC-038)",
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
        description: "B-Tree range scan over key subset in read-only mode (Wave 13 placeholder; concurrent mutations deferred to Wave 18 per DEC-038)",
        max_duration_ms: 5_000,
        max_samples: 20,
        budget: PerformanceBudget {
            max_p50_latency_us: 500,
            max_p95_latency_us: 5_000,
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
}
