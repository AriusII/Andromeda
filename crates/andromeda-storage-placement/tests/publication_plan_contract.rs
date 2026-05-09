use andromeda_hardware::{HardwareProfile, PipelineClass};
use andromeda_segment::{
    ExtentId, SegmentDescriptor, SegmentHeader, SegmentId, SegmentState, SegmentTrailer,
};
use andromeda_storage_page::{AllocationId, Lsn, ObjectId, PageId, PageSize};
use andromeda_storage_placement::{
    ColdSegmentPublicationPlan, CoreIoPlacementDecision, CoreIoPlacementPolicy,
    HotColdIoThresholds, IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget,
    IoUseClass, PipelineStage, StorageTier, StorageWorkloadClass,
    validate_cold_segment_publication_boundary,
};

fn sealed_segment_descriptor(snapshot_id: u64) -> SegmentDescriptor {
    segment_descriptor(SegmentState::Sealed, snapshot_id, 4_096)
}

fn segment_descriptor(state: SegmentState, snapshot_id: u64, page_count: u32) -> SegmentDescriptor {
    let header = SegmentHeader {
        magic: SegmentHeader::MAGIC,
        format_version: SegmentHeader::FORMAT_VERSION_V0,
        segment_id: SegmentId::new(50),
        object_id: ObjectId::new(51),
        allocation_id: AllocationId::new(52),
        first_page_id: PageId::new(1_000),
        page_count,
        min_page_lsn: Lsn::new(20),
        max_page_lsn: Lsn::new(30),
        header_crc: 53,
    };

    SegmentDescriptor {
        segment_id: header.segment_id,
        object_id: header.object_id,
        allocation_id: header.allocation_id,
        first_extent_id: ExtentId::new(54),
        extent_count: 1,
        first_page_id: header.first_page_id,
        page_count: header.page_count,
        page_size: PageSize::KiB16,
        min_page_lsn: header.min_page_lsn,
        max_page_lsn: header.max_page_lsn,
        snapshot_id: Some(snapshot_id),
        state,
        header,
        trailer: SegmentTrailer {
            segment_payload_crc64: 55,
            segment_hash: [56; 32],
            trailer_crc: 57,
        },
    }
}

fn cold_budget() -> IoPathBudget {
    IoPathBudget::new(
        IoPathClass::ColdPathHdd,
        IoLatencyBudget::new(5_000_000, 5_000_000, 5_000_000),
        IoThroughputBudget::new(128 * 1024 * 1024, 128 * 1024 * 1024),
    )
}

#[test]
fn cold_segment_publication_plan_requires_cold_publication_policy() {
    let policy = CoreIoPlacementPolicy::new(
        HardwareProfile::conservative(),
        HotColdIoThresholds::conservative(),
    );
    let sealed = sealed_segment_descriptor(7);

    let plan = ColdSegmentPublicationPlan::new(sealed, &policy, cold_budget()).unwrap();

    assert!(plan.validate().is_ok());
    assert_eq!(
        plan.decision.workload,
        StorageWorkloadClass::ColdPublication
    );
    assert_eq!(
        plan.decision.placement.pipeline_stage,
        PipelineStage::PublishColdStore
    );
    assert_eq!(plan.decision.placement.target_tier, StorageTier::ColdStore);
    assert_eq!(plan.decision.io_use_class, IoUseClass::ColdSegmentPath);
    assert!(!plan.decision.placement.mutation_allowed);
    assert!(!plan.decision.gpu_enabled);
}

#[test]
fn cold_segment_publication_plan_does_not_publish_or_mutate_descriptor() {
    let policy = CoreIoPlacementPolicy::new(
        HardwareProfile::conservative(),
        HotColdIoThresholds::conservative(),
    );
    let sealed = sealed_segment_descriptor(7);

    let plan = ColdSegmentPublicationPlan::new(sealed, &policy, cold_budget()).unwrap();

    assert_eq!(plan.sealed_segment.state, SegmentState::Sealed);
    assert_eq!(plan.sealed_segment.snapshot_id, Some(7));
    assert_eq!(
        plan.decision.placement.pipeline_stage,
        PipelineStage::PublishColdStore
    );
    assert_eq!(plan.decision.placement.target_tier, StorageTier::ColdStore);
    assert!(!plan.decision.placement.mutation_allowed);
}

#[test]
fn cold_segment_publication_rejects_mutable_and_commit_critical_boundaries() {
    let policy = CoreIoPlacementPolicy::new(
        HardwareProfile::conservative(),
        HotColdIoThresholds::conservative(),
    );
    let mutable = segment_descriptor(SegmentState::BuildingHotSnapshot, 7, 4_096);

    assert!(ColdSegmentPublicationPlan::new(mutable, &policy, cold_budget()).is_err());

    let sealed = sealed_segment_descriptor(7);
    let valid = ColdSegmentPublicationPlan::new(sealed, &policy, cold_budget()).unwrap();

    let mut mutable_decision = valid.decision;
    mutable_decision.placement.mutation_allowed = true;
    assert!(validate_cold_segment_publication_boundary(&sealed, &mutable_decision).is_err());

    let commit_critical_cold_publication = CoreIoPlacementDecision {
        workload: StorageWorkloadClass::Commit,
        placement: valid.decision.placement,
        pipeline_class: PipelineClass::Commit,
        io_use_class: IoUseClass::CommitCriticalHotPath,
        path_budget: cold_budget(),
        gpu_enabled: false,
    };
    assert!(
        validate_cold_segment_publication_boundary(&sealed, &commit_critical_cold_publication)
            .is_err()
    );
}
