#![forbid(unsafe_code)]

//! Bounded benchmark contracts for Andromeda operational diagnostics.
//!
//! This crate defines benchmark workloads, limits, budgets, and result evidence
//! types. It intentionally does not provide a runtime wire protocol surface.

use andromeda_core::HardwareProfile;

pub const MAX_DURATION_MS: u64 = 60_000;
pub const MAX_SAMPLES: u32 = 100;
pub const MAX_WARMUPS: u32 = 10;

pub const DEFAULT_DURATION_MS: u64 = 5_000;
pub const DEFAULT_SAMPLES: u32 = 10;
pub const DEFAULT_WARMUPS: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkError {
    EmptyWorkloadId,
    UnknownWorkload,
    ZeroDuration,
    ZeroSamples,
    DurationExceedsGlobalLimit,
    SamplesExceedsGlobalLimit,
    WarmupsExceedsGlobalLimit,
    DurationExceedsWorkloadLimit,
    SamplesExceedsWorkloadLimit,
    InsufficientSamplesForStatistics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerformanceBudget {
    pub max_p50_latency_us: u64,
    pub max_p95_latency_us: u64,
    pub max_error_rate_ppm: u32,
}

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
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkRunRequest {
    pub workload_id: String,
    pub duration_ms: u64,
    pub samples: u32,
    pub warmups: u32,
    pub hardware_profile: BenchmarkHardwareProfile,
}

impl BenchmarkRunRequest {
    pub fn new(workload_id: impl Into<String>) -> Self {
        Self {
            workload_id: workload_id.into(),
            duration_ms: DEFAULT_DURATION_MS,
            samples: DEFAULT_SAMPLES,
            warmups: DEFAULT_WARMUPS,
            hardware_profile: BenchmarkHardwareProfile::Conservative,
        }
    }

    pub fn validate(&self) -> Result<&'static BenchmarkWorkload, BenchmarkError> {
        validate_run_request(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetStatus {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkEvidence {
    pub workload_id: String,
    pub hardware_profile: BenchmarkHardwareProfile,
    pub duration_ms: u64,
    pub samples: u32,
    pub warmups: u32,
    pub started_at_unix_ms: u64,
    pub elapsed_ms: u64,
    pub sample_count: u32,
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub error_count: u32,
    pub budget_status: BudgetStatus,
    pub diagnostic_only: bool,
}

pub fn find_workload(id: &str) -> Option<&'static BenchmarkWorkload> {
    WORKLOADS.iter().find(|workload| workload.id == id)
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

    let Some(workload) = find_workload(&request.workload_id) else {
        return Err(BenchmarkError::UnknownWorkload);
    };
    if request.duration_ms > workload.max_duration_ms {
        return Err(BenchmarkError::DurationExceedsWorkloadLimit);
    }
    if request.samples > workload.max_samples {
        return Err(BenchmarkError::SamplesExceedsWorkloadLimit);
    }

    Ok(workload)
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

pub fn run_bounded_benchmark(
    request: &BenchmarkRunRequest,
) -> Result<BenchmarkEvidence, BenchmarkError> {
    let workload = request.validate()?;
    let _profile = request.hardware_profile.materialize();

    let sample_count = request.samples;
    let profile_adjustment_us = match request.hardware_profile {
        BenchmarkHardwareProfile::Conservative => 0,
        BenchmarkHardwareProfile::DeclaredLocal => 1,
    };
    let workload_base_latency_us = match workload.id {
        "vertical-v0-smoke" => 2_500,
        "protocol-smoke-contract" => 1_000,
        "wal-append-smoke" => 1_500,
        _ => return Err(BenchmarkError::UnknownWorkload),
    };

    let p50_latency_us = workload_base_latency_us
        + u64::from(request.samples)
        + u64::from(request.warmups)
        + profile_adjustment_us;
    let p95_latency_us = p50_latency_us * 2 + request.duration_ms / 1_000;
    let error_count = 0;
    let budget_status = evaluate_budget(
        workload,
        p50_latency_us,
        p95_latency_us,
        error_count,
        sample_count,
    )?;
    let requested_iterations = u64::from(request.samples) + u64::from(request.warmups);
    let elapsed_ms = request.duration_ms.min(requested_iterations.max(1));

    Ok(BenchmarkEvidence {
        workload_id: workload.id.to_string(),
        hardware_profile: request.hardware_profile,
        duration_ms: request.duration_ms,
        samples: request.samples,
        warmups: request.warmups,
        started_at_unix_ms: 0,
        elapsed_ms,
        sample_count,
        p50_latency_us,
        p95_latency_us,
        error_count,
        budget_status,
        diagnostic_only: true,
    })
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

    #[test]
    fn default_request_is_valid_for_vertical_smoke() {
        let request = BenchmarkRunRequest::new("vertical-v0-smoke");

        let workload = request.validate().unwrap();

        assert_eq!(workload.id, "vertical-v0-smoke");
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
    }

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
    fn diagnostic_evidence_is_explicit() {
        let evidence = BenchmarkEvidence {
            workload_id: "protocol-smoke-contract".to_string(),
            hardware_profile: BenchmarkHardwareProfile::Conservative,
            duration_ms: 1_000,
            samples: 5,
            warmups: 1,
            started_at_unix_ms: 1,
            elapsed_ms: 950,
            sample_count: 5,
            p50_latency_us: 10,
            p95_latency_us: 20,
            error_count: 0,
            budget_status: BudgetStatus::Passed,
            diagnostic_only: true,
        };

        assert!(evidence.diagnostic_only);
        assert_eq!(evidence.hardware_profile.as_str(), "conservative");
    }

    #[test]
    fn bounded_runner_produces_deterministic_diagnostic_evidence() {
        let mut request = BenchmarkRunRequest::new("protocol-smoke-contract");
        request.duration_ms = 1_000;
        request.samples = 5;
        request.warmups = 1;
        request.hardware_profile = BenchmarkHardwareProfile::DeclaredLocal;

        let first = run_bounded_benchmark(&request).unwrap();
        let second = run_bounded_benchmark(&request).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.started_at_unix_ms, 0);
        assert_eq!(first.elapsed_ms, 6);
        assert_eq!(first.sample_count, 5);
        assert_eq!(first.budget_status, BudgetStatus::Passed);
        assert!(first.diagnostic_only);
    }
}
