#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![doc = r"
# andromeda-gpu

Optional GPU batch acceleration primitives for advisory-only analytical workloads.

This crate provides types, contracts, and prototype binding contexts for
GPU-accelerated statistics and analytics. It is intentionally excluded from all
C5 durable-kernel crates (WAL, recovery, MVCC, transaction, security).

## Guarantees

- No GPU runtime dependencies (no wgpu/cuda/opencl/ash).
- CPU fallback is mandatory for every GPU execution path.
- GPU output requires CPU validation before publication.
- Kill switch provides cooperative cancellation.
- No unsafe code.
"]

pub mod budget;
pub mod fallback;
pub mod job;
pub mod kill_switch;
pub mod policy;
pub mod prototypes;
pub mod trace;
pub mod validation;

/// Curated re-exports for the most commonly used types.
pub mod prelude {
    pub use crate::budget::{GpuBudget, GpuBudgetRequest};
    pub use crate::fallback::{CpuFallback, ValidationState};
    pub use crate::job::{FallbackReason, GpuJobClass};
    pub use crate::kill_switch::{KillReason, KillSwitch, KillSwitchHandle};
    pub use crate::policy::{
        GpuExecutionPolicy, GpuProfile, OptionalGpuDecision, OptionalGpuRequest,
        select_optional_gpu,
    };
    pub use crate::trace::{GpuExecutionResult, GpuExecutionTrace};
    pub use crate::validation::{GpuStatsValidationGate, Histogram};
}

#[cfg(feature = "trace")]
pub mod observe_integration;
