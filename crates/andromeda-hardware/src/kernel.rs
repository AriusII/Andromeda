//! CPU kernel registry and runtime dispatch policy.

use crate::{CpuProfile, CpuRuntimeProfile};

const CPU_KERNEL_KINDS: [CpuKernelKind; 4] = [
    CpuKernelKind::Crc32,
    CpuKernelKind::Hash,
    CpuKernelKind::Compression,
    CpuKernelKind::Scan,
];

const CPU_KERNEL_VARIANTS: [CpuKernelVariant; 6] = [
    CpuKernelVariant::Scalar,
    CpuKernelVariant::X64Baseline,
    CpuKernelVariant::X64Avx2,
    CpuKernelVariant::X64Avx512,
    CpuKernelVariant::Arm64Neon,
    CpuKernelVariant::Arm64Sve2,
];

/// CPU kernel families that may use runtime dispatch while preserving scalar fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CpuKernelKind {
    Crc32,
    Hash,
    Compression,
    Scan,
}

impl CpuKernelKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Crc32 => "crc32",
            Self::Hash => "hash",
            Self::Compression => "compression",
            Self::Scan => "scan",
        }
    }
}

/// Concrete kernel variant selected for a CPU kernel family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CpuKernelVariant {
    Scalar,
    X64Baseline,
    X64Avx2,
    X64Avx512,
    Arm64Neon,
    Arm64Sve2,
}

impl CpuKernelVariant {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::X64Baseline => "x64-baseline",
            Self::X64Avx2 => "x64-avx2",
            Self::X64Avx512 => "x64-avx512",
            Self::Arm64Neon => "arm64-neon",
            Self::Arm64Sve2 => "arm64-sve2",
        }
    }

    pub const fn is_scalar(self) -> bool {
        matches!(self, Self::Scalar)
    }

    pub const fn supports_runtime_profile(self, runtime_profile: CpuRuntimeProfile) -> bool {
        matches!(
            (self, runtime_profile),
            (Self::Scalar, _)
                | (
                    Self::X64Baseline,
                    CpuRuntimeProfile::X64Baseline
                        | CpuRuntimeProfile::X64Avx2
                        | CpuRuntimeProfile::X64Avx512,
                )
                | (
                    Self::X64Avx2,
                    CpuRuntimeProfile::X64Avx2 | CpuRuntimeProfile::X64Avx512,
                )
                | (Self::X64Avx512, CpuRuntimeProfile::X64Avx512)
                | (
                    Self::Arm64Neon,
                    CpuRuntimeProfile::Arm64Neon | CpuRuntimeProfile::Arm64Sve2,
                )
                | (Self::Arm64Sve2, CpuRuntimeProfile::Arm64Sve2)
        )
    }
}

/// Runtime dispatch decision for a CPU kernel family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuKernelDispatch {
    pub kernel: CpuKernelKind,
    pub runtime_profile: CpuRuntimeProfile,
    pub selected_variant: CpuKernelVariant,
    pub scalar_fallback_variant: CpuKernelVariant,
    pub runtime_detected: bool,
}

impl CpuKernelDispatch {
    pub const fn uses_scalar_fallback(self) -> bool {
        self.selected_variant.is_scalar()
    }
}

/// Registry of supported CPU kernel families and their dispatch variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuKernelRegistry;

impl CpuKernelRegistry {
    pub const fn v0() -> Self {
        Self
    }

    pub const fn kernels(self) -> [CpuKernelKind; 4] {
        CPU_KERNEL_KINDS
    }

