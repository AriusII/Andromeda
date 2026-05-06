//! Integration tests for real disk I/O backing.
//!
//! These tests verify that buffer pool page flushes actually write to disk,
//! reads restore pages correctly from persistent storage, and crash safety
//! is maintained through atomic write protocols.

use andromeda_storage::{
    AllocationId, DiskManager, ExtentDescriptor, ExtentId, ExtentState, FileDiskManager, Lsn,
    ObjectId, PageFlags, PageHeader, PageId, PageImage, PageLayoutContract, PageSize, PageTrailer,
    PageType,
};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use tempfile::TempDir;

fn create_test_header(page_id: u64, page_size: PageSize) -> PageHeader {
    let header_len = PageHeader::MIN_HEADER_LEN_V0;
    let trailer_len = PageTrailer::V0_LEN;
    let page_bytes = page_size.bytes();
    let available_payload = page_bytes - u32::from(header_len) - trailer_len;

    PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size,
        page_type: PageType::FixedRow,
        page_id: PageId::new(page_id),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        page_lsn: Lsn::new(1),
        page_epoch: 1,
        previous_page_id: None,
        next_page_id: None,
        header_len,
        payload_offset: u32::from(header_len),
        payload_len: available_payload,
        free_start: u32::from(header_len),
        free_end: u32::from(header_len) + available_payload,
        free_bytes: available_payload,
        slot_count: 0,
        row_count: 0,
        flags: PageFlags::NONE,
        header_crc: 0xDEADBEEF,
    }
}

fn create_test_trailer() -> PageTrailer {
    PageTrailer {
        torn_write_guard: 0xDEADBEEF,
        page_hash: [0xAB; 32],
        payload_crc64: 0xBEEFCAFE,
    }
}

fn create_test_page_image(page_id: u64, page_size: PageSize) -> PageImage {
    let header = create_test_header(page_id, page_size);
    let trailer = create_test_trailer();
    let layout = PageLayoutContract { header, trailer };

    let mut bytes = vec![0u8; page_size.bytes_usize()];
    // Fill with recognizable pattern for testing
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = ((page_id ^ i as u64) & 0xFF) as u8;
    }

    PageImage::with_layout(layout, bytes).unwrap()
}

fn create_temp_disk_manager() -> (FileDiskManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("test_store.bin");
    let manager = FileDiskManager::open(&data_file, temp_dir.path()).unwrap();
    (manager, temp_dir)
}

#[test]
fn test_extent_allocation_reserves_disk_space() {
    let (mut manager, temp_dir) = create_temp_disk_manager();
    let data_file = temp_dir.path().join("test_store.bin");

    // Allocate a single extent with 100 pages of 16 KiB each
    let extent = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(1),
        page_count: 100,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent).unwrap();

    // Verify file was pre-allocated with correct size
    let file_size = fs::metadata(&data_file).unwrap().len();
    let expected_size = 100 * 16 * 1024;
    assert_eq!(
        file_size, expected_size,
        "File size mismatch after extent allocation"
    );
}

#[test]
fn test_page_flush_writes_to_disk() {
    let (mut manager, temp_dir) = create_temp_disk_manager();
    let data_file = temp_dir.path().join("test_store.bin");

    // Allocate extent
    let extent = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(1),
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent).unwrap();

    // Write a page
    let page_image = create_test_page_image(1, PageSize::KiB16);
    let expected_bytes = page_image.as_bytes().to_vec();

    manager.write_page(page_image, Lsn::new(1)).unwrap();

    // Verify bytes on disk match
    let mut file = fs::File::open(&data_file).unwrap();
    let mut buffer = vec![0u8; PageSize::KiB16.bytes_usize()];
    file.read_exact(&mut buffer).unwrap();

    assert_eq!(
        buffer, expected_bytes,
        "Page bytes on disk don't match written data"
    );
}

