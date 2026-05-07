use andromeda_core::AndromedaErrorKind;

use crate::{
    AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageSize, PageTrailer, PageType,
};

use super::binary::{read_u32, write_u16, write_u32};
use super::format::{
    PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET, PAGE_CODEC_V1_HEADER_LEN, PAGE_CODEC_V1_HEADER_LEN_U32,
};
use super::*;

fn sample_header(page_size: PageSize, payload_len: u32) -> PageHeader {
    PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size,
        page_type: PageType::FixedRow,
        page_id: PageId::new(9),
        object_id: ObjectId::new(8),
        allocation_id: AllocationId::new(7),
        page_lsn: Lsn::new(11),
        page_epoch: 2,
        previous_page_id: Some(PageId::new(6)),
        next_page_id: Some(PageId::new(10)),
        header_len: PAGE_CODEC_V1_HEADER_LEN as u16,
        payload_offset: PAGE_CODEC_V1_HEADER_LEN as u32,
        payload_len,
        free_start: PAGE_CODEC_V1_HEADER_LEN as u32,
        free_end: PAGE_CODEC_V1_HEADER_LEN as u32,
        free_bytes: 0,
        slot_count: 1,
        row_count: 1,
        flags: PageFlags::HAS_PREVIOUS.with(PageFlags::HAS_NEXT),
        header_crc: 0x0102_0304,
    }
}

fn sample_trailer(header: &PageHeader, payload: &[u8]) -> PageTrailer {
    integrity_trailer_for_payload(header, payload)
}

#[test]
fn header_and_trailer_roundtrip() {
    let header = sample_header(PageSize::KiB16, 32);
    let trailer = sample_trailer(&header, &[1; 32]);
    let encoded_h = PageCodecV1::encode_header(&header).unwrap();
    let encoded_t = PageCodecV1::encode_trailer(&trailer).unwrap();
    assert_ne!(
        read_u32(&encoded_h, PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET).unwrap(),
        0
    );
    assert_eq!(PageCodecV1::decode_header(&encoded_h).unwrap(), header);
    assert_eq!(PageCodecV1::decode_trailer(&encoded_t).unwrap(), trailer);
}

#[test]
fn payload_roundtrip_and_crc_validation() {
    let payload = vec![5u8; 64];
    let header = sample_header(PageSize::KiB16, payload.len() as u32);
    let trailer = sample_trailer(&header, &payload);
    let encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();
    let decoded = PageCodecV1::decode_page(&encoded).unwrap();
    assert_eq!(decoded.header, header);
    assert_eq!(decoded.payload, payload);
    assert_eq!(decoded.trailer, trailer);
}

#[test]
fn malformed_rejections_unknown_tag_truncated_and_crc() {
    let mut header_bytes = PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
    header_bytes[6] = 0xFF;
    header_bytes[7] = 0x00;
    assert_eq!(
        PageCodecV1::decode_header(&header_bytes)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );

    assert_eq!(
        PageCodecV1::decode_header(&header_bytes[..60])
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );

    let mut header_bytes = PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
    header_bytes[96] ^= 0x01;
    assert_eq!(
        PageCodecV1::decode_header(&header_bytes)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Storage
    );

    let payload = vec![1u8; 8];
    let header = sample_header(PageSize::KiB16, payload.len() as u32);
    let mut trailer = sample_trailer(&header, &payload);
    trailer.payload_crc64 ^= 1;
    let encoded = PageCodecV1::encode_header(&header)
        .unwrap()
        .into_iter()
        .chain(payload.clone())
        .chain(PageCodecV1::encode_trailer(&trailer).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        PageCodecV1::decode_page(&encoded).unwrap_err().kind(),
        AndromedaErrorKind::Storage
    );

    let payload = vec![2u8; 8];
    let header = sample_header(PageSize::KiB16, payload.len() as u32);
    let mut trailer = sample_trailer(&header, &payload);
    trailer.page_hash[0] ^= 1;
    let encoded = PageCodecV1::encode_header(&header)
        .unwrap()
        .into_iter()
        .chain(payload.clone())
        .chain(PageCodecV1::encode_trailer(&trailer).unwrap())
        .collect::<Vec<_>>();
    let error = PageCodecV1::decode_page(&encoded).unwrap_err();
    assert!(error.message().contains("hash"));

    let payload = vec![3u8; 8];
    let header = sample_header(PageSize::KiB16, payload.len() as u32);
    let mut trailer = sample_trailer(&header, &payload);
    trailer.torn_write_guard ^= 1;
    let encoded = PageCodecV1::encode_header(&header)
        .unwrap()
        .into_iter()
        .chain(payload.clone())
        .chain(PageCodecV1::encode_trailer(&trailer).unwrap())
        .collect::<Vec<_>>();
    let error = PageCodecV1::decode_page(&encoded).unwrap_err();
    assert!(error.message().contains("torn-write"));
}

