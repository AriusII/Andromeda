use crate::{
    BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY, BenchmarkHistoryRecord, BenchmarkRunRequest,
    MAX_DURATION_MS, MAX_SAMPLES, MAX_TEMP_BYTES, find_workload,
};

use super::errors::BenchmarkScenarioEvidenceError;

/// Explicit resource budgets attached to evidence generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BenchmarkEvidenceBudgets {
    pub duration_ms: u64,
    pub samples: u32,
    pub temp_bytes: u64,
}

impl BenchmarkEvidenceBudgets {
    pub fn new(
        duration_ms: u64,
        samples: u32,
        temp_bytes: u64,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        if duration_ms == 0 {
            return Err(BenchmarkScenarioEvidenceError::ZeroDurationBudget);
        }
        if duration_ms > MAX_DURATION_MS {
            return Err(BenchmarkScenarioEvidenceError::DurationBudgetExceedsGlobalLimit);
        }
        if samples == 0 {
            return Err(BenchmarkScenarioEvidenceError::ZeroSampleBudget);
        }
        if samples > MAX_SAMPLES {
            return Err(BenchmarkScenarioEvidenceError::SampleBudgetExceedsGlobalLimit);
        }
        if temp_bytes == 0 {
            return Err(BenchmarkScenarioEvidenceError::ZeroTempBudget);
        }
        if temp_bytes > MAX_TEMP_BYTES {
            return Err(BenchmarkScenarioEvidenceError::TempBudgetExceedsGlobalLimit);
        }
        Ok(Self {
            duration_ms,
            samples,
            temp_bytes,
        })
    }

    pub fn from_run_request(
        request: &BenchmarkRunRequest,
    ) -> Result<Self, BenchmarkScenarioEvidenceError> {
        Self::new(
            request.duration_ms,
            request.samples,
            request.temp_budget_bytes,
        )
    }
}

pub(super) fn validate_budget_against_workload(
    workload_id: &str,
    budgets: BenchmarkEvidenceBudgets,
) -> Result<(), BenchmarkScenarioEvidenceError> {
    let workload =
        find_workload(workload_id).ok_or(BenchmarkScenarioEvidenceError::UnknownWorkloadId)?;
    if budgets.duration_ms > workload.max_duration_ms {
        return Err(BenchmarkScenarioEvidenceError::DurationBudgetExceedsWorkloadLimit);
    }
    if budgets.samples > workload.max_samples {
        return Err(BenchmarkScenarioEvidenceError::SampleBudgetExceedsWorkloadLimit);
    }
    if budgets.temp_bytes > workload.max_temp_bytes {
        return Err(BenchmarkScenarioEvidenceError::TempBudgetExceedsWorkloadLimit);
    }
    Ok(())
}

pub(super) fn validate_history_advisory_metadata(
    record: &BenchmarkHistoryRecord,
    budgets: BenchmarkEvidenceBudgets,
) -> Result<(), BenchmarkScenarioEvidenceError> {
    let advisory = &record.advisory;
    if advisory.advisory_boundary != BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY {
        return Err(BenchmarkScenarioEvidenceError::HistoryAdvisoryBoundaryInvalid);
    }
    if advisory.authoritative {
        return Err(BenchmarkScenarioEvidenceError::HistoryAuthoritative);
    }
    if advisory.can_select_plan_alone {
        return Err(BenchmarkScenarioEvidenceError::HistoryCanSelectPlanAlone);
    }
    if advisory.optimizer_boundary != BENCHMARK_EVIDENCE_OPTIMIZER_BOUNDARY {
        return Err(BenchmarkScenarioEvidenceError::HistoryOptimizerBoundaryInvalid);
    }
    if advisory.duration_cap_ms == 0 || advisory.duration_cap_ms > MAX_DURATION_MS {
        return Err(BenchmarkScenarioEvidenceError::HistoryDurationCapInvalid);
    }
    if advisory.sample_cap == 0 || advisory.sample_cap > MAX_SAMPLES {
        return Err(BenchmarkScenarioEvidenceError::HistorySampleCapInvalid);
    }
    if advisory.temp_cap_bytes == 0 || advisory.temp_cap_bytes > MAX_TEMP_BYTES {
        return Err(BenchmarkScenarioEvidenceError::HistoryTempCapInvalid);
    }
    if budgets.duration_ms > advisory.duration_cap_ms {
        return Err(BenchmarkScenarioEvidenceError::DurationBudgetExceedsHistoryCap);
    }
    if budgets.samples > advisory.sample_cap {
        return Err(BenchmarkScenarioEvidenceError::SampleBudgetExceedsHistoryCap);
    }
    if budgets.temp_bytes > advisory.temp_cap_bytes {
        return Err(BenchmarkScenarioEvidenceError::TempBudgetExceedsHistoryCap);
    }
    Ok(())
}