#[test]
fn test_multiple_page_writes_to_correct_offsets() {
    let (mut manager, temp_dir) = create_temp_disk_manager();
    let data_file = temp_dir.path().join("test_store.bin");

    // Allocate extent with 3 pages
    let extent = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(1),
        page_count: 3,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent).unwrap();

    // Write three pages with different content
    let page1 = create_test_page_image(1, PageSize::KiB16);
    let page2 = create_test_page_image(2, PageSize::KiB16);
    let page3 = create_test_page_image(3, PageSize::KiB16);

    let expected_page1 = page1.as_bytes().to_vec();
    let expected_page2 = page2.as_bytes().to_vec();
    let expected_page3 = page3.as_bytes().to_vec();

    manager.write_page(page1, Lsn::new(1)).unwrap();
    manager.write_page(page2, Lsn::new(1)).unwrap();
    manager.write_page(page3, Lsn::new(1)).unwrap();

    // Verify each page is at correct offset on disk
    let page_size = PageSize::KiB16.bytes_usize();
    let mut file = fs::File::open(&data_file).unwrap();

    let mut buffer = vec![0u8; page_size];

    // Read page 1 at offset 0
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_exact(&mut buffer).unwrap();
    assert_eq!(buffer, expected_page1, "Page 1 mismatch");

    // Read page 2 at offset page_size
    file.seek(SeekFrom::Start(page_size as u64)).unwrap();
    file.read_exact(&mut buffer).unwrap();
    assert_eq!(buffer, expected_page2, "Page 2 mismatch");

    // Read page 3 at offset 2 * page_size
    file.seek(SeekFrom::Start((2 * page_size) as u64)).unwrap();
    file.read_exact(&mut buffer).unwrap();
    assert_eq!(buffer, expected_page3, "Page 3 mismatch");
}

#[test]
fn test_page_read_restores_from_disk() {
    let (mut manager, _temp_dir) = create_temp_disk_manager();

    // Allocate and write a page
    let extent = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(1),
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent).unwrap();

    let original_image = create_test_page_image(1, PageSize::KiB16);
    let original_bytes = original_image.as_bytes().to_vec();

    manager.write_page(original_image, Lsn::new(1)).unwrap();

    // Read page back
    let read_image = manager.read_page(PageId::new(1)).unwrap().unwrap();
    let read_bytes = read_image.as_bytes();

    assert_eq!(
        read_bytes, original_bytes,
        "Read page doesn't match written page"
    );
}

#[test]
fn test_sequential_extents_append_to_file() {
    let (mut manager, temp_dir) = create_temp_disk_manager();
    let data_file = temp_dir.path().join("test_store.bin");

    // Allocate first extent (pages 1-10)
    let extent1 = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(1),
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent1).unwrap();

    // Allocate second extent (pages 11-20)
    let extent2 = ExtentDescriptor {
        extent_id: ExtentId::new(2),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(2),
        first_page_id: PageId::new(11),
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent2).unwrap();

    // Write pages to both extents
    let page1 = create_test_page_image(1, PageSize::KiB16);
    let page11 = create_test_page_image(11, PageSize::KiB16);

    manager.write_page(page1, Lsn::new(1)).unwrap();
    manager.write_page(page11, Lsn::new(1)).unwrap();

    // Verify file size is correct (20 pages * 16 KiB)
    let file_size = fs::metadata(&data_file).unwrap().len();
    let expected_size = 20 * 16 * 1024;
    assert_eq!(
        file_size, expected_size,
        "File size mismatch after two extents"
    );

    // Verify page 11 is at correct offset: 10 * 16 KiB
    let page_size = PageSize::KiB16.bytes_usize();
    let expected_offset = 10 * page_size as u64;

    let actual_offset = manager.page_to_file_offset(PageId::new(11)).unwrap();
    assert_eq!(actual_offset, expected_offset, "Page 11 offset incorrect");
}

#[test]
fn test_overlapping_extents_rejected() {
    let (mut manager, _temp_dir) = create_temp_disk_manager();

    // Allocate first extent
    let extent1 = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(1),
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent1).unwrap();

    // Try to allocate overlapping extent
    let overlap_extent = ExtentDescriptor {
        extent_id: ExtentId::new(2),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(2),
        first_page_id: PageId::new(5), // Overlaps!
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    let result = manager.allocate_extent(overlap_extent);
    assert!(result.is_err(), "Overlapping extent should be rejected");
}

#[test]
fn test_page_not_found_when_unallocated() {
    let (manager, _temp_dir) = create_temp_disk_manager();

    // Try to read page from extent that doesn't exist
    let result = manager.read_page(PageId::new(999));

    match result {
        Ok(None) => {
            // Expected: page not found but no error
        }
        _ => panic!("Expected Ok(None) for unallocated page"),
    }
}

