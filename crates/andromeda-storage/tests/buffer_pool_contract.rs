#![forbid(unsafe_code)]

use std::any::TypeId;

use andromeda_core::AndromedaErrorKind;
use andromeda_storage as storage;
use andromeda_storage::layout;

#[test]
fn buffer_pool_exports_are_narrow_and_use_canonical_page_contracts() {
    fn assert_type<T: 'static>() -> TypeId {
        TypeId::of::<T>()
    }

    assert_eq!(
        assert_type::<storage::PageId>(),
        assert_type::<layout::page::PageId>()
    );
    assert_eq!(
        assert_type::<storage::PageSize>(),
        assert_type::<layout::page::PageSize>()
    );
    assert_eq!(
        assert_type::<storage::PageLayoutContract>(),
        assert_type::<layout::page::PageLayoutContract>()
    );
    assert_type::<storage::PageGuard<'static>>();
    assert_type::<storage::PageGuardMut<'static>>();
    assert_type::<storage::ClockEvictionPolicy>();
    assert_type::<storage::ClockEvictionCandidate>();
    assert_type::<storage::DirtyTracker>();
    assert_type::<storage::DirtyEntry>();
    assert_type::<storage::DirtyFlushCandidate>();

    let config = storage::BufferPoolConfig::new(32, storage::PageSize::KiB16)
        .expect("buffer pool config uses canonical page size");
    assert_eq!(config.page_size(), storage::PageSize::KiB16);

    let frame_id = storage::BufferFrameId::new(1).expect("transient frame id");
    let image = storage::PageImage::zeroed_with_layout(valid_contract(
        storage::PageId::new(90),
        config.page_size(),
        storage::Lsn::new(11),
    ))
    .expect("canonical page image");
    let mut frame = storage::BufferFrame::with_image(frame_id, image)
        .expect("buffer frame uses canonical page id and size");
    frame
        .mark_dirty(storage::Lsn::new(11))
        .expect("buffer frame uses canonical lsn");
    assert_eq!(frame.dirty_lsn(), Some(storage::Lsn::new(11)));
    assert_eq!(frame.page_id(), Some(storage::PageId::new(90)));
    assert_eq!(frame.page_lsn(), Some(storage::Lsn::new(11)));
}

#[test]
fn dirty_tracker_contract_is_exported_and_deduplicates_by_page() {
    let mut tracker = storage::DirtyTracker::new();

    tracker
        .mark_dirty(storage::PageId::new(10), storage::Lsn::new(30))
        .expect("valid dirty mark");
    tracker
        .mark_dirty(storage::PageId::new(10), storage::Lsn::new(40))
        .expect("duplicate dirty mark");
    tracker
        .mark_dirty(storage::PageId::new(11), storage::Lsn::new(20))
        .expect("second dirty page");

    assert_eq!(tracker.len(), 2);
    assert_eq!(
        tracker
            .first_dirty_lsn(storage::PageId::new(10))
            .expect("valid lsn query"),
        Some(storage::Lsn::new(30))
    );
    assert_eq!(
        tracker
            .flush_candidates()
            .iter()
            .map(|candidate| (candidate.page_id(), candidate.first_dirty_lsn()))
            .collect::<Vec<_>>(),
        vec![
            (storage::PageId::new(11), storage::Lsn::new(20)),
            (storage::PageId::new(10), storage::Lsn::new(30)),
        ]
    );
}

#[test]
fn page_guard_contract_unpins_and_tracks_dirty_metadata() {
    let image = storage::PageImage::zeroed_with_layout(valid_contract(
        storage::PageId::new(91),
        storage::PageSize::KiB16,
        storage::Lsn::new(12),
    ))
    .expect("canonical page image");
    let mut frame =
        storage::BufferFrame::with_image(storage::BufferFrameId::new(2).expect("frame id"), image)
            .expect("resident frame");

    {
        let guard = frame.pin_guard().expect("immutable guard pin");
        assert_eq!(guard.pin_count(), 1);
        assert_eq!(guard.page_id(), Some(storage::PageId::new(91)));
    }
    assert_eq!(frame.pin_count(), 0);

    {
        let mut guard = frame.pin_guard_mut().expect("mutable guard pin");
        guard
            .mark_dirty(storage::Lsn::new(12))
            .expect("controlled dirty mark");
        assert_eq!(guard.first_dirty_lsn(), Some(storage::Lsn::new(12)));
    }
    assert_eq!(frame.pin_count(), 0);
    assert_eq!(frame.first_dirty_lsn(), Some(storage::Lsn::new(12)));
}

#[test]
fn buffer_pool_rejects_invalid_transient_values_through_crate_root_exports() {
    assert_eq!(
        storage::BufferFrameId::new(0).unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );
    assert_eq!(
        storage::BufferPoolConfig::new(0, storage::PageSize::KiB16)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );
    assert_eq!(
        storage::BufferFrame::new(
            storage::BufferFrameId::new(1).expect("frame id"),
            storage::PageId::new(0),
            storage::PageSize::KiB16,
        )
        .unwrap_err()
        .kind(),
        AndromedaErrorKind::Storage
    );
}

fn valid_contract(
    page_id: storage::PageId,
    page_size: storage::PageSize,
    page_lsn: storage::Lsn,
) -> storage::PageLayoutContract {
    storage::PageLayoutContract {
        header: storage::PageHeader {
            magic: storage::PageHeader::MAGIC,
            format_version: storage::PageHeader::FORMAT_VERSION_V0,
            page_size,
            page_type: storage::PageType::FixedRow,
            page_id,
            object_id: storage::ObjectId::new(2),
            allocation_id: storage::AllocationId::new(3),
            page_lsn,
            page_epoch: 1,
            previous_page_id: None,
            next_page_id: None,
            header_len: storage::PageHeader::MIN_HEADER_LEN_V0,
            payload_offset: 128,
            payload_len: 512,
            free_start: 256,
            free_end: 512,
            free_bytes: 256,
            slot_count: 1,
            row_count: 1,
            flags: storage::PageFlags::NONE,
            header_crc: 5,
        },
        trailer: storage::PageTrailer {
            payload_crc64: 6,
            page_hash: [7; 32],
            torn_write_guard: 8,
        },
    }
}
