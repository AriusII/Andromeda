use std::num::NonZeroU64;

use andromeda_analytics::{
    AdvisoryAnalyticsJob, AnalyticsAccelerationPolicy, AnalyticsExecutionBounds,
    AnalyticsJobDescriptor, AnalyticsWorkloadKind,
};

#[test]
fn analytics_job_descriptor_is_advisory_and_bounded() {
    let descriptor = AnalyticsJobDescriptor::new(
        AnalyticsWorkloadKind::MapRefresh,
        AnalyticsExecutionBounds::new(
            NonZeroU64::new(1_000).unwrap(),
            NonZeroU64::new(64 * 1024).unwrap(),
            true,
        ),
        AnalyticsAccelerationPolicy::OptionalAcceleratorWithCpuFallback,
    );

    assert_eq!(
        descriptor.workload_kind(),
        AnalyticsWorkloadKind::MapRefresh
    );
    assert!(descriptor.execution_bounds().is_bounded());
    assert!(descriptor.acceleration_policy().has_cpu_fallback());
    assert!(descriptor.requires_decision_trace());
    assert!(!descriptor.is_source_truth());
    assert!(!descriptor.is_c5_critical_path());
    assert!(descriptor.is_admissible_advisory_job());
}

#[test]
fn analytics_job_requires_cancellation_to_be_admissible() {
    let descriptor = AnalyticsJobDescriptor::new(
        AnalyticsWorkloadKind::OperationalDiagnostics,
        AnalyticsExecutionBounds::new(NonZeroU64::MIN, NonZeroU64::MIN, false),
        AnalyticsAccelerationPolicy::CpuOnly,
    );

    assert!(!descriptor.execution_bounds().is_bounded());
    assert!(!descriptor.is_admissible_advisory_job());
}
