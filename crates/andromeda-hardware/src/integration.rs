//! Hardware capability aggregation and resource budgets.

use crate::{CpuProfile, GpuProfile, HardwareArchitecture, PipelineClass, RamProfile};
use andromeda_error::AndromedaResult;

/// Complete hardware profile describing the compute environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareProfile {
    pub architecture: HardwareArchitecture,
    pub has_simd: bool,
    pub has_direct_io: bool,
    pub cpu: CpuProfile,
    pub ram: RamProfile,
    pub gpu: GpuProfile,
}

impl HardwareProfile {
    /// Safe default when hardware capabilities are not configured or detected.
    pub const fn conservative() -> Self {
        let cpu = CpuProfile::conservative();

        Self {
            architecture: cpu.architecture,
            has_simd: cpu.supports_simd(),
            has_direct_io: false,
            cpu,
            ram: RamProfile::conservative(),
            gpu: GpuProfile::disabled(),
        }
    }

    pub fn validate_gpu_pipeline(&self, pipeline: PipelineClass) -> AndromedaResult<()> {
        self.gpu.validate_pipeline(pipeline)
    }

    pub fn validate_ram_budgets(&self) -> AndromedaResult<()> {
        self.ram.validate_budgets()
    }
}

impl Default for HardwareProfile {
    fn default() -> Self {
        Self::conservative()
    }
}

/// Budget for execution resources during request processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceBudget {
    pub max_memory_bytes: u64,
    pub max_temp_bytes: u64,
    pub max_streams: u32,
}

impl ResourceBudget {
    pub const fn new(max_memory_bytes: u64, max_temp_bytes: u64, max_streams: u32) -> Self {
        Self {
            max_memory_bytes,
            max_temp_bytes,
            max_streams,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuCapabilityClass, GpuExecutionPolicy};

    #[test]
    fn conservative_profile_disables_gpu() {
        let profile = HardwareProfile::conservative();

        assert_eq!(
            profile.cpu.capability_class,
            CpuCapabilityClass::Conservative
        );
        assert!(!profile.has_simd);
        assert_eq!(profile.gpu.execution_policy, GpuExecutionPolicy::Disabled);
        assert!(
            profile
                .validate_gpu_pipeline(PipelineClass::BatchAnalytics)
                .is_err()
        );
    }
}
