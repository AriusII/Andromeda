use crate::error::cli_error;
use andromeda_bench::{
    BenchmarkError, BenchmarkRunRequest, MAX_DURATION_MS, MAX_SAMPLES, MAX_WARMUPS,
};

pub(super) fn benchmark_error_to_cli_error(
    error: BenchmarkError,
    request: &BenchmarkRunRequest,
) -> andromeda_core::AndromedaError {
    let message = match error {
        BenchmarkError::EmptyWorkloadId => {
            "benchmark workload identifier must not be empty".to_string()
        }
        BenchmarkError::UnknownWorkload => {
            "unknown benchmark workload; run `andromeda-cli benchmark workloads`".to_string()
        }
        BenchmarkError::ZeroDuration => "--duration-ms must be greater than zero".to_string(),
        BenchmarkError::ZeroSamples => "--samples must be greater than zero".to_string(),
        BenchmarkError::DurationExceedsGlobalLimit => {
            format!("--duration-ms must be <= {MAX_DURATION_MS}")
        }
        BenchmarkError::SamplesExceedsGlobalLimit => {
            format!("--samples must be <= {MAX_SAMPLES}")
        }
        BenchmarkError::WarmupsExceedsGlobalLimit => {
            format!("--warmups must be <= {MAX_WARMUPS}")
        }
        BenchmarkError::DurationExceedsWorkloadLimit => {
            match andromeda_bench::find_workload(&request.workload_id) {
                Some(workload) => format!(
                    "workload `{}` duration must be <= {} ms",
                    workload.id, workload.max_duration_ms
                ),
                None => "benchmark workload duration exceeds its bounded limit".to_string(),
            }
        }
        BenchmarkError::SamplesExceedsWorkloadLimit => {
            match andromeda_bench::find_workload(&request.workload_id) {
                Some(workload) => format!(
                    "workload `{}` samples must be <= {}",
                    workload.id, workload.max_samples
                ),
                None => "benchmark workload samples exceed its bounded limit".to_string(),
            }
        }
        BenchmarkError::InsufficientSamplesForStatistics => {
            "benchmark runner produced no samples".to_string()
        }
        BenchmarkError::HarnessFailed => "benchmark workload harness failed".to_string(),
    };
    cli_error(message)
}
