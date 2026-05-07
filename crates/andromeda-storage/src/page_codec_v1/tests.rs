use andromeda_core::AndromedaErrorKind;
use sha2::{Digest, Sha256};

use crate::{
    AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageSize, PageTrailer, PageType,
};

use super::binary::{read_u32, write_u16, write_u32};
use super::format::{
    PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET, PAGE_CODEC_V1_HEADER_LEN, PAGE_CODEC_V1_HEADER_LEN_U32,
    PAGE_CODEC_V1_TRAILER_LEN,
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

fn patterned_payload(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(seed))
        .collect()
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

#[test]
fn full_page_golden_vectors_pin_16k_and_32k_images() {
    struct FullPageGolden {
        page_size: PageSize,
        payload_len: usize,
        payload_seed: u8,
        image_len: usize,
        header_integrity_crc: u32,
        payload_crc64: u64,
        torn_write_guard: u64,
        payload_prefix: [u8; 16],
        payload_suffix: [u8; 16],
        payload_hash: [u8; 32],
        image_sha256: [u8; 32],
    }

    let vectors = [
        FullPageGolden {
            page_size: PageSize::KiB16,
            payload_len: 16 * 1024 - PAGE_CODEC_V1_HEADER_LEN - PAGE_CODEC_V1_TRAILER_LEN,
            payload_seed: 0x11,
            image_len: 16 * 1024,
            header_integrity_crc: 0x6246_fc29,
            payload_crc64: 0xda4b_9bd0_8670_e785,
            torn_write_guard: 0xd474_115b_c791_e7ff,
            payload_prefix: [
                0x11, 0x36, 0x5b, 0x80, 0xa5, 0xca, 0xef, 0x14, 0x39, 0x5e, 0x83, 0xa8, 0xcd, 0xf2,
                0x17, 0x3c,
            ],
            payload_suffix: [
                0xa1, 0xc6, 0xeb, 0x10, 0x35, 0x5a, 0x7f, 0xa4, 0xc9, 0xee, 0x13, 0x38, 0x5d, 0x82,
                0xa7, 0xcc,
            ],
            payload_hash: [
                0xc4, 0x4e, 0x2f, 0x29, 0xeb, 0xba, 0xbe, 0xfb, 0x10, 0x2f, 0x40, 0xef, 0xa8, 0xf8,
                0xc0, 0x96, 0xd2, 0x1a, 0x8e, 0x34, 0x00, 0x54, 0xf4, 0xb9, 0x3f, 0x47, 0x5c, 0x83,
                0xe5, 0x2c, 0x84, 0xc2,
            ],
            image_sha256: [
                0x2d, 0x14, 0xf0, 0x99, 0x5d, 0x18, 0x55, 0xd9, 0x1a, 0xf1, 0x01, 0xef, 0x2b, 0x01,
                0xd2, 0x55, 0x5c, 0x51, 0x7b, 0x07, 0xea, 0x5d, 0xfa, 0x2f, 0x1c, 0x50, 0x22, 0xd3,
                0x0f, 0x0d, 0xa6, 0x21,
            ],
        },
        FullPageGolden {
            page_size: PageSize::KiB32,
            payload_len: 32 * 1024 - PAGE_CODEC_V1_HEADER_LEN - PAGE_CODEC_V1_TRAILER_LEN,
            payload_seed: 0x29,
            image_len: 32 * 1024,
            header_integrity_crc: 0x1702_2fb5,
            payload_crc64: 0xb31e_06ac_1846_fec5,
            torn_write_guard: 0x16c5_a612_da2c_7651,
            payload_prefix: [
                0x29, 0x4e, 0x73, 0x98, 0xbd, 0xe2, 0x07, 0x2c, 0x51, 0x76, 0x9b, 0xc0, 0xe5, 0x0a,
                0x2f, 0x54,
            ],
            payload_suffix: [
                0xb9, 0xde, 0x03, 0x28, 0x4d, 0x72, 0x97, 0xbc, 0xe1, 0x06, 0x2b, 0x50, 0x75, 0x9a,
                0xbf, 0xe4,
            ],
            payload_hash: [
                0xf9, 0xc3, 0x6d, 0x67, 0x88, 0xbb, 0xea, 0x16, 0x5e, 0x1f, 0x01, 0xb1, 0xc7, 0x6d,
                0x23, 0x2c, 0x75, 0xe6, 0xfa, 0xfb, 0xe3, 0x0a, 0x24, 0xd2, 0x6f, 0xf6, 0x8b, 0x69,
                0x32, 0x5a, 0x62, 0x8a,
            ],
            image_sha256: [
                0x48, 0x94, 0xe0, 0xee, 0x5b, 0xf6, 0x4d, 0x11, 0xc3, 0xc6, 0x90, 0xa0, 0xc3, 0xb8,
                0x64, 0x33, 0x47, 0x1b, 0x4d, 0x07, 0xfb, 0xef, 0x90, 0x02, 0xb7, 0xef, 0xfb, 0xed,
                0xd3, 0xbc, 0x4d, 0x92,
            ],
        },
    ];

    for vector in vectors {
        let payload = patterned_payload(vector.payload_len, vector.payload_seed);
        let header = sample_header(vector.page_size, payload.len() as u32);
        let trailer = sample_trailer(&header, &payload);
        let encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();
        let decoded = PageCodecV1::decode_page(&encoded).unwrap();
        let image_digest: [u8; 32] = Sha256::digest(&encoded).into();
        let header_crc = u32::from_le_bytes(
            encoded
                [PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET..PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        let trailer_offset = PAGE_CODEC_V1_HEADER_LEN + vector.payload_len;

        assert_eq!(encoded.len(), vector.image_len);
        assert_eq!(encoded.len(), vector.page_size.bytes_usize());
        assert_eq!(image_digest, vector.image_sha256);
        assert_eq!(decoded.header, header);
        assert_eq!(decoded.payload, payload);
        assert_eq!(decoded.trailer, trailer);
        assert_eq!(
            PageCodecV1::encode_page(&decoded.header, &decoded.payload, &decoded.trailer).unwrap(),
            encoded
        );
        assert_eq!(header_crc, vector.header_integrity_crc);
        assert_eq!(&encoded[PAGE_CODEC_V1_HEADER_LEN..trailer_offset], &payload);
        assert_eq!(&payload[..16], &vector.payload_prefix);
        assert_eq!(&payload[payload.len() - 16..], &vector.payload_suffix);
        assert_eq!(decoded.trailer.payload_crc64, vector.payload_crc64);
        assert_eq!(decoded.trailer.page_hash, vector.payload_hash);
        assert_eq!(decoded.trailer.torn_write_guard, vector.torn_write_guard);
        assert_eq!(
            &encoded[trailer_offset..trailer_offset + PAGE_CODEC_V1_TRAILER_LEN],
            &PageCodecV1::encode_trailer(&trailer).unwrap()
        );
    }
}
