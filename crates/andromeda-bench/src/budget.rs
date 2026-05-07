use crate::{BenchmarkError, BenchmarkWorkload};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerformanceBudget {
    pub max_p50_latency_us: u64,
    pub max_p95_latency_us: u64,
    pub max_error_rate_ppm: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetStatus {
    Passed,
    Failed,
}

pub fn evaluate_budget(
    workload: &BenchmarkWorkload,
    p50_latency_us: u64,
    p95_latency_us: u64,
    error_count: u32,
    sample_count: u32,
) -> Result<BudgetStatus, BenchmarkError> {
    if sample_count == 0 {
        return Err(BenchmarkError::InsufficientSamplesForStatistics);
    }
    if error_count > sample_count {
        return Err(BenchmarkError::ErrorCountExceedsSamples);
    }

    let error_rate_ppm = (u64::from(error_count) * 1_000_000) / u64::from(sample_count);
    let failed = p50_latency_us > workload.budget.max_p50_latency_us
        || p95_latency_us > workload.budget.max_p95_latency_us
        || error_rate_ppm > u64::from(workload.budget.max_error_rate_ppm);

    Ok(if failed {
        BudgetStatus::Failed
    } else {
        BudgetStatus::Passed
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find_workload;

    #[test]
    fn evaluates_budget_with_latency_and_error_thresholds() {
        let workload = find_workload("protocol-smoke-contract").unwrap();

        assert_eq!(
            evaluate_budget(workload, 10_000, 50_000, 0, 10).unwrap(),
            BudgetStatus::Passed
        );
        assert_eq!(
            evaluate_budget(workload, 10_001, 50_000, 0, 10).unwrap(),
            BudgetStatus::Failed
        );
        assert_eq!(
            evaluate_budget(workload, 10_000, 50_000, 1, 10).unwrap(),
            BudgetStatus::Failed
        );
    }

    #[test]
    fn rejects_error_counts_that_exceed_samples() {
        let workload = find_workload("protocol-smoke-contract").unwrap();

        assert_eq!(
            evaluate_budget(workload, 10_000, 50_000, 2, 1).unwrap_err(),
            BenchmarkError::ErrorCountExceedsSamples
        );
    }
}
