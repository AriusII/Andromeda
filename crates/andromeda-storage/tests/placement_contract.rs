use andromeda_error::AndromedaErrorKind;
use andromeda_hardware::{
    GpuExecutionPolicy, GpuProfile, HardwareProfile, RamProfile, RamSectionBudget, RamSectionRole,
};
use andromeda_observe::{
    CriticalDecisionKind, EventCorrelation, EventEnvelope, EventId, PlacementAuditTransition,
    TraceEvent, TraceId,
};
use andromeda_storage::{
    AllocationId, CoreIoPlacementPolicy, CoreIoPlacementRequest, DataTemperature, ExtentId,
    HotColdIoThresholds, IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget,
    IoUseClass, Lsn, ObjectId, PageId, PageSize, PipelineStage, PlacementDecision,
    SegmentDescriptor, SegmentHeader, SegmentId, SegmentMutation, SegmentState, SegmentTrailer,
    StorageIoBudgetScope, StorageTier, StorageWorkloadClass,
};

fn descriptor(state: SegmentState) -> SegmentDescriptor {
    let header = SegmentHeader {
        magic: SegmentHeader::MAGIC,
        format_version: SegmentHeader::FORMAT_VERSION_V0,
        segment_id: SegmentId::new(90),
        object_id: ObjectId::new(91),
        allocation_id: AllocationId::new(92),
        first_page_id: PageId::new(1_000),
        page_count: 8,
        min_page_lsn: Lsn::new(93),
        max_page_lsn: Lsn::new(94),
        header_crc: 95,
    };

    SegmentDescriptor {
        segment_id: header.segment_id,
        object_id: header.object_id,
        allocation_id: header.allocation_id,
        first_extent_id: ExtentId::new(96),
        extent_count: 1,
        first_page_id: header.first_page_id,
        page_count: header.page_count,
        page_size: PageSize::KiB16,
        min_page_lsn: header.min_page_lsn,
        max_page_lsn: header.max_page_lsn,
        snapshot_id: match state {
            SegmentState::BuildingHotSnapshot => None,
            SegmentState::Sealed | SegmentState::PublishedCold => Some(97),
        },
        state,
        header,
        trailer: SegmentTrailer {
            segment_payload_crc64: 98,
            segment_hash: [99; 32],
            trailer_crc: 100,
        },
    }
}

fn hot_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::HotPathNvmeSsd,
        IoLatencyBudget::new(1_000, 1_000, 2_000),
        IoThroughputBudget::new(128 * 1024 * 1024, 128 * 1024 * 1024),
    )
}

fn cold_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::ColdPathHdd,
        IoLatencyBudget::new(5_000_000, 5_000_000, 5_000_000),
        IoThroughputBudget::new(128 * 1024 * 1024, 128 * 1024 * 1024),
    )
}

fn ram_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::CpuRam,
        IoLatencyBudget::new(1_000, 1_000, 1_000),
        IoThroughputBudget::new(128 * 1024 * 1024, 128 * 1024 * 1024),
    )
}

fn hardware_with_gpu() -> HardwareProfile {
    HardwareProfile {
        ram: RamProfile::new(
            256 * 1024 * 1024,
            vec![
                RamSectionBudget::new(RamSectionRole::Cache, 64 * 1024 * 1024),
                RamSectionBudget::new(RamSectionRole::Io, 64 * 1024 * 1024),
                RamSectionBudget::new(RamSectionRole::Execution, 64 * 1024 * 1024),
            ],
        ),
        gpu: GpuProfile {
            available: true,
            execution_policy: GpuExecutionPolicy::OffCriticalPathOnly,
        },
        ..HardwareProfile::conservative()
    }
}

