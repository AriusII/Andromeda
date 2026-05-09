use andromeda_hardware::HardwareProfile;

use crate::{
    BenchmarkError, BenchmarkWorkload, DEFAULT_DURATION_MS, DEFAULT_SAMPLES, DEFAULT_TEMP_BYTES,
    DEFAULT_WARMUPS, MAX_DURATION_MS, MAX_SAMPLES, MAX_TEMP_BYTES, MAX_WARMUPS, find_workload,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BenchmarkHardwareProfile {
    Conservative,
    DeclaredLocal,
}

impl BenchmarkHardwareProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conservative => "conservative",
            Self::DeclaredLocal => "declared-local",
        }
    }

    pub fn materialize(self) -> HardwareProfile {
        match self {
            Self::Conservative | Self::DeclaredLocal => HardwareProfile::conservative(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkRunRequest {
    pub workload_id: String,
    pub duration_ms: u64,
    pub samples: u32,
    pub warmups: u32,
    pub temp_budget_bytes: u64,
    pub hardware_profile: BenchmarkHardwareProfile,
}

impl BenchmarkRunRequest {
    pub fn new(workload_id: impl Into<String>) -> Self {
        Self {
            workload_id: workload_id.into(),
            duration_ms: DEFAULT_DURATION_MS,
            samples: DEFAULT_SAMPLES,
            warmups: DEFAULT_WARMUPS,
            temp_budget_bytes: DEFAULT_TEMP_BYTES,
            hardware_profile: BenchmarkHardwareProfile::Conservative,
        }
    }

    pub fn validate(&self) -> Result<&'static BenchmarkWorkload, BenchmarkError> {
        validate_run_request(self)
    }
}

pub fn validate_run_request(
    request: &BenchmarkRunRequest,
) -> Result<&'static BenchmarkWorkload, BenchmarkError> {
    if request.workload_id.is_empty() {
        return Err(BenchmarkError::EmptyWorkloadId);
    }
    if request.duration_ms == 0 {
        return Err(BenchmarkError::ZeroDuration);
    }
    if request.samples == 0 {
        return Err(BenchmarkError::ZeroSamples);
    }
    if request.duration_ms > MAX_DURATION_MS {
        return Err(BenchmarkError::DurationExceedsGlobalLimit);
    }
    if request.samples > MAX_SAMPLES {
        return Err(BenchmarkError::SamplesExceedsGlobalLimit);
    }
    if request.warmups > MAX_WARMUPS {
        return Err(BenchmarkError::WarmupsExceedsGlobalLimit);
    }
    if request.temp_budget_bytes == 0 {
        return Err(BenchmarkError::ZeroTempBudget);
    }
    if request.temp_budget_bytes > MAX_TEMP_BYTES {
        return Err(BenchmarkError::TempBudgetExceedsGlobalLimit);
    }

    let Some(workload) = find_workload(&request.workload_id) else {
        return Err(BenchmarkError::UnknownWorkload);
    };
    if request.duration_ms > workload.max_duration_ms {
        return Err(BenchmarkError::DurationExceedsWorkloadLimit);
    }
    if request.samples > workload.max_samples {
        return Err(BenchmarkError::SamplesExceedsWorkloadLimit);
    }
    if request.temp_budget_bytes > workload.max_temp_bytes {
        return Err(BenchmarkError::TempBudgetExceedsWorkloadLimit);
    }

    Ok(workload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_request_is_valid_for_inventory_recoverable_smoke() {
        let request = BenchmarkRunRequest::new("inventory-recoverable-smoke");

        let workload = request.validate().unwrap();

        assert_eq!(workload.id, "inventory-recoverable-smoke");
    }

    #[test]
    fn legacy_vertical_smoke_alias_resolves_to_inventory_recoverable_smoke() {
        let request = BenchmarkRunRequest::new("vertical-v0-smoke");

        let workload = request.validate().unwrap();

        assert_eq!(workload.id, "inventory-recoverable-smoke");
    }

    #[test]
    fn request_validation_enforces_global_and_workload_caps() {
        let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
        request.duration_ms = 60_001;
        assert_eq!(
            request.validate().unwrap_err(),
            BenchmarkError::DurationExceedsGlobalLimit
        );

        request.duration_ms = 5_001;
        assert_eq!(
            request.validate().unwrap_err(),
            BenchmarkError::DurationExceedsWorkloadLimit
        );

        request.duration_ms = 5_000;
        request.samples = 21;
        assert_eq!(
            request.validate().unwrap_err(),
            BenchmarkError::SamplesExceedsWorkloadLimit
        );

        request.samples = 20;
        request.temp_budget_bytes = MAX_TEMP_BYTES + 1;
        assert_eq!(
            request.validate().unwrap_err(),
            BenchmarkError::TempBudgetExceedsGlobalLimit
        );

        request.temp_budget_bytes = 0;
        assert_eq!(
            request.validate().unwrap_err(),
            BenchmarkError::ZeroTempBudget
        );

        request.temp_budget_bytes = DEFAULT_TEMP_BYTES + 1;
        assert_eq!(
            request.validate().unwrap_err(),
            BenchmarkError::TempBudgetExceedsWorkloadLimit
        );
    }
}
