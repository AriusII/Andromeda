use std::num::{NonZeroU16, NonZeroU64};

use andromeda_analytics::{
    AnalyticsAccelerationPolicy, AnalyticsExecutionBounds, AnalyticsJobDescriptor,
    AnalyticsWorkloadKind,
};
use andromeda_columnar::{
    ColumnChunkDescriptor, ColumnChunkPruningMetadata, ColumnarAccelerationPolicy,
    ColumnarConsumer, ColumnarLayoutDescriptor, ColumnarSegmentDescriptor, ColumnarSnapshotBinding,
    ColumnarVersionBinding,
};
use andromeda_maps::{
    MapConsistencyPolicy, MapDeltaLog, MapDescriptor, MapDescriptorError, MapGrain, MapId,
    MapRefreshAdmissionRequest, MapRefreshMode, MapRefreshPlan, MapStalenessPolicy,
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

fn cpu_first_map_job(workload_kind: AnalyticsWorkloadKind) -> AnalyticsJobDescriptor {
    AnalyticsJobDescriptor::new(
        workload_kind,
        AnalyticsExecutionBounds::new(
            NonZeroU64::new(10_000).unwrap(),
            NonZeroU64::new(16 * 1024 * 1024).unwrap(),
            true,
        ),
        AnalyticsAccelerationPolicy::CpuOnly,
    )
}

fn columnar_segment(consumer: ColumnarConsumer) -> ColumnarSegmentDescriptor {
    let layout = ColumnarLayoutDescriptor::new(
        consumer,
        NonZeroU16::new(4).unwrap(),
        ColumnarVersionBinding::new(
            NonZeroU64::new(5).unwrap(),
            NonZeroU64::new(7).unwrap(),
            NonZeroU64::new(11).unwrap(),
            Some(NonZeroU64::new(13).unwrap()),
        ),
        ColumnarAccelerationPolicy::CpuOnly,
    );

    ColumnarSegmentDescriptor::new(
        layout,
        ColumnarSnapshotBinding::new(NonZeroU64::new(97).unwrap()),
        vec![
            ColumnChunkDescriptor::new(
                0,
                NonZeroU64::new(512).unwrap(),
                ColumnChunkPruningMetadata::new(true, true, false),
            ),
            ColumnChunkDescriptor::new(
                1,
                NonZeroU64::new(512).unwrap(),
                ColumnChunkPruningMetadata::new(false, false, true),
            ),
        ],
    )
}

fn policy(descriptor: MapDescriptor) -> MapConsistencyPolicy {
    MapConsistencyPolicy::new(
        descriptor,
        NonZeroU64::new(64).unwrap(),
        NonZeroU64::new(8).unwrap(),
    )
}

#[test]
fn map_refresh_plan_consumes_advisory_analytics_and_columnar_contracts() {
    let plan = MapRefreshPlan::new(
        descriptor(),
        bounded_map_job(AnalyticsWorkloadKind::MapRefresh),
        Some(columnar_segment(ColumnarConsumer::Maps)),
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
        Some(columnar_segment(ColumnarConsumer::Maps)),
    )
    .unwrap_err();

    assert_eq!(error, MapDescriptorError::AnalyticsWorkloadNotMapRefresh);
}

#[test]
fn map_refresh_plan_rejects_non_map_columnar_consumers() {
    let error = MapRefreshPlan::new(
        descriptor(),
        bounded_map_job(AnalyticsWorkloadKind::MapRefresh),
        Some(columnar_segment(ColumnarConsumer::Statistics)),
    )
    .unwrap_err();

    assert_eq!(error, MapDescriptorError::ColumnarConsumerNotMaps);
}

#[test]
fn immediate_map_refresh_requires_bounded_cost_and_same_transaction_wal_records() {
    let descriptor = MapDescriptor::new(
        MapId::new(23).unwrap(),
        MapGrain::Relation,
        MapRefreshMode::Immediate,
        MapStalenessPolicy::CurrentOnly,
    );
    let plan = MapRefreshPlan::new(
        descriptor,
        cpu_first_map_job(AnalyticsWorkloadKind::MapRefresh),
        Some(columnar_segment(ColumnarConsumer::Maps)),
    )
    .unwrap();

    let decision = policy(descriptor)
        .admit(
            &plan,
            MapRefreshAdmissionRequest::new(12, 0, 3)
                .with_table_wal_records(2)
                .with_map_wal_records(1)
                .with_decision_trace(),
            23,
            29,
        )
        .unwrap();

    assert_eq!(decision.candidate.descriptor, descriptor);
    assert_eq!(decision.refresh_cost, 12);
    assert_eq!(decision.wal_records_reserved, 3);
    assert_eq!(
        decision.source_snapshot_lsn,
        Some(NonZeroU64::new(97).unwrap())
    );
    assert!(decision.preserves_wal_priority());
}

#[test]
fn incremental_map_refresh_requires_controlled_delta_log() {
    let descriptor = MapDescriptor::new(
        MapId::new(31).unwrap(),
        MapGrain::Partition,
        MapRefreshMode::Incremental,
        MapStalenessPolicy::bounded_millis(250).unwrap(),
    );
    let plan = MapRefreshPlan::new(
        descriptor,
        cpu_first_map_job(AnalyticsWorkloadKind::MapRefresh),
        Some(columnar_segment(ColumnarConsumer::Maps)),
    )
    .unwrap();
    let delta_log = MapDeltaLog::new(
        descriptor,
        NonZeroU64::new(90).unwrap(),
        NonZeroU64::new(96).unwrap(),
        NonZeroU64::new(4).unwrap(),
        NonZeroU64::new(2).unwrap(),
    );

    let decision = policy(descriptor)
        .admit(
            &plan,
            MapRefreshAdmissionRequest::new(18, 200, 4)
                .with_decision_trace()
                .with_delta_log(delta_log),
            41,
            43,
        )
        .unwrap();

    assert!(decision.uses_incremental_delta());
    assert_eq!(decision.delta_log, Some(delta_log));
    assert_eq!(decision.wal_records_reserved, 6);
}

#[test]
fn snapshot_only_map_refresh_requires_published_snapshot_binding() {
    let descriptor = MapDescriptor::new(
        MapId::new(37).unwrap(),
        MapGrain::Segment,
        MapRefreshMode::SnapshotOnly,
        MapStalenessPolicy::AdvisorySnapshot,
    );
    let plan = MapRefreshPlan::new(
        descriptor,
        cpu_first_map_job(AnalyticsWorkloadKind::MapRefresh),
        Some(columnar_segment(ColumnarConsumer::Maps)),
    )
    .unwrap();

    let decision = policy(descriptor)
        .admit(
            &plan,
            MapRefreshAdmissionRequest::new(9, 0, 1)
                .with_decision_trace()
                .with_published_snapshot_lsn(NonZeroU64::new(97).unwrap()),
            51,
            53,
        )
        .unwrap();

    assert_eq!(
        decision.source_snapshot_lsn,
        Some(NonZeroU64::new(97).unwrap())
    );
    assert_eq!(decision.wal_records_reserved, 0);
}

#[test]
fn map_refresh_admission_rejects_wal_priority_starvation() {
    let descriptor = MapDescriptor::new(
        MapId::new(41).unwrap(),
        MapGrain::Relation,
        MapRefreshMode::Deferred,
        MapStalenessPolicy::bounded_millis(500).unwrap(),
    );
    let plan = MapRefreshPlan::new(
        descriptor,
        cpu_first_map_job(AnalyticsWorkloadKind::MapRefresh),
        Some(columnar_segment(ColumnarConsumer::Maps)),
    )
    .unwrap();

    let error = policy(descriptor)
        .admit(
            &plan,
            MapRefreshAdmissionRequest::new(10, 200, 9).with_decision_trace(),
            61,
            67,
        )
        .unwrap_err();

    assert_eq!(error, MapDescriptorError::WalPriorityExceeded);
}

#[test]
fn map_refresh_admission_rejects_stale_refresh_candidates() {
    let descriptor = MapDescriptor::new(
        MapId::new(43).unwrap(),
        MapGrain::KeyRange,
        MapRefreshMode::Deferred,
        MapStalenessPolicy::bounded_millis(100).unwrap(),
    );
    let plan = MapRefreshPlan::new(
        descriptor,
        cpu_first_map_job(AnalyticsWorkloadKind::MapRefresh),
        Some(columnar_segment(ColumnarConsumer::Maps)),
    )
    .unwrap();

    let error = policy(descriptor)
        .admit(
            &plan,
            MapRefreshAdmissionRequest::new(10, 101, 1).with_decision_trace(),
            71,
            73,
        )
        .unwrap_err();

    assert_eq!(error, MapDescriptorError::StalenessExceeded);
}
