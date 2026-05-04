//! Hardware capability aggregation and resource budgets.
//!
//! This module integrates CPU, RAM, and GPU profiles into a complete hardware
//! capability description, along with execution resource budgets.

use super::hardware_cpu::{CpuProfile, HardwareArchitecture};
use super::hardware_gpu::GpuProfile;
use super::hardware_pipeline::PipelineClass;
use super::hardware_ram::RamProfile;
use crate::AndromedaResult;

/// Complete hardware profile describing the compute environment.
///
/// This struct aggregates CPU, RAM, and GPU capabilities to provide a comprehensive
/// view of available compute resources and constraints. It's used for resource scheduling,
/// execution strategy selection, and validation of operational policies.
///
/// # Examples
///
/// ```ignore
/// let profile = HardwareProfile::conservative();
/// profile.validate_gpu_pipeline(PipelineClass::BatchAnalytics)?;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareProfile {
    /// Detected or configured hardware architecture
    pub architecture: HardwareArchitecture,
    /// Whether SIMD support is available
    pub has_simd: bool,
    /// Whether direct I/O (bypassing OS cache) is available
    pub has_direct_io: bool,
    /// CPU profile with architecture and capability details
    pub cpu: CpuProfile,
    /// RAM budget allocation by role
    pub ram: RamProfile,
    /// GPU availability and execution policy
    pub gpu: GpuProfile,
}

impl HardwareProfile {
    /// Creates a conservative hardware profile with minimal capabilities.
    ///
    /// This is the safe default for systems where capabilities cannot be reliably detected.
    /// Conservative profile assumes:
    /// - No SIMD support
    /// - Single hardware thread
    /// - No GPU
    /// - No direct I/O
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = HardwareProfile::conservative();
    /// assert_eq!(profile.cpu.capability_class, CpuCapabilityClass::Conservative);
    /// ```
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

    /// Validates that GPU execution is permitted for the given pipeline class.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU is not available or the pipeline class
    /// is restricted by the GPU execution policy.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = HardwareProfile::conservative();
    /// assert!(profile.validate_gpu_pipeline(PipelineClass::BatchAnalytics).is_err());
    /// ```
    pub fn validate_gpu_pipeline(&self, pipeline: PipelineClass) -> AndromedaResult<()> {
        self.gpu.validate_pipeline(pipeline)
    }

    /// Validates that RAM section budgets do not exceed the total declared budget.
    ///
    /// # Errors
    ///
    /// Returns an error if the sum of section budgets exceeds the total bytes.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = HardwareProfile::conservative();
    /// profile.validate_ram_budgets()?;
    /// ```
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
///
/// This struct defines limits on memory, temporary storage, and concurrent streams
/// available to a single execution context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceBudget {
    /// Maximum memory available in bytes
    pub max_memory_bytes: u64,
    /// Maximum temporary storage in bytes (working set)
    pub max_temp_bytes: u64,
    /// Maximum concurrent streams (result sets)
    pub max_streams: u32,
}

impl ResourceBudget {
    /// Creates a new resource budget with the specified limits.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let budget = ResourceBudget::new(1_000_000, 100_000, 10);
    /// ```
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
    use super::super::hardware_cpu::CpuCapabilityClass;
    use super::super::hardware_gpu::GpuExecutionPolicy;
    use super::*;

    #[test]
    fn conservative_profile_disables_gpu() {
        let profile = HardwareProfile::conservative();

        assert_eq!(
            profile.cpu.capability_class,
            CpuCapabilityClass::Conservative
        );
        assert!(!profile.has_simd);
        assert_eq!(profile.gpu.execution_policy, GpuExecutionPolicy::Disabled);
        assert!(profile
            .validate_gpu_pipeline(PipelineClass::BatchAnalytics)
            .is_err());
    }
}
