//! Verifies that `select_optional_gpu` rejects every C5 pipeline class.
//!
//! C5 durable-kernel pipelines (`Commit`, `WalAppend`, `Rollback`, `Recovery`,
//! `MvccVisibility`, `CatalogPublication`, `SecurityCriticalPath`) must never be
//! accepted for optional GPU execution. This invariant is enforced by
//! `andromeda-hardware` and exposed through `andromeda-gpu::policy`.

use andromeda_error::AndromedaErrorKind;
use andromeda_gpu::policy::{GpuProfile, OptionalGpuRequest, PipelineClass, select_optional_gpu};

const C5_PIPELINES: [PipelineClass; 7] = [
    PipelineClass::Commit,
    PipelineClass::WalAppend,
    PipelineClass::Rollback,
    PipelineClass::Recovery,
    PipelineClass::MvccVisibility,
    PipelineClass::CatalogPublication,
    PipelineClass::SecurityCriticalPath,
];

#[test]
fn select_optional_gpu_rejects_every_c5_pipeline_with_resource_error() {
    let profile = GpuProfile::batch_analytics_only();

    for pipeline in C5_PIPELINES {
        let request = OptionalGpuRequest::new(pipeline)
            .with_cpu_fallback()
            .with_cancellation_boundary();

        let err = select_optional_gpu(profile, request).unwrap_err();

        assert_eq!(
            err.kind(),
            AndromedaErrorKind::Resource,
            "expected Resource error for C5 pipeline {:?}, got {:?}",
            pipeline,
            err.kind()
        );
        assert!(
            err.message().contains("C5"),
            "error message must mention 'C5' for pipeline {:?}: {:?}",
            pipeline,
            err.message()
        );
        assert!(
            err.message().contains(pipeline.name()),
            "error message must include pipeline name '{}': {:?}",
            pipeline.name(),
            err.message()
        );
    }
}

#[test]
fn off_critical_path_profile_also_rejects_every_c5_pipeline() {
    let profile = GpuProfile::off_critical_path();

    for pipeline in C5_PIPELINES {
        let request = OptionalGpuRequest::new(pipeline)
            .with_cpu_fallback()
            .with_cancellation_boundary();

        let result = select_optional_gpu(profile, request);
        assert!(
            result.is_err(),
            "off_critical_path profile must reject C5 pipeline {pipeline:?}"
        );
    }
}

#[test]
fn disabled_profile_also_rejects_every_c5_pipeline() {
    let profile = GpuProfile::disabled();

    for pipeline in C5_PIPELINES {
        let request = OptionalGpuRequest::new(pipeline)
            .with_cpu_fallback()
            .with_cancellation_boundary();

        let result = select_optional_gpu(profile, request);
        assert!(
            result.is_err(),
            "disabled profile must reject C5 pipeline {pipeline:?}"
        );
    }
}
