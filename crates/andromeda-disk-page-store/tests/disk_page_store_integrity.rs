#![forbid(unsafe_code)]

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use andromeda_disk_page_store::{DiskPageStore, PageIntegrityMode};
use andromeda_segment::{ExtentDescriptor, ExtentId, ExtentState};
use andromeda_storage_page::{
    AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageImage, PageLayoutContract,
    PageSize, PageStore, PageType, integrity_trailer_for_payload,
};

fn test_extent() -> ExtentDescriptor {
    ExtentDescriptor {
        extent_id: ExtentId::new(1),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(3),
        first_page_id: PageId::new(1),
        page_count: 1,
        page_size: PageSize::KiB16,
        state: ExtentState::AllocatingHot,
        segment_id: None,
        file_offset: 0,
        allocated_on_disk: false,
    }
}

fn page_header(header_crc: u32) -> PageHeader {
    PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size: PageSize::KiB16,
        page_type: PageType::FixedRow,
        page_id: PageId::new(1),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(3),
        page_lsn: Lsn::new(10),
        page_epoch: 1,
        previous_page_id: None,
        next_page_id: None,
        // PageCodecV1 requires header_len == 112 and payload_offset == 112.
        header_len: 112,
        payload_offset: 112,
        payload_len: 512,
        free_start: 256,
        free_end: 512,
        free_bytes: 256,
        slot_count: 1,
        row_count: 1,
        flags: PageFlags::NONE,
        header_crc,
    }
}

fn page_contract_for_bytes(header_crc: u32, bytes: &[u8]) -> PageLayoutContract {
    let header = page_header(header_crc);
    let payload_start = header.payload_offset as usize;
    let payload_end = payload_start + header.payload_len as usize;
    PageLayoutContract {
        header,
        trailer: integrity_trailer_for_payload(&header, &bytes[payload_start..payload_end]),
    }
}

fn page_image() -> PageImage {
    let mut bytes = vec![0; PageSize::KiB16.bytes_usize()];
    // Payload starts at offset 112 (PageCodecV1 header length), length 512.
    bytes[112..624].fill(0xA5);
    let contract = page_contract_for_bytes(5, &bytes);
    PageImage::with_layout(contract, bytes).expect("valid page image")
}

fn write_page(data_file: &Path, temp_io_dir: &Path, mode: PageIntegrityMode) -> PageLayoutContract {
    let mut store =
        DiskPageStore::new_with_integrity(data_file, temp_io_dir, mode).expect("disk page store");
    store.allocate_extent(test_extent()).expect("extent");
    store
        .write_page(page_image(), Lsn::new(10))
        .expect("write page");

    let persisted = store
        .read_page(PageId::new(1))
        .expect("read clean page")
        .expect("page exists");
    persisted
        .layout_contract()
        .expect("persisted page has layout")
}

fn reopen_store(data_file: &Path, temp_io_dir: &Path, mode: PageIntegrityMode) -> DiskPageStore {
    let mut store = DiskPageStore::new_with_integrity(data_file, temp_io_dir, mode)
        .expect("reopen disk page store");
    store.register_extent(test_extent()).expect("extent");
    store
}

fn corrupt_byte(data_file: &Path, offset: u64) -> u8 {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(data_file)
        .expect("open data file");
    file.seek(SeekFrom::Start(offset)).expect("seek corruption");
    let mut byte = [0];
    file.read_exact(&mut byte).expect("read byte");
    byte[0] ^= 0x01;
    file.seek(SeekFrom::Start(offset)).expect("seek rewrite");
    file.write_all(&byte).expect("write corruption");
    file.sync_all().expect("sync corruption");
    byte[0]
}

fn read_u32_at(data_file: &Path, offset: u64) -> u32 {
    let mut file = OpenOptions::new()
        .read(true)
        .open(data_file)
        .expect("open data file");
    file.seek(SeekFrom::Start(offset)).expect("seek u32");
    let mut bytes = [0; 4];
    file.read_exact(&mut bytes).expect("read u32");
    u32::from_le_bytes(bytes)
}

