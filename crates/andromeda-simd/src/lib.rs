#![forbid(unsafe_code)]

//! Optional SIMD dispatch boundary.
//!
//! SIMD is an optional CPU acceleration path. Scalar execution remains the
//! authoritative fallback, and optional SIMD must not be selected for C5 truth
//! paths.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_hardware::{CpuCapabilityClass, CpuProfile, PipelineClass};

/// Request to consider optional SIMD for advisory analytical work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimdDispatchRequest {
    pub pipeline: PipelineClass,
    pub scalar_fallback_available: bool,
    pub simd_enabled: bool,
}

impl SimdDispatchRequest {
    pub const fn new(pipeline: PipelineClass) -> Self {
        Self {
            pipeline,
            scalar_fallback_available: false,
            simd_enabled: false,
        }
    }

    pub const fn with_scalar_fallback(mut self) -> Self {
        self.scalar_fallback_available = true;
        self
    }

    pub const fn with_simd_enabled(mut self) -> Self {
        self.simd_enabled = true;
        self
    }
}

/// Selected SIMD dispatch mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdExecutionMode {
    ScalarFallback,
    Simd128,
    Simd256,
    Simd512,
}

/// Decision emitted by the SIMD dispatch boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimdDispatchDecision {
    pub pipeline: PipelineClass,
    pub mode: SimdExecutionMode,
    pub scalar_fallback_available: bool,
}

/// Validates and selects optional SIMD execution.
pub fn select_simd_dispatch(
    cpu: CpuProfile,
    request: SimdDispatchRequest,
) -> AndromedaResult<SimdDispatchDecision> {
    reject_c5_pipeline(request.pipeline, "SIMD")?;

    if !request.pipeline.is_optional_acceleration_candidate() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            format!(
                "optional SIMD execution is not permitted for {} pipeline",
                request.pipeline.name()
            ),
        ));
    }

    if !request.scalar_fallback_available {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            "optional SIMD execution requires scalar CPU fallback",
        ));
    }

    let mode = if request.simd_enabled {
        match cpu.capability_class {
            CpuCapabilityClass::Simd128 => SimdExecutionMode::Simd128,
            CpuCapabilityClass::Simd256 => SimdExecutionMode::Simd256,
            CpuCapabilityClass::Simd512 => SimdExecutionMode::Simd512,
            CpuCapabilityClass::Conservative | CpuCapabilityClass::Scalar64 => {
                SimdExecutionMode::ScalarFallback
            }
        }
    } else {
        SimdExecutionMode::ScalarFallback
    };

    Ok(SimdDispatchDecision {
        pipeline: request.pipeline,
        mode,
        scalar_fallback_available: true,
    })
}

fn reject_c5_pipeline(pipeline: PipelineClass, accelerator: &str) -> AndromedaResult<()> {
    if pipeline.is_c5_truth_path() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            format!(
                "optional {accelerator} execution is forbidden on C5 {} pipeline",
                pipeline.name()
            ),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_hardware::HardwareArchitecture;

    fn c5_pipelines() -> [PipelineClass; 7] {
        [
            PipelineClass::Commit,
            PipelineClass::WalAppend,
            PipelineClass::Rollback,
            PipelineClass::Recovery,
            PipelineClass::MvccVisibility,
            PipelineClass::CatalogPublication,
            PipelineClass::SecurityCriticalPath,
        ]
    }

    fn cpu_with(capability_class: CpuCapabilityClass) -> CpuProfile {
        CpuProfile {
            architecture: HardwareArchitecture::X64,
            capability_class,
            hardware_threads: 8,
        }
    }

    #[test]
    fn optional_simd_rejects_every_c5_truth_path() {
        let cpu = cpu_with(CpuCapabilityClass::Simd256);

        for pipeline in c5_pipelines() {
            let request = SimdDispatchRequest::new(pipeline)
                .with_scalar_fallback()
                .with_simd_enabled();
            let error = select_simd_dispatch(cpu, request).unwrap_err();

            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(error.message().contains("C5"));
            assert!(error.message().contains(pipeline.name()));
        }
    }

    #[test]
    fn optional_simd_requires_scalar_fallback() {
        let request = SimdDispatchRequest::new(PipelineClass::BatchAnalytics).with_simd_enabled();
        let error =
            select_simd_dispatch(cpu_with(CpuCapabilityClass::Simd256), request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("scalar CPU fallback"));
    }

    #[test]
    fn optional_simd_uses_scalar_when_disabled_or_not_supported() {
        let request =
            SimdDispatchRequest::new(PipelineClass::BatchAnalytics).with_scalar_fallback();
        let disabled =
            select_simd_dispatch(cpu_with(CpuCapabilityClass::Simd256), request).unwrap();
        assert_eq!(disabled.mode, SimdExecutionMode::ScalarFallback);

        let request = request.with_simd_enabled();
        let conservative =
            select_simd_dispatch(cpu_with(CpuCapabilityClass::Conservative), request).unwrap();
        assert_eq!(conservative.mode, SimdExecutionMode::ScalarFallback);
    }

    #[test]
    fn optional_simd_selects_matching_supported_width() {
        let request = SimdDispatchRequest::new(PipelineClass::BatchAnalytics)
            .with_scalar_fallback()
            .with_simd_enabled();

        assert_eq!(
            select_simd_dispatch(cpu_with(CpuCapabilityClass::Simd128), request)
                .unwrap()
                .mode,
            SimdExecutionMode::Simd128
        );
        assert_eq!(
            select_simd_dispatch(cpu_with(CpuCapabilityClass::Simd256), request)
                .unwrap()
                .mode,
            SimdExecutionMode::Simd256
        );
        assert_eq!(
            select_simd_dispatch(cpu_with(CpuCapabilityClass::Simd512), request)
                .unwrap()
                .mode,
            SimdExecutionMode::Simd512
        );
    }

    #[test]
    fn optional_simd_rejects_non_advisory_non_c5_pipeline() {
        let request = SimdDispatchRequest::new(PipelineClass::ForegroundExecution)
            .with_scalar_fallback()
            .with_simd_enabled();
        let error =
            select_simd_dispatch(cpu_with(CpuCapabilityClass::Simd256), request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("foreground_execution"));
    }
}