#[test]
fn append_uses_hotstore_and_rejects_cold_temperature() {
    let decision = PlacementDecision::append(DataTemperature::Hot).unwrap();

    assert_eq!(decision.target_tier, StorageTier::HotStore);
    assert_eq!(decision.pipeline_stage, PipelineStage::AppendHotStore);
    assert!(decision.mutation_allowed);
    assert!(decision.validate().is_ok());

    assert_eq!(
        PlacementDecision::append(DataTemperature::Cold)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn reads_follow_temperature_fallback_rules() {
    let ram = PlacementDecision::read(DataTemperature::RamWorkingSet);
    assert_eq!(ram.target_tier, StorageTier::Ram);
    assert!(ram.read_fallback.allows_tier(StorageTier::ColdStore));
    assert!(ram.validate().is_ok());

    let cold = PlacementDecision::read(DataTemperature::Cold);
    assert_eq!(cold.target_tier, StorageTier::ColdStore);
    assert!(cold.read_fallback.allows_tier(StorageTier::ColdStore));
    assert!(!cold.read_fallback.allows_tier(StorageTier::HotStore));
    assert!(cold.validate().is_ok());
}

#[test]
fn hot_to_cold_pipeline_requires_sealed_snapshot_segment() {
    let building = descriptor(SegmentState::BuildingHotSnapshot);
    let seal = PlacementDecision::seal_hot_segment(&building).unwrap();
    assert_eq!(seal.pipeline_stage, PipelineStage::SealHotStoreSegment);
    assert_eq!(seal.target_tier, StorageTier::HotStore);
    assert!(seal.validate().is_ok());

    let sealed = descriptor(SegmentState::Sealed);
    let publish = PlacementDecision::publish_cold_segment(&sealed).unwrap();
    assert_eq!(publish.pipeline_stage, PipelineStage::PublishColdStore);
    assert_eq!(publish.target_tier, StorageTier::ColdStore);
    assert!(!publish.mutation_allowed);
    assert!(publish.validate().is_ok());

    let published = descriptor(SegmentState::PublishedCold);
    assert_eq!(
        PlacementDecision::mutation(
            &published,
            SegmentMutation::AppendExtent,
            DataTemperature::Hot,
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn placement_decisions_do_not_mutate_segment_state_or_durable_truth() {
    let sealed = descriptor(SegmentState::Sealed);
    let publication = PlacementDecision::publish_cold_segment(&sealed).unwrap();

    assert_eq!(sealed.state, SegmentState::Sealed);
    assert_eq!(publication.target_tier, StorageTier::ColdStore);
    assert_eq!(publication.pipeline_stage, PipelineStage::PublishColdStore);
    assert!(!publication.mutation_allowed);

    let building = descriptor(SegmentState::BuildingHotSnapshot);
    let seal = PlacementDecision::seal_hot_segment(&building).unwrap();

    assert_eq!(building.state, SegmentState::BuildingHotSnapshot);
    assert_eq!(seal.target_tier, StorageTier::HotStore);
    assert_eq!(seal.pipeline_stage, PipelineStage::SealHotStoreSegment);
}

#[test]
fn core_io_policy_maps_ram_hot_and_cold_workloads_to_storage_tiers() {
    let policy =
        CoreIoPlacementPolicy::new(hardware_with_gpu(), HotColdIoThresholds::conservative());

    let ram = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::RamWorkingSet,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            ram_budget(),
            false,
        ))
        .unwrap();
    assert_eq!(ram.placement.target_tier, StorageTier::Ram);
    assert_eq!(ram.path_budget.path_class, IoPathClass::CpuRam);

    let hot = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::HotAppend,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            hot_budget(),
            false,
        ))
        .unwrap();
    assert_eq!(hot.placement.target_tier, StorageTier::HotStore);
    assert_eq!(hot.io_use_class, IoUseClass::OnlineHotPath);

    let cold_read = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::ColdRead,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            cold_budget(),
            false,
        ))
        .unwrap();
    assert_eq!(cold_read.placement.target_tier, StorageTier::ColdStore);
    assert_eq!(cold_read.io_use_class, IoUseClass::ColdSegmentPath);
    assert!(!cold_read.placement.mutation_allowed);
}

#[test]
fn core_io_policy_rejects_gpu_for_commit_wal_rollback_and_recovery() {
    let policy =
        CoreIoPlacementPolicy::new(hardware_with_gpu(), HotColdIoThresholds::conservative());

    for workload in [
        StorageWorkloadClass::Commit,
        StorageWorkloadClass::WalAppend,
        StorageWorkloadClass::Rollback,
        StorageWorkloadClass::Recovery,
    ] {
        let error = policy
            .plan(CoreIoPlacementRequest::new(
                workload,
                StorageIoBudgetScope::Page(PageSize::KiB16),
                hot_budget(),
                true,
            ))
            .unwrap_err();
        assert_eq!(error.kind(), AndromedaErrorKind::Resource);
    }
}