#[test]
fn header_crc32_stamps_persisted_layout_and_reads_clean_page() {
    let temp_dir = tempfile::TempDir::new().expect("temp dir");
    let data_file = temp_dir.path().join("pages.bin");
    let temp_io_dir = temp_dir.path().join("io");

    let persisted_contract = write_page(&data_file, &temp_io_dir, PageIntegrityMode::HeaderCrc32);

    assert_ne!(persisted_contract.header.header_crc, 5);
    assert_eq!(
        persisted_contract.header.header_crc,
        // PageCodecV1 outer CRC (header_crc) lives at bytes 100-103.
        read_u32_at(&data_file, 100)
    );
    assert_eq!(persisted_contract.header.page_id, PageId::new(1));
    assert_eq!(persisted_contract.header.page_lsn, Lsn::new(10));
}

#[test]
fn header_crc32_rejects_corrupted_header_payload_and_trailer() {
    for (label, offset) in [
        ("header", 16),
        ("payload", 256),
        ("trailer", PageSize::KiB16.bytes_usize() - 1),
    ] {
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let data_file = temp_dir.path().join(format!("{label}.bin"));
        let temp_io_dir = temp_dir.path().join("io");

        write_page(&data_file, &temp_io_dir, PageIntegrityMode::HeaderCrc32);
        corrupt_byte(&data_file, offset as u64);

        let store = reopen_store(&data_file, &temp_io_dir, PageIntegrityMode::HeaderCrc32);
        let error = store
            .read_page(PageId::new(1))
            .expect_err("corrupted page must be rejected");
        assert!(
            error.message().contains("integrity") || error.message().contains("CRC"),
            "expected integrity error for {label} corruption, got: {}",
            error.message()
        );
    }
}

#[test]
fn header_crc32_rejects_semantically_valid_header_bit_flip() {
    let temp_dir = tempfile::TempDir::new().expect("temp dir");
    let data_file = temp_dir.path().join("pages.bin");
    let temp_io_dir = temp_dir.path().join("io");

    write_page(&data_file, &temp_io_dir, PageIntegrityMode::HeaderCrc32);
    corrupt_byte(&data_file, 88);

    let store = reopen_store(&data_file, &temp_io_dir, PageIntegrityMode::HeaderCrc32);
    let error = store
        .read_page(PageId::new(1))
        .expect_err("header CRC mode must reject row-count header corruption");
    assert!(
        error.message().contains("integrity") || error.message().contains("CRC"),
        "expected header integrity error, got: {}",
        error.message()
    );
}

#[test]
fn none_mode_rejects_payload_corruption_through_page_codec_integrity() {
    let temp_dir = tempfile::TempDir::new().expect("temp dir");
    let data_file = temp_dir.path().join("pages.bin");
    let temp_io_dir = temp_dir.path().join("io");

    write_page(&data_file, &temp_io_dir, PageIntegrityMode::None);
    corrupt_byte(&data_file, 256);

    let store = reopen_store(&data_file, &temp_io_dir, PageIntegrityMode::None);
    let error = store
        .read_page(PageId::new(1))
        .expect_err("payload corruption must be rejected");
    assert!(
        error.message().contains("CRC") || error.message().contains("hash"),
        "expected page codec integrity error, got: {}",
        error.message()
    );
}

#[test]
fn none_mode_pagecodecv1_detects_header_corruption_via_integrity_crc() {
    // In PageCodecV1, bytes 0-103 of the header are covered by the
    // HeaderIntegrityCrc at bytes 104-107. Any single-bit corruption
    // within the header — including the `free_bytes` field at offset 88 —
    // is detected even in None mode (no outer CRC). This is the expected
    // behavior under the unified codec and is NOT a gap in the boundary.
    let temp_dir = tempfile::TempDir::new().expect("temp dir");
    let data_file = temp_dir.path().join("pages.bin");
    let temp_io_dir = temp_dir.path().join("io");

    write_page(&data_file, &temp_io_dir, PageIntegrityMode::None);
    // Corrupt byte 88 = `free_bytes` field in PageCodecV1 header layout.
    corrupt_byte(&data_file, 88);

    let store = reopen_store(&data_file, &temp_io_dir, PageIntegrityMode::None);
    let error = store
        .read_page(PageId::new(1))
        .expect_err("PageCodecV1 HeaderIntegrityCrc must catch header corruption in None mode");
    assert!(
        error.message().contains("integrity")
            || error.message().contains("CRC")
            || error.message().contains("free")
            || error.message().contains("offset")
            || error.message().contains("invalid"),
        "expected HeaderIntegrityCrc or header-validation error, got: {}",
        error.message()
    );
}
