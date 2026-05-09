use std::num::{NonZeroU16, NonZeroU64};

use andromeda_analytics::{
    AnalyticsAccelerationPolicy, AnalyticsExecutionBounds, AnalyticsJobDescriptor,
    AnalyticsWorkloadKind,
};
use andromeda_columnar::{
    ColumnarAccelerationPolicy, ColumnarConsumer, ColumnarLayoutDescriptor, ColumnarVersionBinding,
};
use andromeda_maps::{
    MapDescriptor, MapDescriptorError, MapGrain, MapId, MapRefreshMode, MapRefreshPlan,
    MapStalenessPolicy,
};

fn descriptor() -> MapDescriptor {
    MapDescriptor::new(
        MapId::new(17).unwrap(),
        MapGrain::Relation,
        MapRefreshMode::Deferred,
        MapStalenessPolicy::AdvisorySnapshot,
    )
}

fn bounded_map_job(workload_kind: AnalyticsWorkloadKind) -> AnalyticsJobDescriptor {
    AnalyticsJobDescriptor::new(
        workload_kind,
        AnalyticsExecutionBounds::new(
            NonZeroU64::new(10_000).unwrap(),
            NonZeroU64::new(16 * 1024 * 1024).unwrap(),
            true,
        ),
        AnalyticsAccelerationPolicy::OptionalAcceleratorWithCpuFallback,
    )
}

fn columnar_layout(consumer: ColumnarConsumer) -> ColumnarLayoutDescriptor {
    ColumnarLayoutDescriptor::new(
        consumer,
        NonZeroU16::new(4).unwrap(),
        ColumnarVersionBinding::new(
            NonZeroU64::new(5).unwrap(),
            NonZeroU64::new(7).unwrap(),
            NonZeroU64::new(11).unwrap(),
            Some(NonZeroU64::new(13).unwrap()),
        ),
        ColumnarAccelerationPolicy::CpuOnly,
    )
}

#[test]
fn map_refresh_plan_consumes_advisory_analytics_and_columnar_contracts() {
    let plan = MapRefreshPlan::new(
        descriptor(),
        bounded_map_job(AnalyticsWorkloadKind::MapRefresh),
        Some(columnar_layout(ColumnarConsumer::Maps)),
    )
    .unwrap();

    assert!(plan.requires_decision_trace());
    assert!(!plan.owns_source_truth());
    assert!(!plan.owns_durable_publication_path());

    let candidate = plan.candidate(23, 29).unwrap();

    assert_eq!(candidate.descriptor, descriptor());
    assert_eq!(candidate.catalog_version, 23);
    assert_eq!(candidate.stats_version, 29);
}

#[test]
fn map_refresh_plan_rejects_non_map_analytics_workloads() {
    let error = MapRefreshPlan::new(
        descriptor(),
        bounded_map_job(AnalyticsWorkloadKind::OperationalDiagnostics),
        Some(columnar_layout(ColumnarConsumer::Maps)),
    )
    .unwrap_err();

    assert_eq!(error, MapDescriptorError::AnalyticsWorkloadNotMapRefresh);
}

#[test]
fn map_refresh_plan_rejects_non_map_columnar_consumers() {
    let error = MapRefreshPlan::new(
        descriptor(),
        bounded_map_job(AnalyticsWorkloadKind::MapRefresh),
        Some(columnar_layout(ColumnarConsumer::Statistics)),
    )
    .unwrap_err();

    assert_eq!(error, MapDescriptorError::ColumnarConsumerNotMaps);
}
