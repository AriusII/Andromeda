//! Re-exports of GPU execution policy primitives from `andromeda_hardware`.
//!
//! These are the policy types that callers use to decide whether GPU execution
//! is permitted for a given pipeline. They are defined in `andromeda-hardware`
//! and re-exported here for convenience so consumers of `andromeda-gpu` do not
//! need a direct dependency on `andromeda-hardware`.

pub use andromeda_hardware::{
    GpuExecutionPolicy, GpuProfile, OptionalGpuDecision, OptionalGpuRequest, PipelineClass,
    select_optional_gpu,
};
