use andromeda_core::PipelineClass;
use andromeda_storage::layout::cold::PublishedColdSegment;
use andromeda_storage::layout::extent::ExtentId;
use andromeda_storage::layout::io_budget::{
    HotColdIoThresholds, IoLatencyBudget, IoPathBudget, IoPathClass, IoThroughputBudget, IoUseClass,
};
use andromeda_storage::layout::page::{
    AllocationId, ObjectId, PageFlags, PageHeader, PageId, PageLayoutContract, PageSize,
    PageTrailer, PageType,
};
use andromeda_storage::layout::placement::{
    CoreIoPlacementDecision, CoreIoPlacementPolicy, PipelineStage, StorageTier,
    StorageWorkloadClass,
};
use andromeda_storage::layout::segment::{
    SegmentDescriptor, SegmentHeader, SegmentId, SegmentMutation, SegmentState, SegmentTrailer,
};
use andromeda_storage::publication::{
    validate_cold_segment_publication_boundary, ColdSegmentPublicationPlan, DatabaseManifest,
    DatabaseSnapshotPublication, SnapshotAvailabilityContract, SnapshotSegmentReference,
};
use andromeda_storage::Lsn;

fn page_header() -> PageHeader {
    PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size: PageSize::KiB16,
        page_type: PageType::FixedRow,
        page_id: PageId::new(100),
        object_id: ObjectId::new(200),
        allocation_id: AllocationId::new(300),
        page_lsn: Lsn::new(10),
        page_epoch: 1,
        previous_page_id: None,
        next_page_id: Some(PageId::new(101)),
        header_len: PageHeader::MIN_HEADER_LEN_V0,
        payload_offset: 128,
        payload_len: 512,
        free_start: 256,
        free_end: 512,
        free_bytes: 256,
        slot_count: 4,
        row_count: 3,
        flags: PageFlags::HAS_NEXT,
        header_crc: 11,
    }
}

fn page_trailer() -> PageTrailer {
    PageTrailer {
        payload_crc64: 12,
        page_hash: [13; 32],
        torn_write_guard: 14,
    }
}

fn manifest(snapshot_id: u64) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id,
        base_checkpoint_lsn: Lsn::new(10),
        required_wal_start_lsn: Lsn::new(10),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    }
}

fn cold_segment_descriptor(snapshot_id: u64) -> SegmentDescriptor {
    segment_descriptor(SegmentState::PublishedCold, snapshot_id, 8)
}

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
fn page_layout_facade_validates_offsets_flags_and_trailer() {
    let contract = PageLayoutContract {
        header: page_header(),
        trailer: page_trailer(),
    };
    assert!(contract.validate().is_ok());

    let mut bad_flags = page_header();
    bad_flags.flags = PageFlags::NONE;
    assert!(bad_flags.validate().is_err());

    let mut bad_offsets = page_header();
    bad_offsets.free_start = bad_offsets.free_end + 1;
    assert!(bad_offsets.validate().is_err());
}

#[test]
fn cold_segments_are_immutable_after_publication() {
    let descriptor = cold_segment_descriptor(7);
    let published = PublishedColdSegment::new(descriptor).unwrap();

    assert!(published.validate_immutable().is_ok());
    assert!(published
        .reject_mutation(SegmentMutation::AppendExtent)
        .is_err());
    assert!(descriptor
        .validate_mutation(SegmentMutation::UpdatePageInPlace)
        .is_err());
}

#[test]
fn snapshot_publication_keeps_available_manifest_references() {
    let publication = DatabaseSnapshotPublication {
        manifest: manifest(7),
        snapshot_id: 7,
        publication_epoch: 1,
        snapshot_descriptor_hash: [2; 32],
        segments: vec![SnapshotSegmentReference {
            segment_id: SegmentId::new(50),
            descriptor_hash: [3; 32],
        }],
    };
    assert!(publication.validate().is_ok());

    let availability = SnapshotAvailabilityContract {
        published_snapshot_id: 7,
        available_snapshot_ids: vec![6, 7],
    };
    assert!(availability.validate().is_ok());

    let mut duplicate_segments = publication.clone();
    duplicate_segments
        .segments
        .push(duplicate_segments.segments[0]);
    assert!(duplicate_segments.validate().is_err());

    let unavailable = SnapshotAvailabilityContract {
        published_snapshot_id: 7,
        available_snapshot_ids: vec![6],
    };
    assert!(unavailable.validate().is_err());
}

#[test]
fn cold_segment_publication_plan_requires_cold_publication_policy() {
    let policy = CoreIoPlacementPolicy::new(
        andromeda_core::HardwareProfile::conservative(),
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
fn cold_segment_publication_rejects_mutable_and_commit_critical_boundaries() {
    let policy = CoreIoPlacementPolicy::new(
        andromeda_core::HardwareProfile::conservative(),
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
