/// Metrics collected for a single operation type.
#[derive(Debug, Clone, PartialEq)]
pub struct CrudOperationMetrics {
    pub operation: String,
    pub count: u64,
    pub total_us: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub throughput_ops_sec: f64,
    pub error_count: u32,
}

/// Computes percentiles from a sorted slice of latencies.
pub fn compute_percentile(sorted_latencies: &[u64], percentile: f64) -> u64 {
    if sorted_latencies.is_empty() {
        return 0;
    }
    let rank = (percentile / 100.0 * sorted_latencies.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1);
    sorted_latencies[idx.min(sorted_latencies.len() - 1)]
}
