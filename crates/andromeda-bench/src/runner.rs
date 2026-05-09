use crate::{BenchmarkError, BenchmarkEvidence, BenchmarkRunRequest};
use andromeda_bench_harness::run_bounded_benchmark_with_latency_dispatch;

mod dispatch;
mod harnesses;

use dispatch::dispatch_latency_evidence;

pub fn run_bounded_benchmark(
    request: &BenchmarkRunRequest,
) -> Result<BenchmarkEvidence, BenchmarkError> {
    run_bounded_benchmark_with_latency_dispatch(request, dispatch_latency_evidence)
}

#[cfg(test)]
mod tests;
