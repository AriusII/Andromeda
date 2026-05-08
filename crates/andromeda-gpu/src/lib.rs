#![forbid(unsafe_code)]

//! Optional GPU advisory boundary.
//!
//! GPU work is optional and advisory. It must never become commit, WAL,
//! rollback, recovery, MVCC visibility, catalog publication, or
//! security-critical truth.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_hardware::{GpuProfile, PipelineClass};

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
}
