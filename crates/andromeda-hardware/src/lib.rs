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

pub use cpu::{CpuCapabilityClass, CpuProfile, HardwareArchitecture};
pub use gpu::{GpuExecutionPolicy, GpuProfile};
pub use integration::{HardwareProfile, ResourceBudget};
pub use pipeline::PipelineClass;
pub use ram::{RamProfile, RamSectionBudget, RamSectionRole};
