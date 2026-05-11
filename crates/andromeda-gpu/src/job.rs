//! GPU job classification and fallback reason types.
//!
//! [`GpuJobClass`] categorises GPU workloads by domain. [`FallbackReason`]
//! records why execution fell back to the authoritative CPU path.

/// Classification of GPU batch workloads by analytical domain.
///
/// All variants return `false` from
/// [`is_publishable_without_validation`][GpuJobClass::is_publishable_without_validation]
/// because GPU output requires CPU shadow validation before any result can be
/// published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuJobClass {
    /// Histogram and statistics refresh workloads.
    Statistics,
    /// Columnar analytics scan workloads.
    Analytics,
    /// Benchmark and calibration workloads.
    Benchmark,
}

impl GpuJobClass {
    /// Returns `false` for every variant.
    ///
    /// GPU-produced output must always pass CPU shadow validation before
    /// publication. Silent publication of unvalidated GPU results is
    /// rejected by the specification.
    #[must_use]
    pub fn is_publishable_without_validation(self) -> bool {
        false
    }
}

/// Records why a GPU execution path fell back to the CPU.
///
/// This is attached to [`GpuExecutionTrace`][crate::trace::GpuExecutionTrace]
/// when the GPU path was bypassed or its result was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FallbackReason {
    /// GPU execution is disabled by policy.
    Disabled,
    /// The requested resource budget was exceeded.
    BudgetExceeded,
    /// A kill switch was set to [`Active`][crate::kill_switch::KillSwitch::Active].
    KillSwitchActive,
    /// GPU output diverged from the CPU shadow beyond the validation threshold.
    CpuValidationFailed,
    /// The GPU compute path returned an execution error.
    GpuComputationFailed,
    /// No GPU device is available on this system.
    DeviceUnavailable,
    /// The job was cancelled before completion.
    Cancelled,
}