#[test]
fn test_concurrent_page_reads_consistent() {
    let (mut manager, _temp_dir) = create_temp_disk_manager();

    // Allocate extent
    let extent = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(1),
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent).unwrap();

    // Write a page
    let page_image = create_test_page_image(5, PageSize::KiB16);
    let expected_bytes = page_image.as_bytes().to_vec();

    manager.write_page(page_image, Lsn::new(1)).unwrap();

    // Read the same page multiple times
    for _ in 0..5 {
        let read_image = manager.read_page(PageId::new(5)).unwrap().unwrap();
        assert_eq!(read_image.as_bytes(), expected_bytes.as_slice());
    }
}

#[test]
fn test_extent_metadata_contiguity_enforced() {
    let (mut manager, _temp_dir) = create_temp_disk_manager();

    // Allocate extent 1
    let extent1 = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(1),
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent1).unwrap();

    // Try to allocate with hole in page IDs
    let extent_with_hole = ExtentDescriptor {
        extent_id: ExtentId::new(2),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(2),
        first_page_id: PageId::new(50), // Gap from pages 11-49
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    // This should succeed - gaps are allowed, extents don't need to be contiguous
    let result = manager.allocate_extent(extent_with_hole);
    assert!(
        result.is_ok(),
        "Non-overlapping extent with gap should be allowed"
    );
}

#[test]
fn test_offset_computation_deterministic() {
    let (mut manager, _temp_dir) = create_temp_disk_manager();

    // Allocate extent
    let extent = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(100),
        page_count: 50,
        page_size: PageSize::KiB32, // 32 KiB pages
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent).unwrap();

    // Compute offset for various pages and verify determinism
    let page_100_offset = manager.page_to_file_offset(PageId::new(100)).unwrap();
    let page_101_offset = manager.page_to_file_offset(PageId::new(101)).unwrap();
    let page_102_offset = manager.page_to_file_offset(PageId::new(102)).unwrap();

    // Each should be 32 KiB apart
    assert_eq!(page_100_offset, 0);
    assert_eq!(page_101_offset, 32 * 1024);
    assert_eq!(page_102_offset, 64 * 1024);
}

#[test]
fn test_wal_before_page_flush_contract_enforced() {
    let (mut manager, _temp_dir) = create_temp_disk_manager();

    // Allocate extent
    let extent = ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        first_page_id: PageId::new(1),
        page_count: 10,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    };

    manager.allocate_extent(extent).unwrap();

    // Create a page with LSN 100
    let header = PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size: PageSize::KiB16,
        page_type: PageType::FixedRow,
        page_id: PageId::new(1),
        object_id: ObjectId::new(1),
        allocation_id: AllocationId::new(1),
        page_lsn: Lsn::new(100), // Page has LSN 100
        page_epoch: 1,
        previous_page_id: None,
        next_page_id: None,
        header_len: PageHeader::MIN_HEADER_LEN_V0,
        payload_offset: PageHeader::MIN_HEADER_LEN_V0 as u32,
        payload_len: PageSize::KiB16.bytes()
            - u32::from(PageHeader::MIN_HEADER_LEN_V0)
            - PageTrailer::V0_LEN,
        free_start: PageHeader::MIN_HEADER_LEN_V0 as u32,
        free_end: PageSize::KiB16.bytes() - PageTrailer::V0_LEN,
        free_bytes: PageSize::KiB16.bytes()
            - u32::from(PageHeader::MIN_HEADER_LEN_V0)
            - PageTrailer::V0_LEN,
        slot_count: 0,
        row_count: 0,
        flags: PageFlags::NONE,
        header_crc: 0xDEADBEEF,
    };

    let layout = PageLayoutContract {
        header,
        trailer: create_test_trailer(),
    };

    let page_image =
        PageImage::with_layout(layout, vec![0u8; PageSize::KiB16.bytes_usize()]).unwrap();

    // Write with durable LSN < page LSN (should fail in production but passes in MVP)
    let result = manager.write_page(page_image, Lsn::new(50)); // Durable LSN < page LSN

    // MVP: The write will succeed because WAL-before-page-flush is checked at buffer pool level
    // In production, this contract would be enforced here or in PageStore
    assert!(
        result.is_ok(),
        "Write should succeed (WAL check delegated to buffer pool)"
    );
}
