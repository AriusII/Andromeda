//! Optional acceleration admission policy for advisory analytical work.

use crate::{CpuCapabilityClass, CpuProfile, GpuProfile, PipelineClass};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Request to consider optional GPU execution for advisory analytical work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionalGpuRequest {
    pub pipeline: PipelineClass,
    pub cpu_fallback_available: bool,
    pub cancellation_boundary: bool,
}

impl OptionalGpuRequest {
    pub const fn new(pipeline: PipelineClass) -> Self {
        Self {
            pipeline,
            cpu_fallback_available: false,
            cancellation_boundary: false,
        }
    }

    pub const fn with_cpu_fallback(mut self) -> Self {
        self.cpu_fallback_available = true;
        self
    }

    pub const fn with_cancellation_boundary(mut self) -> Self {
        self.cancellation_boundary = true;
        self
    }
}

/// Selected execution path for optional GPU work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalGpuSelection {
    AdvisoryGpu,
    CpuFallback,
}

/// Decision emitted by the optional GPU boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionalGpuDecision {
    pub pipeline: PipelineClass,
    pub selection: OptionalGpuSelection,
    pub advisory_only: bool,
}

/// Validates and selects optional GPU execution.
pub fn select_optional_gpu(
    profile: GpuProfile,
    request: OptionalGpuRequest,
) -> AndromedaResult<OptionalGpuDecision> {
    reject_c5_pipeline(request.pipeline, "GPU")?;

    let selected = profile.select_advisory_gpu(
        request.pipeline,
        request.cpu_fallback_available,
        request.cancellation_boundary,
    )?;

    let selection = if selected {
        OptionalGpuSelection::AdvisoryGpu
    } else {
        OptionalGpuSelection::CpuFallback
    };

    Ok(OptionalGpuDecision {
        pipeline: request.pipeline,
        selection,
        advisory_only: true,
    })
}

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
            },
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

/// Advisory vector workload class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorAdvisoryKind {
    SimilarityRanking,
    CandidatePruning,
    MapRefreshHint,
}

/// Request to accept vector output as advisory evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorAdvisoryRequest {
    pub pipeline: PipelineClass,
    pub kind: VectorAdvisoryKind,
    pub source_truth_available: bool,
    pub bounded_candidates: bool,
    pub advisory_only: bool,
}

impl VectorAdvisoryRequest {
    pub const fn new(pipeline: PipelineClass, kind: VectorAdvisoryKind) -> Self {
        Self {
            pipeline,
            kind,
            source_truth_available: false,
            bounded_candidates: false,
            advisory_only: false,
        }
    }

    pub const fn with_source_truth(mut self) -> Self {
        self.source_truth_available = true;
        self
    }

    pub const fn with_bounded_candidates(mut self) -> Self {
        self.bounded_candidates = true;
        self
    }

    pub const fn advisory_only(mut self) -> Self {
        self.advisory_only = true;
        self
    }
}

/// Decision emitted by the vector advisory boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorAdvisoryDecision {
    pub pipeline: PipelineClass,
    pub kind: VectorAdvisoryKind,
    pub advisory_only: bool,
    pub may_publish_as_truth: bool,
}

