//! CPU hardware capabilities and profiles.

/// Describes the CPU architecture this process is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareArchitecture {
    X64,
    Arm64,
    Unknown,
}

/// CPU capability class ordered from conservative to wider SIMD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CpuCapabilityClass {
    Conservative,
    Scalar64,
    Simd128,
    Simd256,
    Simd512,
}

impl CpuCapabilityClass {
    pub const fn supports_simd(self) -> bool {
        matches!(self, Self::Simd128 | Self::Simd256 | Self::Simd512)
    }
}

/// CPU profile describing the hardware environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuProfile {
    pub architecture: HardwareArchitecture,
    pub capability_class: CpuCapabilityClass,
    pub hardware_threads: u16,
}

impl CpuProfile {
    /// Safe default when capabilities are not configured or detected.
    pub const fn conservative() -> Self {
        Self {
            architecture: HardwareArchitecture::Unknown,
            capability_class: CpuCapabilityClass::Conservative,
            hardware_threads: 1,
        }
    }

    pub const fn supports_simd(&self) -> bool {
        self.capability_class.supports_simd()
    }
}

impl Default for CpuProfile {
    fn default() -> Self {
        Self::conservative()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simd_capability_class_ordering() {
        assert!(CpuCapabilityClass::Conservative < CpuCapabilityClass::Scalar64);
        assert!(CpuCapabilityClass::Scalar64 < CpuCapabilityClass::Simd128);
        assert!(CpuCapabilityClass::Simd128 < CpuCapabilityClass::Simd256);
        assert!(CpuCapabilityClass::Simd256 < CpuCapabilityClass::Simd512);
    }

    #[test]
    fn conservative_profile_has_no_simd() {
        let profile = CpuProfile::conservative();
        assert!(!profile.supports_simd());
        assert_eq!(profile.hardware_threads, 1);
    }

    #[test]
    fn simd_support_check_matches_class() {
        assert!(!CpuCapabilityClass::Conservative.supports_simd());
        assert!(!CpuCapabilityClass::Scalar64.supports_simd());
        assert!(CpuCapabilityClass::Simd128.supports_simd());
        assert!(CpuCapabilityClass::Simd256.supports_simd());
        assert!(CpuCapabilityClass::Simd512.supports_simd());
    }
}