#[test]
fn header_decode_checks_magic_version_and_fixed_lengths_first() {
    let mut header_bytes = PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
    header_bytes[0..4].copy_from_slice(&0u32.to_le_bytes());
    header_bytes[6] = 0xFF;
    let error = PageCodecV1::decode_header(&header_bytes).unwrap_err();
    assert!(error.message().contains("magic"));

    let mut header_bytes = PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
    header_bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
    let error = PageCodecV1::decode_header(&header_bytes).unwrap_err();
    assert!(error.message().contains("format version"));

    let mut header_bytes = PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
    write_u16(&mut header_bytes, 68, PageHeader::MIN_HEADER_LEN_V0);
    let error = PageCodecV1::decode_header(&header_bytes).unwrap_err();
    assert!(error.message().contains("header_len"));

    let mut header_bytes = PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
    write_u32(&mut header_bytes, 72, PAGE_CODEC_V1_HEADER_LEN_U32 + 1);
    let error = PageCodecV1::decode_header(&header_bytes).unwrap_err();
    assert!(error.message().contains("payload_offset"));
}

#[test]
fn standalone_header_and_trailer_decoders_require_exact_lengths() {
    let header = sample_header(PageSize::KiB16, 8);
    let trailer = sample_trailer(&header, &[1; 8]);
    let header_bytes = PageCodecV1::encode_header(&header).unwrap();
    let trailer_bytes = PageCodecV1::encode_trailer(&trailer).unwrap();

    let mut oversized_header = header_bytes.to_vec();
    oversized_header.push(0);
    let err = PageCodecV1::decode_header(&oversized_header).unwrap_err();
    assert!(err.message().contains("header length"));

    let mut oversized_trailer = trailer_bytes.to_vec();
    oversized_trailer.push(0);
    let err = PageCodecV1::decode_trailer(&oversized_trailer).unwrap_err();
    assert!(err.message().contains("trailer length"));
}

#[test]
fn boundary_min_max_values() {
    let mut header = sample_header(PageSize::KiB16, 1);
    header.page_epoch = u64::MAX;
    header.page_id = PageId::new(u64::MAX);
    header.object_id = ObjectId::new(u64::MAX);
    header.allocation_id = AllocationId::new(u64::MAX);
    header.page_lsn = Lsn::new(u64::MAX);
    header.previous_page_id = Some(PageId::new(u64::MAX - 1));
    header.next_page_id = None;
    header.flags = PageFlags::HAS_PREVIOUS;
    let encoded = PageCodecV1::encode_header(&header).unwrap();
    let decoded = PageCodecV1::decode_header(&encoded).unwrap();
    assert_eq!(decoded, header);
}

#[test]
fn golden_header_vectors_match_expected_bytes() {
    let header = sample_header(PageSize::KiB16, 32);
    let encoded = PageCodecV1::encode_header(&header).unwrap();
    let golden_prefix: [u8; 24] = [
        0x52, 0x44, 0x4E, 0x41, // magic little-endian
        0x01, 0x00, // version
        0x01, 0x00, // size tag
        0x01, 0x00, // page type tag
        0x03, 0x00, // flags
        0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // page_id
        0x08, 0x00, 0x00, 0x00, // object_id prefix
    ];
    assert_eq!(&encoded[..24], &golden_prefix);
    assert_eq!(encoded.len(), PAGE_CODEC_V1_HEADER_LEN);
}

#[test]
fn corpus_page_sizes_16k_and_32k() {
    for (size, payload_len) in [(PageSize::KiB16, 16usize), (PageSize::KiB32, 64usize)] {
        let payload = vec![7u8; payload_len];
        let header = sample_header(size, payload.len() as u32);
        let trailer = sample_trailer(&header, &payload);
        let encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();
        let decoded = PageCodecV1::decode_page(&encoded).unwrap();
        assert_eq!(decoded.header.page_size, size);
        assert_eq!(decoded.payload.len(), payload_len);
    }
}