/// Validates vector output as advisory-only evidence.
pub fn validate_vector_advisory(
    request: VectorAdvisoryRequest,
) -> AndromedaResult<VectorAdvisoryDecision> {
    reject_c5_pipeline(request.pipeline, "vector")?;

    if !request.pipeline.is_optional_acceleration_candidate() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            format!(
                "vector advisory output is not permitted for {} pipeline",
                request.pipeline.name()
            ),
        ));
    }

    if !request.source_truth_available {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            "vector advisory output requires independent source truth",
        ));
    }

    if !request.bounded_candidates {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            "vector advisory output requires bounded candidates",
        ));
    }

    if !request.advisory_only {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Resource,
            "vector output must remain advisory-only",
        ));
    }

    Ok(VectorAdvisoryDecision {
        pipeline: request.pipeline,
        kind: request.kind,
        advisory_only: true,
        may_publish_as_truth: false,
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
    use crate::HardwareArchitecture;

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

    fn valid_vector_request(pipeline: PipelineClass) -> VectorAdvisoryRequest {
        VectorAdvisoryRequest::new(pipeline, VectorAdvisoryKind::SimilarityRanking)
            .with_source_truth()
            .with_bounded_candidates()
            .advisory_only()
    }

    #[test]
    fn optional_gpu_rejects_every_c5_truth_path() {
        let profile = GpuProfile::off_critical_path();

        for pipeline in c5_pipelines() {
            let request = OptionalGpuRequest::new(pipeline)
                .with_cpu_fallback()
                .with_cancellation_boundary();
            let error = select_optional_gpu(profile, request).unwrap_err();

            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(error.message().contains("C5"));
            assert!(error.message().contains(pipeline.name()));
        }
    }

    #[test]
    fn optional_gpu_requires_authoritative_cpu_fallback() {
        let request =
            OptionalGpuRequest::new(PipelineClass::BatchAnalytics).with_cancellation_boundary();
        let error = select_optional_gpu(GpuProfile::batch_analytics_only(), request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("CPU fallback"));
    }

    #[test]
    fn optional_gpu_requires_cancellation_boundary() {
        let request = OptionalGpuRequest::new(PipelineClass::BatchAnalytics).with_cpu_fallback();
        let error = select_optional_gpu(GpuProfile::batch_analytics_only(), request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("cancellation boundary"));
    }

    #[test]
    fn disabled_gpu_selects_cpu_fallback_for_advisory_work() {
        let request = OptionalGpuRequest::new(PipelineClass::BatchAnalytics)
            .with_cpu_fallback()
            .with_cancellation_boundary();
        let decision = select_optional_gpu(GpuProfile::disabled(), request).unwrap();

        assert_eq!(decision.selection, OptionalGpuSelection::CpuFallback);
        assert!(decision.advisory_only);
    }

    #[test]
    fn enabled_gpu_can_only_select_advisory_gpu_for_advisory_work() {
        let request = OptionalGpuRequest::new(PipelineClass::BatchAnalytics)
            .with_cpu_fallback()
            .with_cancellation_boundary();
        let decision = select_optional_gpu(GpuProfile::batch_analytics_only(), request).unwrap();

        assert_eq!(decision.selection, OptionalGpuSelection::AdvisoryGpu);
        assert!(decision.advisory_only);
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

    #[test]
    fn vector_advisory_rejects_every_c5_truth_path() {
        for pipeline in c5_pipelines() {
            let error = validate_vector_advisory(valid_vector_request(pipeline)).unwrap_err();

            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(error.message().contains("C5"));
            assert!(error.message().contains(pipeline.name()));
        }
    }

    #[test]
    fn vector_advisory_rejects_non_advisory_non_c5_pipeline() {
        let error =
            validate_vector_advisory(valid_vector_request(PipelineClass::ForegroundExecution))
                .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("foreground_execution"));
    }

    #[test]
    fn vector_advisory_requires_independent_source_truth() {
        let request = VectorAdvisoryRequest::new(
            PipelineClass::BatchAnalytics,
            VectorAdvisoryKind::SimilarityRanking,
        )
        .with_bounded_candidates()
        .advisory_only();
        let error = validate_vector_advisory(request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("source truth"));
    }

    #[test]
    fn vector_advisory_requires_bounded_candidates() {
        let request = VectorAdvisoryRequest::new(
            PipelineClass::BatchAnalytics,
            VectorAdvisoryKind::SimilarityRanking,
        )
        .with_source_truth()
        .advisory_only();
        let error = validate_vector_advisory(request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("bounded candidates"));
    }

    #[test]
    fn vector_output_must_remain_advisory_only() {
        let request = VectorAdvisoryRequest::new(
            PipelineClass::BatchAnalytics,
            VectorAdvisoryKind::SimilarityRanking,
        )
        .with_source_truth()
        .with_bounded_candidates();
        let error = validate_vector_advisory(request).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains("advisory-only"));
    }

    #[test]
    fn vector_advisory_never_publishes_truth() {
        let decision =
            validate_vector_advisory(valid_vector_request(PipelineClass::BatchAnalytics)).unwrap();

        assert!(decision.advisory_only);
        assert!(!decision.may_publish_as_truth);
    }
}
