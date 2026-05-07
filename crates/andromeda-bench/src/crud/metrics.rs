use crate::flat_json::escape_json_string;

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

pub(super) fn operation_metrics_json(op: &CrudOperationMetrics) -> String {
    format!(
        "{{\"operation\":\"{}\",\"count\":{},\"total_us\":{},\"p50_us\":{},\"p95_us\":{},\"p99_us\":{},\"throughput_ops_sec\":{:.2},\"error_count\":{}}}",
        escape_json_string(&op.operation),
        op.count,
        op.total_us,
        op.p50_us,
        op.p95_us,
        op.p99_us,
        op.throughput_ops_sec,
        op.error_count
    )
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
