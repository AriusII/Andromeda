//! CPU hardware capabilities and profiles.
//!
//! This module defines CPU architecture detection, capability classes,
//! and profiles that determine supported instruction sets and threading models.

/// Describes the CPU architecture this process is running on.
///
/// Used to determine which instruction sets and machine code patterns are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareArchitecture {
    /// x86-64 (AMD64) architecture
    X64,
    /// ARM 64-bit architecture
    Arm64,
    /// Unknown or unsupported architecture
    Unknown,
}

/// CPU capability class indicating supported instruction set extensions.
///
/// Classes are ordered by capability - higher enum variants indicate
/// more advanced SIMD capabilities and broader vectorization potential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CpuCapabilityClass {
    /// No SIMD support; scalar operations only
    Conservative,
    /// 64-bit scalar operations
    Scalar64,
    /// 128-bit SIMD (SSE/NEON)
    Simd128,
    /// 256-bit SIMD (AVX/AVX2)
    Simd256,
    /// 512-bit SIMD (AVX-512)
    Simd512,
}

impl CpuCapabilityClass {
    /// Returns true if this capability class supports SIMD vectorization.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert!(!CpuCapabilityClass::Conservative.supports_simd());
    /// assert!(CpuCapabilityClass::Simd256.supports_simd());
    /// ```
    pub const fn supports_simd(self) -> bool {
        matches!(self, Self::Simd128 | Self::Simd256 | Self::Simd512)
    }
}

/// CPU profile describing the hardware environment.
///
/// Contains architecture detection and capability information that determines
/// which execution strategies and optimizations are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuProfile {
    /// Detected CPU architecture
    pub architecture: HardwareArchitecture,
    /// CPU capability class (SIMD support, etc.)
    pub capability_class: CpuCapabilityClass,
    /// Number of hardware threads available
    pub hardware_threads: u16,
}

impl CpuProfile {
    /// Creates a conservative CPU profile with minimal capabilities.
    ///
    /// This is the safe default for systems where capabilities cannot be reliably detected.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = CpuProfile::conservative();
    /// assert_eq!(profile.hardware_threads, 1);
    /// assert!(!profile.supports_simd());
    /// ```
    pub const fn conservative() -> Self {
        Self {
            architecture: HardwareArchitecture::Unknown,
            capability_class: CpuCapabilityClass::Conservative,
            hardware_threads: 1,
        }
    }

    /// Returns true if this CPU profile supports SIMD vectorization.
    ///
    /// Useful for determining whether vectorized execution paths are available.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = CpuProfile::conservative();
    /// assert!(!profile.supports_simd());
    /// ```
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
