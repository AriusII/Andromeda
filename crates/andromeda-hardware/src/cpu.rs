//! CPU hardware capabilities and profiles.

use std::thread;

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

/// Explicit runtime hardware profiles required by P11 CPU dispatch policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CpuRuntimeProfile {
    Conservative,
    X64Baseline,
    X64Avx2,
    X64Avx512,
    Arm64Neon,
    Arm64Sve2,
}

impl CpuRuntimeProfile {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Conservative => "conservative",
            Self::X64Baseline => "x64-baseline",
            Self::X64Avx2 => "x64-avx2",
            Self::X64Avx512 => "x64-avx512",
            Self::Arm64Neon => "arm64-neon",
            Self::Arm64Sve2 => "arm64-sve2",
        }
    }

    pub const fn architecture(self) -> HardwareArchitecture {
        match self {
            Self::Conservative => HardwareArchitecture::Unknown,
            Self::X64Baseline | Self::X64Avx2 | Self::X64Avx512 => HardwareArchitecture::X64,
            Self::Arm64Neon | Self::Arm64Sve2 => HardwareArchitecture::Arm64,
        }
    }

    pub const fn capability_class(self) -> CpuCapabilityClass {
        match self {
            Self::Conservative => CpuCapabilityClass::Conservative,
            Self::X64Baseline | Self::Arm64Neon => CpuCapabilityClass::Simd128,
            Self::X64Avx2 | Self::Arm64Sve2 => CpuCapabilityClass::Simd256,
            Self::X64Avx512 => CpuCapabilityClass::Simd512,
        }
    }

    pub const fn supports_simd(self) -> bool {
        self.capability_class().supports_simd()
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

    pub const fn from_runtime_profile(
        runtime_profile: CpuRuntimeProfile,
        hardware_threads: u16,
    ) -> Self {
        Self {
            architecture: runtime_profile.architecture(),
            capability_class: runtime_profile.capability_class(),
            hardware_threads: normalize_threads(hardware_threads),
        }
    }

    pub const fn x64_baseline(hardware_threads: u16) -> Self {
        Self::from_runtime_profile(CpuRuntimeProfile::X64Baseline, hardware_threads)
    }

    pub const fn x64_avx2(hardware_threads: u16) -> Self {
        Self::from_runtime_profile(CpuRuntimeProfile::X64Avx2, hardware_threads)
    }

    pub const fn x64_avx512(hardware_threads: u16) -> Self {
        Self::from_runtime_profile(CpuRuntimeProfile::X64Avx512, hardware_threads)
    }

    pub const fn arm64_neon(hardware_threads: u16) -> Self {
        Self::from_runtime_profile(CpuRuntimeProfile::Arm64Neon, hardware_threads)
    }

    pub const fn arm64_sve2(hardware_threads: u16) -> Self {
        Self::from_runtime_profile(CpuRuntimeProfile::Arm64Sve2, hardware_threads)
    }

    pub fn detect() -> Self {
        Self::from_runtime_profile(detect_runtime_profile(), detected_hardware_threads())
    }

    pub const fn runtime_profile(&self) -> CpuRuntimeProfile {
        match (self.architecture, self.capability_class) {
            (HardwareArchitecture::X64, CpuCapabilityClass::Simd512) => {
                CpuRuntimeProfile::X64Avx512
            },
            (HardwareArchitecture::X64, CpuCapabilityClass::Simd256) => CpuRuntimeProfile::X64Avx2,
            (HardwareArchitecture::X64, CpuCapabilityClass::Simd128) => {
                CpuRuntimeProfile::X64Baseline
            },
            (HardwareArchitecture::Arm64, CpuCapabilityClass::Simd256) => {
                CpuRuntimeProfile::Arm64Sve2
            },
            (HardwareArchitecture::Arm64, CpuCapabilityClass::Simd128) => {
                CpuRuntimeProfile::Arm64Neon
            },
            _ => CpuRuntimeProfile::Conservative,
        }
    }

    pub const fn supports_simd(&self) -> bool {
        self.capability_class.supports_simd()
    }
}

const fn normalize_threads(hardware_threads: u16) -> u16 {
    if hardware_threads == 0 {
        1
    } else {
        hardware_threads
    }
}

fn detected_hardware_threads() -> u16 {
    thread::available_parallelism()
        .map(|parallelism| parallelism.get().min(u16::MAX as usize) as u16)
        .unwrap_or(1)
}

fn detect_runtime_profile() -> CpuRuntimeProfile {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx512f") {
            CpuRuntimeProfile::X64Avx512
        } else if std::arch::is_x86_feature_detected!("avx2") {
            CpuRuntimeProfile::X64Avx2
        } else {
            CpuRuntimeProfile::X64Baseline
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("sve2") {
            CpuRuntimeProfile::Arm64Sve2
        } else {
            CpuRuntimeProfile::Arm64Neon
        }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        CpuRuntimeProfile::Conservative
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
    fn runtime_profile_names_are_stable() {
        assert_eq!(CpuRuntimeProfile::Conservative.name(), "conservative");
        assert_eq!(CpuRuntimeProfile::X64Baseline.name(), "x64-baseline");
        assert_eq!(CpuRuntimeProfile::X64Avx2.name(), "x64-avx2");
        assert_eq!(CpuRuntimeProfile::X64Avx512.name(), "x64-avx512");
        assert_eq!(CpuRuntimeProfile::Arm64Neon.name(), "arm64-neon");
        assert_eq!(CpuRuntimeProfile::Arm64Sve2.name(), "arm64-sve2");
    }

    #[test]
    fn runtime_profiles_cover_explicit_x64_and_arm64_buckets() {
        assert_eq!(
            CpuProfile::x64_baseline(8).runtime_profile(),
            CpuRuntimeProfile::X64Baseline
        );
        assert_eq!(
            CpuProfile::x64_avx2(8).runtime_profile(),
            CpuRuntimeProfile::X64Avx2
        );
        assert_eq!(
            CpuProfile::x64_avx512(8).runtime_profile(),
            CpuRuntimeProfile::X64Avx512
        );
        assert_eq!(
            CpuProfile::arm64_neon(8).runtime_profile(),
            CpuRuntimeProfile::Arm64Neon
        );
        assert_eq!(
            CpuProfile::arm64_sve2(8).runtime_profile(),
            CpuRuntimeProfile::Arm64Sve2
        );
    }

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
        assert_eq!(profile.runtime_profile(), CpuRuntimeProfile::Conservative);
    }

    #[test]
    fn simd_support_check_matches_class() {
        assert!(!CpuCapabilityClass::Conservative.supports_simd());
        assert!(!CpuCapabilityClass::Scalar64.supports_simd());
        assert!(CpuCapabilityClass::Simd128.supports_simd());
        assert!(CpuCapabilityClass::Simd256.supports_simd());
        assert!(CpuCapabilityClass::Simd512.supports_simd());
    }

    #[test]
    fn runtime_profile_detection_reports_current_platform_bucket() {
        let profile = CpuProfile::detect();

        assert!(profile.hardware_threads >= 1);

        #[cfg(target_arch = "x86_64")]
        assert!(matches!(
            profile.runtime_profile(),
            CpuRuntimeProfile::X64Baseline
                | CpuRuntimeProfile::X64Avx2
                | CpuRuntimeProfile::X64Avx512
        ));

        #[cfg(target_arch = "aarch64")]
        assert!(matches!(
            profile.runtime_profile(),
            CpuRuntimeProfile::Arm64Neon | CpuRuntimeProfile::Arm64Sve2
        ));

        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        assert_eq!(profile.runtime_profile(), CpuRuntimeProfile::Conservative);
    }
}
