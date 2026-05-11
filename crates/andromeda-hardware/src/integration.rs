//! Hardware capability aggregation and resource budgets.

use crate::{
    CpuProfile, CpuRuntimeProfile, GpuProfile, HardwareArchitecture, PipelineClass, RamProfile,
};
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

    pub fn detected(ram: RamProfile, gpu: GpuProfile, has_direct_io: bool) -> Self {
        Self::from_cpu_profile(CpuProfile::detect(), ram, gpu, has_direct_io)
    }

    pub fn from_cpu_profile(
        cpu: CpuProfile,
        ram: RamProfile,
        gpu: GpuProfile,
        has_direct_io: bool,
    ) -> Self {
        Self {
            architecture: cpu.architecture,
            has_simd: cpu.supports_simd(),
            has_direct_io,
            cpu,
            ram,
            gpu,
        }
    }

    pub const fn runtime_profile(&self) -> CpuRuntimeProfile {
        self.cpu.runtime_profile()
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
    use crate::{CpuCapabilityClass, CpuRuntimeProfile, GpuExecutionPolicy};

    #[test]
    fn conservative_profile_disables_gpu() {
        let profile = HardwareProfile::conservative();

        assert_eq!(
            profile.cpu.capability_class,
            CpuCapabilityClass::Conservative
        );
        assert!(!profile.has_simd);
        assert_eq!(profile.runtime_profile(), CpuRuntimeProfile::Conservative);
        assert_eq!(profile.gpu.execution_policy, GpuExecutionPolicy::Disabled);
        assert!(
            profile
                .validate_gpu_pipeline(PipelineClass::BatchAnalytics)
                .is_err()
        );
    }

    #[test]
    fn detected_hardware_profile_uses_detected_cpu_runtime_profile() {
        let profile =
            HardwareProfile::detected(RamProfile::conservative(), GpuProfile::disabled(), false);

        assert_eq!(profile.runtime_profile(), profile.cpu.runtime_profile());
        assert_eq!(profile.has_simd, profile.cpu.supports_simd());
        assert!(profile.cpu.hardware_threads >= 1);
    }
}
