//! WAL Recovery Integration Test.
//!
//! This test verifies that WAL recovery can successfully read pages
//! written by DiskManager and reconstruct durable state.

use andromeda_disk_page_store::{DiskManager, FileDiskManager};
use andromeda_segment::{ExtentDescriptor, ExtentId, ExtentState};
use andromeda_storage_page::{
    AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageImage, PageLayoutContract,
    PageSize, PageTrailer, PageType,
};
use tempfile::TempDir;

/// Helper: Create a valid page header.
fn create_page_header(page_id: u64, page_size: PageSize, page_lsn: u64) -> PageHeader {
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
        page_lsn: Lsn::new(page_lsn),
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

/// Helper: Create page trailer.
fn create_page_trailer() -> PageTrailer {
    PageTrailer {
        torn_write_guard: 0xDEADBEEF,
        page_hash: [0xAB; 32],
        payload_crc64: 0xBEEFCAFE,
    }
}

/// Helper: Create page with recognizable pattern.
fn create_page_with_pattern(page_id: u64, page_size: PageSize, lsn: u64, pattern: u8) -> PageImage {
    let header = create_page_header(page_id, page_size, lsn);
    let trailer = create_page_trailer();
    let layout = PageLayoutContract { header, trailer };

    let bytes = vec![pattern; page_size.bytes_usize()];
    PageImage::with_layout(layout, bytes).unwrap()
}

// Test: WAL Recovery Can Read Pages Written by DiskManager

#[test]
fn test_wal_recovery_reads_pages_written_by_disk_manager() {
    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("wal_integration.bin");

    // Phase 1: Simulate buffer pool flushing pages to DiskManager
    // (This would normally be done by the buffer pool during dirty page flush)
    {
        let mut manager = FileDiskManager::open(&data_file, temp_dir.path()).unwrap();

        // Create extent for pages 1-50
        let extent = ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(100), // Object ID 100
            allocation_id: AllocationId::new(1),
            first_page_id: PageId::new(1),
            page_count: 50,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        };
        manager.allocate_extent(extent).unwrap();

        // Write pages representing a table's dirty pages at LSN 1000
        // These would have been modified in memory and marked for flush
        for page_num in 1..=5 {
            let page = create_page_with_pattern(page_num as u64, PageSize::KiB16, 1000, 0xAA);
            manager
                .write_page(page, Lsn::new(1000))
                .unwrap_or_else(|_| panic!("Failed to write page {}", page_num));
        }
    }

    // Phase 2: Recovery phase - Reopen DiskManager and verify pages are readable
    // (This simulates WAL recovery reading pages from cold storage)
    {
        let mut manager = FileDiskManager::open(&data_file, temp_dir.path()).unwrap();

        // Register extent (would come from manifest in real recovery)
        let extent = ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(100),
            allocation_id: AllocationId::new(1),
            first_page_id: PageId::new(1),
            page_count: 50,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        };
        manager.register_extent(extent).unwrap();

        // Verify recovery can read all pages
        for page_num in 1..=5 {
            let page_id = PageId::new(page_num as u64);
            let page = manager
                .read_page(page_id)
                .unwrap_or_else(|_| panic!("Recovery read failed for page {}", page_num))
                .unwrap_or_else(|| panic!("Page {} not found during recovery", page_num));

            // Verify page content is correct
            assert_eq!(
                page.as_bytes()[0],
                0xAA,
                "Page {} content pattern mismatch after recovery",
                page_num
            );
        }
    }
}

// Test: Recovery Reads Multiple Extents Written by DiskManager

#[test]
fn test_wal_recovery_reads_multiple_extents_from_disk_manager() {
    let temp_dir = TempDir::new().unwrap();
    let data_file = temp_dir.path().join("multi_extent_recovery.bin");

    // Phase 1: Write pages across multiple extents
    {
        let mut manager = FileDiskManager::open(&data_file, temp_dir.path()).unwrap();

        // Extent 1: Object 1, pages 1-25
        let extent1 = ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(1),
            allocation_id: AllocationId::new(1),
            first_page_id: PageId::new(1),
            page_count: 25,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        };
        manager.allocate_extent(extent1).unwrap();

        // Extent 2: Object 2, pages 26-50
        let extent2 = ExtentDescriptor {
            extent_id: ExtentId::new(2),
            object_id: ObjectId::new(2),
            allocation_id: AllocationId::new(2),
            first_page_id: PageId::new(26),
            page_count: 25,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        };
        manager.allocate_extent(extent2).unwrap();

        // Write pages from both extents at different LSNs
        for page_num in 1..=25 {
            let page = create_page_with_pattern(page_num as u64, PageSize::KiB16, 500, 0xBB);
            manager.write_page(page, Lsn::new(500)).unwrap();
        }

        for page_num in 26..=50 {
            let page = create_page_with_pattern(page_num as u64, PageSize::KiB16, 600, 0xCC);
            manager.write_page(page, Lsn::new(600)).unwrap();
        }
    }

    // Phase 2: Recovery reads from both extents
    {
        let mut manager = FileDiskManager::open(&data_file, temp_dir.path()).unwrap();

        let extent1 = ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(1),
            allocation_id: AllocationId::new(1),
            first_page_id: PageId::new(1),
            page_count: 25,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 0,
            allocated_on_disk: false,
        };
        manager.register_extent(extent1).unwrap();

        let extent2 = ExtentDescriptor {
            extent_id: ExtentId::new(2),
            object_id: ObjectId::new(2),
            allocation_id: AllocationId::new(2),
            first_page_id: PageId::new(26),
            page_count: 25,
            page_size: PageSize::KiB16,
            state: ExtentState::AllocatingHot,
            segment_id: None,
            file_offset: 25 * 16 * 1024,
            allocated_on_disk: false,
        };
        manager.register_extent(extent2).unwrap();

        // Verify pages from extent 1
        for page_num in 1..=25 {
            let page = manager
                .read_page(PageId::new(page_num as u64))
                .unwrap()
                .unwrap();
            assert_eq!(
                page.as_bytes()[0],
                0xBB,
                "Extent 1 page {} mismatch",
                page_num
            );
        }

        // Verify pages from extent 2
        for page_num in 26..=50 {
            let page = manager
                .read_page(PageId::new(page_num as u64))
                .unwrap()
                .unwrap();
            assert_eq!(
                page.as_bytes()[0],
                0xCC,
                "Extent 2 page {} mismatch",
                page_num
            );
        }
    }
}
