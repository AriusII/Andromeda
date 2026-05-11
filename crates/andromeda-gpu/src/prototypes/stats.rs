//! `GPU_STATS` prototype binding context.
//!
//! [`GpuStatsBindingContext`] orchestrates the full GPU statistics pipeline:
//! kill-switch check, GPU histogram computation, CPU shadow, and validation
//! gate. Only a validated histogram is returned as the primary result; all
//! other cases fall back to the CPU result.

use std::fmt;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    budget::{GpuBudget, GpuBudgetRequest},
    fallback::{CpuFallback, ValidationState},
    job::{FallbackReason, GpuJobClass},
    kill_switch::KillSwitchHandle,
    policy::GpuProfile,
    prototypes::permit::StatsExecutionPermit,
    trace::{GpuBudgetEvidence, GpuExecutionResult, GpuExecutionTrace},
    validation::{GpuStatsValidationGate, Histogram},
};

/// Input for a GPU statistics job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsInput {
    /// Log sequence number of the snapshot being analysed.
    pub snapshot_lsn: u64,
    /// Column identifier for which statistics are computed.
    pub column_id: u32,
}

/// Prototype binding context for `GPU_STATS` histogram workloads.
///
/// Enforce the full execution contract:
/// 1. Kill switch — short-circuits with [`AndromedaErrorKind::Resource`] if
///    active.
/// 2. GPU availability — skips the GPU closure and uses the CPU fallback
///    directly if [`GpuProfile::available`] is `false`.
/// 3. GPU closure — caller-injected; no real GPU runtime is linked.
/// 4. CPU shadow — always executed for validation.
/// 5. Validation gate — returns the CPU result if the GPU histogram is rejected.
///
/// The `job` field is always [`GpuJobClass::Statistics`].
pub struct GpuStatsBindingContext<F>
where
    F: CpuFallback<Input = StatsInput, Output = Histogram>,
{
    /// Job class — must be [`GpuJobClass::Statistics`].
    pub job: GpuJobClass,
    /// Cooperative cancellation handle.
    pub kill_switch: KillSwitchHandle,
    /// Resource budget for this job.
    pub budget: GpuBudget,
    /// Validation gate that compares GPU and CPU histograms.
    pub validation_gate: GpuStatsValidationGate,
    /// Authoritative CPU fallback implementation.
    pub cpu_fallback: F,
    /// GPU device profile — used to skip the GPU closure when unavailable.
    pub gpu_profile: GpuProfile,
}

impl<F> GpuStatsBindingContext<F>
where
    F: CpuFallback<Input = StatsInput, Output = Histogram>,
{
    fn budget_request(input: &StatsInput) -> GpuBudgetRequest {
        let memory_bytes = 4_096u64.saturating_add(u64::from(input.column_id).saturating_mul(256));
        let time_ms = 10u64;
        let transfer_bytes = 4_096u64.saturating_add(input.snapshot_lsn % 4_096);
        GpuBudgetRequest {
            memory_bytes,
            time_ms,
            transfer_bytes,
        }
    }

    /// Executes the `GPU_STATS` pipeline.
    ///
    /// Timestamps in the returned [`GpuExecutionTrace`] are set to `0` in v0.
    /// Callers that require real timestamps should wrap this in a higher-level
    /// context.
    ///
    /// # Errors
    ///
    /// - Returns [`AndromedaErrorKind::Resource`] if the kill switch is active.
    /// - Returns [`AndromedaErrorKind::Resource`] if the CPU fallback fails.
    pub fn execute(
        &self,
        permit: &StatsExecutionPermit,
        input: &StatsInput,
        gpu_compute: impl FnOnce(&StatsInput) -> AndromedaResult<Histogram>,
    ) -> AndromedaResult<(Histogram, GpuExecutionTrace)> {
        self.execute_with_trace_id(0, permit, input, gpu_compute)
    }

    /// Executes the `GPU_STATS` pipeline with a caller-supplied trace ID.
    ///
    /// # Errors
    ///
    /// - Returns [`AndromedaErrorKind::Resource`] if the kill switch is active.
    /// - Returns [`AndromedaErrorKind::Resource`] if the CPU fallback fails.
    pub fn execute_with_trace_id(
        &self,
        trace_id: u128,
        permit: &StatsExecutionPermit,
        input: &StatsInput,
        gpu_compute: impl FnOnce(&StatsInput) -> AndromedaResult<Histogram>,
    ) -> AndromedaResult<(Histogram, GpuExecutionTrace)> {
        // Step 1 — kill switch check.
        if self.kill_switch.is_cancelled() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "GPU_STATS execution cancelled by kill switch",
            ));
        }

        let started_at: u64 = 0;
        let trace_base = GpuExecutionTrace::new(self.job, trace_id, started_at);
        let budget_request = Self::budget_request(input);
        let budget_evidence = GpuBudgetEvidence::new(budget_request, self.budget);

        // Step 2 — GPU availability check.
        if !permit.gpu_selected() {
            let cpu_result = self.cpu_fallback.execute(input)?;
            let trace = trace_base
                .finish(0, GpuExecutionResult::Success)
                .with_validation_state(ValidationState::NotValidated)
                .with_fallback_reason(FallbackReason::DeviceUnavailable)
                .with_budget_evidence(budget_evidence);
            return Ok((cpu_result, trace));
        }

        // Step 3 — budget check before dispatching GPU work.
        self.budget.fits(&budget_request)?;

        // Step 4 — run GPU closure.
        let Ok(gpu_histogram) = gpu_compute(input) else {
            let cpu_result = self.cpu_fallback.execute(input)?;
            let trace = trace_base
                .finish(
                    0,
                    GpuExecutionResult::Failed("GPU computation error".to_owned()),
                )
                .with_validation_state(ValidationState::NotValidated)
                .with_fallback_reason(FallbackReason::GpuComputationFailed)
                .with_budget_evidence(budget_evidence);
            return Ok((cpu_result, trace));
        };

        // Step 5 — CPU shadow.
        let cpu_shadow = self.cpu_fallback.execute(input)?;

        // Step 6 — validation gate.
        let validation_state = self
            .validation_gate
            .validate_histogram(&gpu_histogram, &cpu_shadow)?;

        let is_validated = validation_state == ValidationState::Validated;
        if is_validated {
            let trace = trace_base
                .finish(0, GpuExecutionResult::Success)
                .with_validation_state(validation_state)
                .with_budget_evidence(budget_evidence);
            Ok((gpu_histogram, trace))
        } else {
            let trace = trace_base
                .finish(
                    0,
                    GpuExecutionResult::Failed("histogram validation rejected".to_owned()),
                )
                .with_validation_state(validation_state)
                .with_fallback_reason(FallbackReason::CpuValidationFailed)
                .with_budget_evidence(budget_evidence);
            Ok((cpu_shadow, trace))
        }
    }
}

impl<F> fmt::Debug for GpuStatsBindingContext<F>
where
    F: CpuFallback<Input = StatsInput, Output = Histogram>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuStatsBindingContext")
            .field("job", &self.job)
            .field("kill_switch", &self.kill_switch)
            .field("budget", &self.budget)
            .field("validation_gate", &self.validation_gate)
            .field("gpu_profile", &self.gpu_profile)
            .finish_non_exhaustive()
    }
}