#[test]
fn core_io_policy_rejects_cold_hdd_for_commit_critical_hot_path() {
    let policy =
        CoreIoPlacementPolicy::new(hardware_with_gpu(), HotColdIoThresholds::conservative());

    let error = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::Commit,
            StorageIoBudgetScope::Page(PageSize::KiB16),
            cold_budget(),
            false,
        ))
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
}

#[test]
fn core_io_policy_validates_ram_and_segment_io_budgets() {
    let policy =
        CoreIoPlacementPolicy::new(hardware_with_gpu(), HotColdIoThresholds::conservative());

    let over_ram = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::RamWorkingSet,
            StorageIoBudgetScope::Segment {
                bytes: 128 * 1024 * 1024,
            },
            ram_budget(),
            false,
        ))
        .unwrap_err();
    assert_eq!(over_ram.kind(), AndromedaErrorKind::Resource);

    let cold_publication = policy
        .plan(CoreIoPlacementRequest::new(
            StorageWorkloadClass::ColdPublication,
            StorageIoBudgetScope::Segment {
                bytes: 128 * 1024 * 1024,
            },
            cold_budget(),
            false,
        ))
        .unwrap();
    assert_eq!(
        cold_publication.placement.pipeline_stage,
        PipelineStage::PublishColdStore
    );
    assert_eq!(
        cold_publication.placement.target_tier,
        StorageTier::ColdStore
    );
}

#[test]
fn placement_fsm_emits_all_five_transition_audit_events() {
    let placement = PlacementDecision::append(DataTemperature::Hot).unwrap();
    let placement_audit = placement
        .audit_placement_decision(TraceId::new(801))
        .unwrap();
    assert_eq!(
        placement_audit.transition,
        PlacementAuditTransition::PlacementDecisionMade
    );
    assert!(placement_audit.accepted);

    let building = descriptor(SegmentState::BuildingHotSnapshot);
    let (_, sealed_audit) =
        PlacementDecision::seal_hot_segment_with_audit(&building, TraceId::new(802)).unwrap();
    assert_eq!(
        sealed_audit.transition,
        PlacementAuditTransition::SegmentSealed
    );
    assert_eq!(sealed_audit.segment_id, Some(building.segment_id.get()));

    let sealed = descriptor(SegmentState::Sealed);
    let (_, published_audit) =
        PlacementDecision::publish_cold_segment_with_audit(&sealed, TraceId::new(803)).unwrap();
    assert_eq!(
        published_audit.transition,
        PlacementAuditTransition::SegmentPublishedCold
    );
    assert_eq!(published_audit.segment_id, Some(sealed.segment_id.get()));

    let reclaimed =
        PlacementDecision::extent_reclaimed_audit(TraceId::new(804), 96, "extent reclaimed")
            .unwrap();
    assert_eq!(
        reclaimed.transition,
        PlacementAuditTransition::ExtentReclaimed
    );
    assert_eq!(reclaimed.extent_id, Some(96));

    let published = descriptor(SegmentState::PublishedCold);
    assert!(
        PlacementDecision::mutation(
            &published,
            SegmentMutation::AppendExtent,
            DataTemperature::Hot
        )
        .is_err()
    );
    let rejected = PlacementDecision::cold_mutation_rejected_audit(
        TraceId::new(805),
        &published,
        SegmentMutation::AppendExtent,
    )
    .unwrap();
    assert_eq!(
        rejected.transition,
        PlacementAuditTransition::ColdMutationRejected
    );
    assert!(!rejected.accepted);
    assert_eq!(rejected.segment_id, Some(published.segment_id.get()));

    let envelope = EventEnvelope::new(
        EventId::new(806),
        EventCorrelation::empty(),
        TraceEvent::PlacementAudit(rejected),
    )
    .expect("placement audit events must be envelope-valid and roundtrippable");
    assert_eq!(envelope.event.kind(), CriticalDecisionKind::PlacementAudit);
}
