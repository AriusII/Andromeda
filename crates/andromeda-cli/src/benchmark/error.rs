use crate::error::cli_error;
use andromeda_bench_workload::{
    BenchmarkError, BenchmarkRunRequest, MAX_DURATION_MS, MAX_SAMPLES, MAX_TEMP_BYTES, MAX_WARMUPS,
    find_workload,
};

pub(super) fn benchmark_error_to_cli_error(
    error: BenchmarkError,
    request: &BenchmarkRunRequest,
) -> andromeda_error::AndromedaError {
    let message = match error {
        BenchmarkError::EmptyWorkloadId => {
            "benchmark workload identifier must not be empty".to_string()
        },
        BenchmarkError::UnknownWorkload => {
            "unknown benchmark workload; run `andromeda-cli benchmark workloads`".to_string()
        },
        BenchmarkError::ZeroDuration => "--duration-ms must be greater than zero".to_string(),
        BenchmarkError::ZeroSamples => "--samples must be greater than zero".to_string(),
        BenchmarkError::DurationExceedsGlobalLimit => {
            format!("--duration-ms must be <= {MAX_DURATION_MS}")
        },
        BenchmarkError::SamplesExceedsGlobalLimit => {
            format!("--samples must be <= {MAX_SAMPLES}")
        },
        BenchmarkError::WarmupsExceedsGlobalLimit => {
            format!("--warmups must be <= {MAX_WARMUPS}")
        },
        BenchmarkError::ZeroTempBudget => {
            "--temp-budget-bytes must be greater than zero".to_string()
        },
        BenchmarkError::TempBudgetExceedsGlobalLimit => {
            format!("--temp-budget-bytes must be <= {MAX_TEMP_BYTES}")
        },
        BenchmarkError::DurationExceedsWorkloadLimit => match find_workload(&request.workload_id) {
            Some(workload) => format!(
                "workload `{}` duration must be <= {} ms",
                workload.id, workload.max_duration_ms
            ),
            None => "benchmark workload duration exceeds its bounded limit".to_string(),
        },
        BenchmarkError::SamplesExceedsWorkloadLimit => match find_workload(&request.workload_id) {
            Some(workload) => format!(
                "workload `{}` samples must be <= {}",
                workload.id, workload.max_samples
            ),
            None => "benchmark workload samples exceed its bounded limit".to_string(),
        },
        BenchmarkError::TempBudgetExceedsWorkloadLimit => {
            match find_workload(&request.workload_id) {
                Some(workload) => format!(
                    "workload `{}` temp budget must be <= {} bytes",
                    workload.id, workload.max_temp_bytes
                ),
                None => "benchmark workload temp budget exceeds its bounded limit".to_string(),
            }
        },
        BenchmarkError::InsufficientSamplesForStatistics => {
            "benchmark runner produced no samples".to_string()
        },
        BenchmarkError::ErrorCountExceedsSamples => {
            "benchmark runner produced more errors than samples".to_string()
        },
        BenchmarkError::HarnessFailed => "benchmark workload harness failed".to_string(),
    };
    cli_error(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> BenchmarkRunRequest {
        BenchmarkRunRequest::new("protocol-smoke-contract")
    }

    #[test]
    fn maps_integrity_error_without_echoing_input_values() {
        let request = request();

        let error =
            benchmark_error_to_cli_error(BenchmarkError::ErrorCountExceedsSamples, &request);

        assert_eq!(
            error.message(),
            "benchmark runner produced more errors than samples"
        );
        assert!(!error.message().contains("protocol-smoke-contract"));
    }

    #[test]
    fn maps_temp_budget_errors_to_bounded_messages() {
        let request = request();

        assert_eq!(
            benchmark_error_to_cli_error(BenchmarkError::ZeroTempBudget, &request).message(),
            "--temp-budget-bytes must be greater than zero"
        );
        assert_eq!(
            benchmark_error_to_cli_error(BenchmarkError::TempBudgetExceedsGlobalLimit, &request)
                .message(),
            format!("--temp-budget-bytes must be <= {MAX_TEMP_BYTES}")
        );
        assert_eq!(
            benchmark_error_to_cli_error(BenchmarkError::TempBudgetExceedsWorkloadLimit, &request)
                .message(),
            "workload `protocol-smoke-contract` temp budget must be <= 8388608 bytes"
        );
    }

    #[test]
    fn maps_every_benchmark_error_variant_to_message() {
        let request = request();
        let errors = [
            BenchmarkError::EmptyWorkloadId,
            BenchmarkError::UnknownWorkload,
            BenchmarkError::ZeroDuration,
            BenchmarkError::ZeroSamples,
            BenchmarkError::DurationExceedsGlobalLimit,
            BenchmarkError::SamplesExceedsGlobalLimit,
            BenchmarkError::WarmupsExceedsGlobalLimit,
            BenchmarkError::ZeroTempBudget,
            BenchmarkError::TempBudgetExceedsGlobalLimit,
            BenchmarkError::DurationExceedsWorkloadLimit,
            BenchmarkError::SamplesExceedsWorkloadLimit,
            BenchmarkError::TempBudgetExceedsWorkloadLimit,
            BenchmarkError::InsufficientSamplesForStatistics,
            BenchmarkError::ErrorCountExceedsSamples,
            BenchmarkError::HarnessFailed,
        ];

        for error in errors {
            let message = benchmark_error_to_cli_error(error, &request)
                .message()
                .to_string();
            assert!(!message.trim().is_empty());
            assert!(!message.contains("super-secret"));
        }
    }
}
