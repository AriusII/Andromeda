//! Contract tests for `GpuJobClass::Benchmark` and GPU policy boundaries.

use andromeda_error::AndromedaErrorKind;
use andromeda_gpu::{
    job::GpuJobClass,
    policy::{GpuProfile, OptionalGpuRequest, PipelineClass, select_optional_gpu},
};

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
fn benchmark_job_class_is_advisory_only_and_never_publishable_without_validation() {
    assert!(
        !GpuJobClass::Benchmark.is_publishable_without_validation(),
        "benchmark GPU output must never be publishable before CPU validation"
    );
}

#[test]
fn optional_gpu_policy_stays_outside_c5_commit_recovery_and_security_paths() {
    let profile = GpuProfile::batch_analytics_only();

    for pipeline in C5_PIPELINES {
        assert!(
            !pipeline.is_gpu_advisory_candidate(),
            "C5 pipeline {pipeline:?} must not be a GPU advisory candidate"
        );

        let request = OptionalGpuRequest::new(pipeline)
            .with_cpu_fallback()
            .with_cancellation_boundary();
        let err = select_optional_gpu(profile, request).unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Resource);
        assert!(
            err.message().contains("C5"),
            "expected C5 rejection message for {pipeline:?}, got {:?}",
            err.message()
        );
    }
}
