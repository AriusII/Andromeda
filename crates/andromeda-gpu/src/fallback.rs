//! CPU fallback contract and validation state.
//!
//! [`CpuFallback`] is the mandatory companion to every GPU execution path. The
//! CPU path is the authoritative source of truth; GPU output is only publishable
//! after passing [`GpuStatsValidationGate`][crate::validation::GpuStatsValidationGate]
//! or an equivalent domain-specific validator.
//!
//! [`ValidationState`] records whether a GPU result has been verified by the
//! CPU shadow, is pending verification, or has been rejected.

use andromeda_error::AndromedaResult;

/// Mandatory CPU fallback contract for GPU execution paths.
///
/// Every [`GpuStatsBindingContext`][crate::prototypes::stats::GpuStatsBindingContext]
/// and [`GpuAnalyticsBindingContext`][crate::prototypes::analytics::GpuAnalyticsBindingContext]
/// requires a `CpuFallback` implementation. The CPU path is invoked both as the
/// authoritative shadow for GPU result validation and as the primary result
/// source when GPU execution is unavailable or rejected.
pub trait CpuFallback: Send + Sync {
    /// The type of input consumed by this fallback.
    type Input;
    /// The type of output produced by this fallback.
    ///
    /// Must implement `Eq` and `Debug` so that GPU results can be compared
    /// against the CPU shadow.
    type Output: Eq + std::fmt::Debug;

    /// Executes the CPU fallback computation.
    ///
    /// # Errors
    ///
    /// Returns [`andromeda_error::AndromedaError`] if the CPU computation fails.
    fn execute(&self, input: &Self::Input) -> AndromedaResult<Self::Output>;
}

/// Records whether a GPU result has been verified against a CPU shadow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationState {
    /// The GPU result has not yet been compared to a CPU shadow.
    NotValidated,
    /// The GPU result was compared to the CPU shadow and passed the deviation
    /// threshold check. The result is safe to use.
    Validated,
    /// The GPU result diverged from the CPU shadow beyond the configured
    /// threshold. The enclosed string describes the rejection reason.
    Rejected(String),
}
