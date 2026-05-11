//! Golden vector tests for PageCodecV1 on-disk format.
//!
//! These tests pin the deterministic byte layout of the PageCodecV1 112-byte
//! header and 48-byte trailer so that any accidental field-order change,
//! endianness regression, or size change is caught immediately.
//!
//! The golden constants encode the following known inputs:
//!
//!   page_size       = KiB16  (tag = 1)
//!   page_type       = FixedRow (tag = 1)
//!   flags           = NONE   (0)
//!   page_id         = 1
//!   object_id       = 2
//!   allocation_id   = 3
//!   page_lsn        = 4
//!   page_epoch      = 1
//!   prev_page_id    = None   (stored as 0)
//!   next_page_id    = None   (stored as 0)
//!   header_len      = 112    (0x70)
//!   payload_offset  = 112    (0x70)
//!   payload_len     = 8
//!   free_start      = 120    (= payload_offset + payload_len, no free space)
//!   free_end        = 120
//!   free_bytes      = 0
//!   slot_count      = 1
//!   row_count       = 1
//!   header_crc      = 0x0102_0304
//!
//! Reserved bytes at offsets 70-71, 94-95, and 108-111 must be zero.
//! HeaderIntegrityCrc at bytes 104-107 is runtime-computed and NOT hardcoded
//! because it depends on the software CRC-32 implementation.

#![forbid(unsafe_code)]

use andromeda_storage_page::{
    AllocationId, Lsn, ObjectId, PAGE_CODEC_V1_HEADER_LEN, PAGE_CODEC_V1_TRAILER_LEN, PageCodecV1,
    PageFlags, PageHeader, PageId, PageSize, PageType, header_integrity_crc32,
    integrity_trailer_for_payload,
};

/// The deterministic first 104 bytes of the golden header encoding.
///
/// Bytes 0-103 are fully determined by the input fields (no CRC dependency).
/// Bytes 104-107 (HeaderIntegrityCrc) and 108-111 (reserved tail = 0) are
/// tested separately via encode + decode roundtrip.
const GOLDEN_HEADER_BYTES_0_TO_103: [u8; 104] = [
    // [0..4]   magic = 0x414E_4452 little-endian ("RNDA")
    0x52, 0x44, 0x4E, 0x41, // [4..6]   format_version = 1 LE
    0x01, 0x00, // [6..8]   page_size_tag = 1 (KiB16) LE
    0x01, 0x00, // [8..10]  page_type_tag = 1 (FixedRow) LE
    0x01, 0x00, // [10..12] flags = 0 LE
    0x00, 0x00, // [12..20] page_id = 1 LE
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [20..28] object_id = 2 LE
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [28..36] allocation_id = 3 LE
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [36..44] page_lsn = 4 LE
    0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [44..52] page_epoch = 1 LE
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // [52..60] prev_page_id = 0 (None) LE
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // [60..68] next_page_id = 0 (None) LE
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // [68..70] header_len = 112 (0x70) LE
    0x70, 0x00, // [70..72] reserved pad0 — must be zero
    0x00, 0x00, // [72..76] payload_offset = 112 (0x70) LE
    0x70, 0x00, 0x00, 0x00, // [76..80] payload_len = 8 LE
    0x08, 0x00, 0x00, 0x00, // [80..84] free_start = 120 (= 112 + 8) LE
    0x78, 0x00, 0x00, 0x00, // [84..88] free_end = 120 LE
    0x78, 0x00, 0x00, 0x00, // [88..92] free_bytes = 0 LE
    0x00, 0x00, 0x00, 0x00, // [92..94] slot_count = 1 LE
    0x01, 0x00, // [94..96] reserved pad1 — must be zero
    0x00, 0x00, // [96..100] row_count = 1 LE
    0x01, 0x00, 0x00, 0x00, // [100..104] header_crc = 0x0102_0304 LE
    0x04, 0x03, 0x02, 0x01,
];

fn golden_header() -> PageHeader {
    PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size: PageSize::KiB16,
        page_type: PageType::FixedRow,
        page_id: PageId::new(1),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(3),
        page_lsn: Lsn::new(4),
        page_epoch: 1,
        previous_page_id: None,
        next_page_id: None,
        header_len: PAGE_CODEC_V1_HEADER_LEN as u16,
        payload_offset: PAGE_CODEC_V1_HEADER_LEN as u32,
        payload_len: 8,
        free_start: 120,
        free_end: 120,
        free_bytes: 0,
        slot_count: 1,
        row_count: 1,
        flags: PageFlags::NONE,
        header_crc: 0x0102_0304,
    }
}

/// The deterministic golden payload (8 bytes): a recognizable pattern.
const GOLDEN_PAYLOAD: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0x01, 0x02];

