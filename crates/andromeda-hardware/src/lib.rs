#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Hardware

Conservative CPU, RAM, GPU, and resource policy descriptors. GPU eligibility
remains explicitly outside commit, WAL, rollback, recovery, MVCC visibility,
catalog publication, and security-critical paths.
"#]

mod cpu;
mod gpu;
mod integration;
mod pipeline;
mod ram;

/// Explicit policy namespace for consumers that prefer importing bounded
/// hardware policy contracts via `andromeda_hardware::policy::*`.
pub mod policy {
    pub use crate::{
        CpuCapabilityClass, CpuProfile, GpuExecutionPolicy, GpuProfile, HardwareArchitecture,
        HardwareProfile, PipelineClass, RamProfile, RamSectionBudget, RamSectionRole,
        ResourceBudget,
    };
}

pub use cpu::{CpuCapabilityClass, CpuProfile, HardwareArchitecture};
pub use gpu::{GpuExecutionPolicy, GpuProfile};
pub use integration::{HardwareProfile, ResourceBudget};
pub use pipeline::PipelineClass;
pub use ram::{RamProfile, RamSectionBudget, RamSectionRole};
