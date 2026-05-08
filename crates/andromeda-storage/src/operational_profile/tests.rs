use andromeda_core::{AndromedaErrorKind, CpuCapabilityClass, GpuProfile, PipelineClass};

use crate::{HotColdIoThresholds, IoPathClass, IoUseClass, PageSize};

use super::*;

#[test]
fn operational_presets_validate_as_contracts() {
    for profile in [
        OperationalProfile::conservative(),
        OperationalProfile::hot_write(),
        OperationalProfile::cold_archive(),
        OperationalProfile::analytics_off_critical_path(),
    ] {
        assert!(profile.validate().is_ok(), "{:?}", profile.mode);
    }
}

#[test]
fn conservative_profile_uses_cpu_ram_and_hotstore_safe_defaults() {
    let profile = OperationalProfile::conservative();

    assert_eq!(
        profile.hardware.cpu.capability_class,
        CpuCapabilityClass::Conservative
    );
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
    assert!(profile.hardware.ram.declared_section_bytes() <= profile.hardware.ram.total_bytes);
    assert!(profile.validate().is_ok());
}

#[test]
fn cold_archive_profile_uses_coldstore_only_for_segment_work() {
    let profile = OperationalProfile::cold_archive();

    assert_eq!(
        profile.workflow.page_budget.path_budget.path_class,
        IoPathClass::HotPathNvmeSsd
    );
    assert_eq!(
        profile.workflow.segment_budget.path_budget.path_class,
        IoPathClass::ColdPathHdd
    );
    assert_eq!(
        profile.workflow.segment_budget.use_class,
        IoUseClass::ColdSegmentPath
    );
    assert!(profile.validate().is_ok());
}

#[test]
fn analytics_gpu_profile_permits_only_off_critical_path_analytics_pipelines() {
    let profile = OperationalProfile::analytics_off_critical_path();

    for pipeline in [
        PipelineClass::StatisticsRefresh,
        PipelineClass::MapRefresh,
        PipelineClass::BatchAnalytics,
    ] {
        assert!(profile.validate_gpu_pipeline(pipeline).is_ok());
    }

    for pipeline in [
        PipelineClass::Commit,
        PipelineClass::WalAppend,
        PipelineClass::Rollback,
        PipelineClass::Recovery,
        PipelineClass::MvccVisibility,
        PipelineClass::CatalogPublication,
        PipelineClass::SecurityCriticalPath,
        PipelineClass::ForegroundExecution,
        PipelineClass::BackgroundMaintenance,
    ] {
        assert!(profile.validate_gpu_pipeline(pipeline).is_err());
    }

    assert!(profile.validate().is_ok());
}

#[test]
fn validation_rejects_mode_or_threshold_drift() {
    let mut profile = OperationalProfile::hot_write();
    profile.workflow.mode = OperationalProfileMode::Conservative;
    assert_eq!(
        profile.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );

    let mut workflow = IoWorkflowProfile::cold_archive();
    workflow.thresholds = HotColdIoThresholds::new(1, 1);
    assert_eq!(
        workflow.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}
