//! `GPU_ANALYTICS` prototype binding context.
//!
//! [`GpuAnalyticsBindingContext`] orchestrates the GPU analytics scan pipeline:
//! kill-switch check, GPU compute, CPU shadow, and equality-based validation.
//! Any mismatch between GPU and CPU outputs is treated as a rejection; the CPU
//! result is returned in that case.

use std::fmt;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{
    budget::{GpuBudget, GpuBudgetRequest},
    fallback::{CpuFallback, ValidationState},
    job::{FallbackReason, GpuJobClass},
    kill_switch::KillSwitchHandle,
    policy::GpuProfile,
    prototypes::permit::AnalyticsExecutionPermit,
    trace::{GpuBudgetEvidence, GpuExecutionResult, GpuExecutionTrace},
};

/// Input for a GPU analytics job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsInput {
    /// Segment identifier for the columnar scan.
    pub segment_id: u64,
    /// Column identifiers included in this scan.
    pub column_ids: Vec<u32>,
}

/// Output of a GPU analytics job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsOutput {
    /// Number of rows processed in this scan.
    pub row_count: u64,
    /// A digest of the scanned data for cross-validation purposes.
    pub digest: u64,
}

/// Prototype binding context for `GPU_ANALYTICS` columnar scan workloads.
///
/// Enforces the full execution contract:
/// 1. Kill switch — short-circuits with [`AndromedaErrorKind::Resource`] if
///    active.
/// 2. GPU availability — skips the GPU closure and uses CPU fallback directly
///    if [`GpuProfile::available`] is `false`.
/// 3. GPU closure — caller-injected; no real GPU runtime is linked.
/// 4. CPU shadow — always executed for validation.
/// 5. Equality check — if GPU and CPU outputs differ, returns the CPU result
///    with [`FallbackReason::CpuValidationFailed`].
///
/// The `job` field is always [`GpuJobClass::Analytics`].
pub struct GpuAnalyticsBindingContext<F>
where
    F: CpuFallback<Input = AnalyticsInput, Output = AnalyticsOutput>,
{
    /// Job class — must be [`GpuJobClass::Analytics`].
    pub job: GpuJobClass,
    /// Cooperative cancellation handle.
    pub kill_switch: KillSwitchHandle,
    /// Resource budget for this job.
    pub budget: GpuBudget,
    /// Authoritative CPU fallback implementation.
    pub cpu_fallback: F,
    /// GPU device profile — used to skip the GPU closure when unavailable.
    pub gpu_profile: GpuProfile,
}

impl<F> GpuAnalyticsBindingContext<F>
where
    F: CpuFallback<Input = AnalyticsInput, Output = AnalyticsOutput>,
{
    fn budget_request(input: &AnalyticsInput) -> GpuBudgetRequest {
        let column_count = input.column_ids.len() as u64;
        let column_count = column_count.max(1);
        let memory_bytes = 2_048u64.saturating_add(column_count.saturating_mul(512));
        let time_ms = 25u64.saturating_add(column_count.saturating_mul(5));
        let transfer_bytes = 4_096u64.saturating_add(column_count.saturating_mul(1_024));
        GpuBudgetRequest {
            memory_bytes,
            time_ms,
            transfer_bytes,
        }
    }

    /// Executes the `GPU_ANALYTICS` pipeline.
    ///
    /// Timestamps in the returned [`GpuExecutionTrace`] are set to `0` in v0.
    ///
    /// # Errors
    ///
    /// - Returns [`AndromedaErrorKind::Resource`] if the kill switch is active.
    /// - Returns [`AndromedaErrorKind::Resource`] if the CPU fallback fails.
    pub fn execute(
        &self,
        permit: &AnalyticsExecutionPermit,
        input: &AnalyticsInput,
        gpu_compute: impl FnOnce(&AnalyticsInput) -> AndromedaResult<AnalyticsOutput>,
    ) -> AndromedaResult<(AnalyticsOutput, GpuExecutionTrace)> {
        self.execute_with_trace_id(0, permit, input, gpu_compute)
    }

    /// Executes the `GPU_ANALYTICS` pipeline with a caller-supplied trace ID.
    ///
    /// # Errors
    ///
    /// - Returns [`AndromedaErrorKind::Resource`] if the kill switch is active.
    /// - Returns [`AndromedaErrorKind::Resource`] if the CPU fallback fails.
    pub fn execute_with_trace_id(
        &self,
        trace_id: u128,
        permit: &AnalyticsExecutionPermit,
        input: &AnalyticsInput,
        gpu_compute: impl FnOnce(&AnalyticsInput) -> AndromedaResult<AnalyticsOutput>,
    ) -> AndromedaResult<(AnalyticsOutput, GpuExecutionTrace)> {
        // Step 1 — kill switch check.
        if self.kill_switch.is_cancelled() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "GPU_ANALYTICS execution cancelled by kill switch",
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
        let Ok(gpu_output) = gpu_compute(input) else {
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

        // Step 6 — equality-based validation.
        if gpu_output == cpu_shadow {
            let trace = trace_base
                .finish(0, GpuExecutionResult::Success)
                .with_validation_state(ValidationState::Validated)
                .with_budget_evidence(budget_evidence);
            Ok((gpu_output, trace))
        } else {
            let rejection_msg = format!(
                "GPU output (row_count={}, digest={}) differs from CPU shadow \
                 (row_count={}, digest={})",
                gpu_output.row_count, gpu_output.digest, cpu_shadow.row_count, cpu_shadow.digest,
            );
            let trace = trace_base
                .finish(
                    0,
                    GpuExecutionResult::Failed("analytics validation rejected".to_owned()),
                )
                .with_validation_state(ValidationState::Rejected(rejection_msg))
                .with_fallback_reason(FallbackReason::CpuValidationFailed)
                .with_budget_evidence(budget_evidence);
            Ok((cpu_shadow, trace))
        }
    }
}

impl<F> fmt::Debug for GpuAnalyticsBindingContext<F>
where
    F: CpuFallback<Input = AnalyticsInput, Output = AnalyticsOutput>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuAnalyticsBindingContext")
            .field("job", &self.job)
            .field("kill_switch", &self.kill_switch)
            .field("budget", &self.budget)
            .field("gpu_profile", &self.gpu_profile)
            .finish_non_exhaustive()
    }
}
