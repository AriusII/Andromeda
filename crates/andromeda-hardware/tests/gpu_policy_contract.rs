use andromeda_error::AndromedaErrorKind;
use andromeda_hardware::{GpuExecutionPolicy, GpuProfile, PipelineClass};

fn critical_truth_paths() -> [PipelineClass; 7] {
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

fn advisory_gpu_pipelines() -> [PipelineClass; 3] {
    [
        PipelineClass::StatisticsRefresh,
        PipelineClass::MapRefresh,
        PipelineClass::BatchAnalytics,
    ]
}

#[test]
fn gpu_policies_never_permit_critical_truth_paths() {
    for policy in [
        GpuExecutionPolicy::OffCriticalPathOnly,
        GpuExecutionPolicy::BatchAnalyticsOnly,
    ] {
        for pipeline in critical_truth_paths() {
            let error = policy.validate_pipeline(pipeline).unwrap_err();

            assert_eq!(error.kind(), AndromedaErrorKind::Resource);
            assert!(!policy.permits_pipeline(pipeline));
        }
    }
}

#[test]
fn off_critical_gpu_policy_is_advisory_only() {
    let policy = GpuExecutionPolicy::OffCriticalPathOnly;

    for pipeline in advisory_gpu_pipelines() {
        assert!(policy.validate_pipeline(pipeline).is_ok());
        assert!(policy.permits_pipeline(pipeline));
    }

    for pipeline in [
        PipelineClass::ForegroundExecution,
        PipelineClass::BackgroundMaintenance,
    ] {
        let error = policy.validate_pipeline(pipeline).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(!policy.permits_pipeline(pipeline));
    }
}

#[test]
fn advisory_gpu_selection_requires_cpu_fallback_and_cancellation_boundary() {
    let profile = GpuProfile::batch_analytics_only();

    let missing_fallback = profile
        .select_advisory_gpu(PipelineClass::BatchAnalytics, false, true)
        .unwrap_err();
    assert_eq!(missing_fallback.kind(), AndromedaErrorKind::Resource);
    assert!(missing_fallback.message().contains("CPU fallback"));

    let missing_cancellation = profile
        .select_advisory_gpu(PipelineClass::BatchAnalytics, true, false)
        .unwrap_err();
    assert_eq!(missing_cancellation.kind(), AndromedaErrorKind::Resource);
    assert!(
        missing_cancellation
            .message()
            .contains("cancellation boundary")
    );
}

#[test]
fn gpu_disablement_selects_cpu_fallback_for_advisory_work() {
    let selected = GpuProfile::disabled()
        .select_advisory_gpu(PipelineClass::BatchAnalytics, true, true)
        .unwrap();

    assert!(!selected);
}

#[test]
fn gpu_selection_rejects_critical_truth_paths_even_with_guards() {
    let profile = GpuProfile::off_critical_path();

    for pipeline in critical_truth_paths() {
        let error = profile
            .select_advisory_gpu(pipeline, true, true)
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains(pipeline.name()));
    }
}

#[test]
fn gpu_selection_accepts_only_advisory_pipelines_with_guards() {
    let profile = GpuProfile::batch_analytics_only();

    for pipeline in advisory_gpu_pipelines() {
        let selected = profile.select_advisory_gpu(pipeline, true, true).unwrap();

        assert!(selected);
    }

    for pipeline in [
        PipelineClass::ForegroundExecution,
        PipelineClass::BackgroundMaintenance,
    ] {
        let error = profile
            .select_advisory_gpu(pipeline, true, true)
            .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
        assert!(error.message().contains(pipeline.name()));
    }
}
