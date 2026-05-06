use andromeda_core::AndromedaErrorKind;

use crate::{
    AllocationId, Lsn, ObjectId, PageId, PageSize, SegmentDescriptor, SegmentHeader, SegmentId,
    SegmentState, SegmentTrailer,
};

use super::{
    ColdExtentReclaimEvidence, ExtentDescriptor, ExtentFreeRange, ExtentId, ExtentManager,
    ExtentManagerReplayRecord, ExtentState, validate_segment_extent_contiguity,
};

fn extent() -> ExtentDescriptor {
    ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(3),
        first_page_id: PageId::new(100),
        page_count: 16,
        page_size: PageSize::KiB16,
        state: ExtentState::PublishedCold,
        segment_id: Some(SegmentId::new(4)),
        file_offset: 0,
        allocated_on_disk: false,
    }
}

fn segment() -> SegmentDescriptor {
    let header = SegmentHeader {
        magic: SegmentHeader::MAGIC,
        format_version: SegmentHeader::FORMAT_VERSION_V0,
        segment_id: SegmentId::new(4),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(3),
        first_page_id: PageId::new(100),
        page_count: 16,
        min_page_lsn: Lsn::new(10),
        max_page_lsn: Lsn::new(20),
        header_crc: 30,
    };
    SegmentDescriptor {
        segment_id: header.segment_id,
        object_id: header.object_id,
        allocation_id: header.allocation_id,
        first_extent_id: ExtentId::new(1),
        extent_count: 2,
        first_page_id: header.first_page_id,
        page_count: header.page_count,
        page_size: PageSize::KiB16,
        min_page_lsn: header.min_page_lsn,
        max_page_lsn: header.max_page_lsn,
        snapshot_id: Some(40),
        state: SegmentState::PublishedCold,
        header,
        trailer: SegmentTrailer {
            segment_payload_crc64: 50,
            segment_hash: [60; 32],
            trailer_crc: 70,
        },
    }
}

#[test]
fn extent_contract_validates_page_range() {
    let descriptor = extent();
    assert!(descriptor.validate().is_ok());
    assert_eq!(descriptor.last_page_id().unwrap(), PageId::new(115));
}

#[test]
fn extent_contract_rejects_published_extent_without_segment() {
    let mut descriptor = extent();
    descriptor.segment_id = None;

    assert_eq!(
        descriptor.validate().unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
}

#[test]
fn extent_manager_allocates_from_free_range_and_replays_lifecycle() {
    let free = ExtentFreeRange {
        first_page_id: PageId::new(100),
        page_count: 32,
        page_size: PageSize::KiB16,
        recyclable_after_lsn: None,
    };
    let mut manager = ExtentManager::new(vec![free]).unwrap();
    let mut descriptor = extent();
    descriptor.state = ExtentState::AllocatingHot;
    descriptor.segment_id = None;

    manager
        .apply_replay_record(ExtentManagerReplayRecord::AllocateHot(descriptor))
        .unwrap();
    manager
        .apply_replay_record(ExtentManagerReplayRecord::Seal {
            extent_id: descriptor.extent_id,
        })
        .unwrap();
    manager
        .apply_replay_record(ExtentManagerReplayRecord::PublishCold {
            extent_id: descriptor.extent_id,
            segment_id: SegmentId::new(4),
        })
        .unwrap();

    let published = manager.descriptor(descriptor.extent_id).unwrap();
    assert_eq!(published.state, ExtentState::PublishedCold);
    assert_eq!(published.segment_id, Some(SegmentId::new(4)));
    assert_eq!(manager.free_ranges().len(), 1);
    assert_eq!(manager.free_ranges()[0].first_page_id, PageId::new(116));
}

#[test]
fn extent_manager_rejects_direct_free_after_cold_publication() {
    let mut manager = ExtentManager::from_descriptors(vec![extent()]).unwrap();

    assert_eq!(
        manager.free_sealed_extent(ExtentId::new(1)).unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
    assert!(manager.descriptor(ExtentId::new(1)).is_some());
}

#[test]
fn extent_manager_reclaims_published_cold_only_with_manifest_evidence() {
    let mut manager = ExtentManager::from_descriptors(vec![extent()]).unwrap();

    manager
        .reclaim_published_cold_extent(
            ExtentId::new(1),
            ColdExtentReclaimEvidence {
                retired_manifest_version: 2,
                reclaim_lsn: Lsn::new(30),
                catalog_epoch: 3,
            },
        )
        .unwrap();

    assert!(manager.descriptor(ExtentId::new(1)).is_none());
    assert_eq!(manager.free_ranges()[0].recyclable_after_lsn, Some(Lsn::new(30)));
}

#[test]
fn segment_extent_contiguity_requires_exact_physical_coverage() {
    let segment = segment();
    let mut first = extent();
    first.page_count = 8;
    let mut second = first;
    second.extent_id = ExtentId::new(2);
    second.first_page_id = PageId::new(108);
    second.page_count = 8;

    assert!(validate_segment_extent_contiguity(&segment, &[first, second]).is_ok());

    second.first_page_id = PageId::new(109);
    assert_eq!(
        validate_segment_extent_contiguity(&segment, &[first, second])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
}
