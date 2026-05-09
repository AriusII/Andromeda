use andromeda_hardware::{GpuProfile, PipelineClass};
use andromeda_storage_page::PageSize;
use andromeda_storage_placement::{
    IoPathClass, IoUseClass, OperationalProfile, OperationalProfileMode,
};

#[test]
fn conservative_operational_profile_uses_cpu_ram_hotstore_defaults() {
    let profile = OperationalProfile::conservative();

    assert_eq!(profile.mode, OperationalProfileMode::Conservative);
    assert!(!profile.hardware.has_simd);
    assert_eq!(profile.hardware.gpu, GpuProfile::disabled());
    assert_eq!(profile.workflow.page_budget.page_size, PageSize::KiB16);
    assert_eq!(
        profile.workflow.page_budget.path_budget.path_class,
        IoPathClass::HotPathNvmeSsd
    );
    assert_eq!(
        profile.workflow.segment_budget.path_budget.path_class,
        IoPathClass::HotPathNvmeSsd
    );
    assert!(profile.validate().is_ok());
}

#[test]
fn analytics_operational_profile_keeps_gpu_off_critical_path() {
    let profile = OperationalProfile::analytics_off_critical_path();

    assert_eq!(
        profile.mode,
        OperationalProfileMode::AnalyticsOffCriticalPath
    );
    assert!(
        profile
            .validate_gpu_pipeline(PipelineClass::BatchAnalytics)
            .is_ok()
    );
    assert!(
        profile
            .validate_gpu_pipeline(PipelineClass::StatisticsRefresh)
            .is_ok()
    );
    assert!(
        profile
            .validate_gpu_pipeline(PipelineClass::MapRefresh)
            .is_ok()
    );

    for pipeline in [
        PipelineClass::Commit,
        PipelineClass::WalAppend,
        PipelineClass::Rollback,
        PipelineClass::Recovery,
        PipelineClass::ForegroundExecution,
        PipelineClass::BackgroundMaintenance,
    ] {
        assert!(profile.validate_gpu_pipeline(pipeline).is_err());
    }

    assert!(profile.validate().is_ok());
}

#[test]
fn cold_archive_operational_profile_routes_segments_to_coldstore_contract() {
    let profile = OperationalProfile::cold_archive();

    assert_eq!(
        profile.workflow.page_budget.path_budget.path_class,
        IoPathClass::HotPathNvmeSsd
    );
    assert_eq!(
        profile.workflow.segment_budget.use_class,
        IoUseClass::ColdSegmentPath
    );
    assert_eq!(
        profile.workflow.segment_budget.path_budget.path_class,
        IoPathClass::ColdPathHdd
    );
    assert!(profile.validate().is_ok());
}