    pub const fn variants(self, _kernel: CpuKernelKind) -> &'static [CpuKernelVariant] {
        &CPU_KERNEL_VARIANTS
    }

    pub fn dispatch_for_cpu(self, kernel: CpuKernelKind, cpu: CpuProfile) -> CpuKernelDispatch {
        self.dispatch_for_runtime_profile(kernel, cpu.runtime_profile())
    }

    pub fn dispatch_for_runtime_profile(
        self,
        kernel: CpuKernelKind,
        runtime_profile: CpuRuntimeProfile,
    ) -> CpuKernelDispatch {
        let selected_variant = self
            .variants(kernel)
            .iter()
            .rev()
            .copied()
            .find(|variant| variant.supports_runtime_profile(runtime_profile))
            .unwrap_or(CpuKernelVariant::Scalar);

        CpuKernelDispatch {
            kernel,
            runtime_profile,
            selected_variant,
            scalar_fallback_variant: CpuKernelVariant::Scalar,
            runtime_detected: false,
        }
    }

    pub fn detect_and_dispatch(self, kernel: CpuKernelKind) -> CpuKernelDispatch {
        let mut dispatch = self.dispatch_for_cpu(kernel, CpuProfile::detect());
        dispatch.runtime_detected = true;
        dispatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_all_required_kernel_families() {
        let registry = CpuKernelRegistry::v0();
        let kernels = registry.kernels();

        assert_eq!(kernels.len(), 4);
        assert!(kernels.contains(&CpuKernelKind::Crc32));
        assert!(kernels.contains(&CpuKernelKind::Hash));
        assert!(kernels.contains(&CpuKernelKind::Compression));
        assert!(kernels.contains(&CpuKernelKind::Scan));

        for kernel in kernels {
            let variants = registry.variants(kernel);
            assert!(variants.contains(&CpuKernelVariant::Scalar));
            assert!(variants.contains(&CpuKernelVariant::X64Baseline));
            assert!(variants.contains(&CpuKernelVariant::X64Avx2));
            assert!(variants.contains(&CpuKernelVariant::X64Avx512));
            assert!(variants.contains(&CpuKernelVariant::Arm64Neon));
            assert!(variants.contains(&CpuKernelVariant::Arm64Sve2));
        }
    }

    #[test]
    fn dispatch_uses_scalar_fallback_for_conservative_profile() {
        let dispatch = CpuKernelRegistry::v0()
            .dispatch_for_runtime_profile(CpuKernelKind::Scan, CpuRuntimeProfile::Conservative);

        assert_eq!(dispatch.selected_variant, CpuKernelVariant::Scalar);
        assert!(dispatch.uses_scalar_fallback());
        assert_eq!(dispatch.scalar_fallback_variant, CpuKernelVariant::Scalar);
    }

    #[test]
    fn dispatch_selects_highest_supported_x64_kernel() {
        let registry = CpuKernelRegistry::v0();

        assert_eq!(
            registry
                .dispatch_for_runtime_profile(CpuKernelKind::Hash, CpuRuntimeProfile::X64Baseline)
                .selected_variant,
            CpuKernelVariant::X64Baseline
        );
        assert_eq!(
            registry
                .dispatch_for_runtime_profile(CpuKernelKind::Hash, CpuRuntimeProfile::X64Avx2)
                .selected_variant,
            CpuKernelVariant::X64Avx2
        );
        assert_eq!(
            registry
                .dispatch_for_runtime_profile(CpuKernelKind::Hash, CpuRuntimeProfile::X64Avx512)
                .selected_variant,
            CpuKernelVariant::X64Avx512
        );
    }

    #[test]
    fn dispatch_selects_highest_supported_arm64_kernel() {
        let registry = CpuKernelRegistry::v0();

        assert_eq!(
            registry
                .dispatch_for_runtime_profile(
                    CpuKernelKind::Compression,
                    CpuRuntimeProfile::Arm64Neon
                )
                .selected_variant,
            CpuKernelVariant::Arm64Neon
        );
        assert_eq!(
            registry
                .dispatch_for_runtime_profile(
                    CpuKernelKind::Compression,
                    CpuRuntimeProfile::Arm64Sve2
                )
                .selected_variant,
            CpuKernelVariant::Arm64Sve2
        );
    }

    #[test]
    fn detect_and_dispatch_marks_runtime_detection() {
        let dispatch = CpuKernelRegistry::v0().detect_and_dispatch(CpuKernelKind::Crc32);

        assert!(dispatch.runtime_detected);
        assert_eq!(dispatch.scalar_fallback_variant, CpuKernelVariant::Scalar);
    }
}