#[test]
fn page_codec_v1_header_golden_vector_bytes_0_to_103() {
    // Encode the golden header and assert bytes 0-103 are deterministic.
    let header = golden_header();
    let encoded = PageCodecV1::encode_header(&header).unwrap();

    assert_eq!(
        encoded.len(),
        PAGE_CODEC_V1_HEADER_LEN,
        "encoded header must be exactly {PAGE_CODEC_V1_HEADER_LEN} bytes"
    );
    assert_eq!(
        &encoded[..104],
        &GOLDEN_HEADER_BYTES_0_TO_103,
        "bytes 0-103 of the PageCodecV1 golden header diverge from the pinned constant"
    );
    // Bytes 108-111 are the reserved tail — must be zero.
    assert_eq!(
        &encoded[108..112],
        &[0u8; 4],
        "bytes 108-111 (reserved tail) must be zero"
    );
}

#[test]
fn page_codec_v1_header_golden_vector_hic_is_nonzero_and_self_consistent() {
    // The HeaderIntegrityCrc at bytes 104-107 is runtime-computed.
    // Assert it is nonzero and that it matches a fresh computation.
    let header = golden_header();
    let encoded = PageCodecV1::encode_header(&header).unwrap();
    let stored_hic = u32::from_le_bytes(encoded[104..108].try_into().unwrap());
    let computed_hic = header_integrity_crc32(&encoded);

    assert_ne!(stored_hic, 0, "HeaderIntegrityCrc must not be zero");
    assert_eq!(
        stored_hic, computed_hic,
        "stored HeaderIntegrityCrc at bytes 104-107 must match recomputed value"
    );
}

#[test]
fn page_codec_v1_header_golden_vector_roundtrip() {
    // Encode → decode and verify every field survives intact.
    let header = golden_header();
    let encoded = PageCodecV1::encode_header(&header).unwrap();
    let decoded = PageCodecV1::decode_header(&encoded).unwrap();

    assert_eq!(decoded.magic, PageHeader::MAGIC);
    assert_eq!(decoded.format_version, PageHeader::FORMAT_VERSION_V0);
    assert_eq!(decoded.page_size, PageSize::KiB16);
    assert_eq!(decoded.page_type, PageType::FixedRow);
    assert_eq!(decoded.page_id, PageId::new(1));
    assert_eq!(decoded.object_id, ObjectId::new(2));
    assert_eq!(decoded.allocation_id, AllocationId::new(3));
    assert_eq!(decoded.page_lsn, Lsn::new(4));
    assert_eq!(decoded.page_epoch, 1);
    assert_eq!(decoded.previous_page_id, None);
    assert_eq!(decoded.next_page_id, None);
    assert_eq!(decoded.header_len, PAGE_CODEC_V1_HEADER_LEN as u16);
    assert_eq!(decoded.payload_offset, PAGE_CODEC_V1_HEADER_LEN as u32);
    assert_eq!(decoded.payload_len, 8);
    assert_eq!(decoded.free_start, 120);
    assert_eq!(decoded.free_end, 120);
    assert_eq!(decoded.free_bytes, 0);
    assert_eq!(decoded.slot_count, 1);
    assert_eq!(decoded.row_count, 1);
    assert_eq!(decoded.flags, PageFlags::NONE);
    assert_eq!(decoded.header_crc, 0x0102_0304);
}

#[test]
fn page_codec_v1_golden_vector_full_page_roundtrip() {
    // Full encode_page → decode_page roundtrip with the golden payload.
    let header = golden_header();
    let payload = GOLDEN_PAYLOAD.to_vec();
    let trailer = integrity_trailer_for_payload(&header, &payload);
    let encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();

    assert_eq!(
        encoded.len(),
        PAGE_CODEC_V1_HEADER_LEN + GOLDEN_PAYLOAD.len() + PAGE_CODEC_V1_TRAILER_LEN,
        "encode_page stream length must equal header + payload + trailer"
    );

    // Bytes 0-103 of the stream match the golden constant.
    assert_eq!(&encoded[..104], &GOLDEN_HEADER_BYTES_0_TO_103);

    // Payload bytes follow immediately after the 112-byte header.
    assert_eq!(
        &encoded[PAGE_CODEC_V1_HEADER_LEN..PAGE_CODEC_V1_HEADER_LEN + 8],
        &GOLDEN_PAYLOAD
    );

    let decoded = PageCodecV1::decode_page(&encoded).unwrap();
    assert_eq!(decoded.header, header);
    assert_eq!(decoded.payload, payload);
    assert_eq!(decoded.trailer, trailer);
}

#[test]
fn page_codec_v1_trailer_is_deterministic_for_fixed_payload() {
    // Encoding the same payload twice must produce identical trailer bytes.
    let header = golden_header();
    let payload = GOLDEN_PAYLOAD.to_vec();
    let trailer_a = integrity_trailer_for_payload(&header, &payload);
    let trailer_b = integrity_trailer_for_payload(&header, &payload);
    assert_eq!(
        trailer_a, trailer_b,
        "integrity trailer must be deterministic for identical input"
    );

    // Encoding the trailer must also be deterministic.
    let bytes_a = PageCodecV1::encode_trailer(&trailer_a).unwrap();
    let bytes_b = PageCodecV1::encode_trailer(&trailer_b).unwrap();
    assert_eq!(
        bytes_a, bytes_b,
        "encoded trailer bytes must be deterministic"
    );
    assert_eq!(bytes_a.len(), PAGE_CODEC_V1_TRAILER_LEN);
}
