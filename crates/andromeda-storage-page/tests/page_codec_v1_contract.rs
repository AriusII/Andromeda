use andromeda_storage_page::{
    AllocationId, Lsn, ObjectId, PAGE_CODEC_V1_HEADER_LEN, PAGE_CODEC_V1_TRAILER_LEN, PageCodecV1,
    PageFlags, PageHeader, PageId, PageSize, PageType, integrity_trailer_for_payload,
};

fn sample_header(page_size: PageSize, payload_len: usize) -> PageHeader {
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
        payload_len: payload_len as u32,
        free_start: PAGE_CODEC_V1_HEADER_LEN as u32,
        free_end: PAGE_CODEC_V1_HEADER_LEN as u32,
        free_bytes: 0,
        slot_count: 1,
        row_count: 1,
        flags: PageFlags::HAS_PREVIOUS.with(PageFlags::HAS_NEXT),
        header_crc: 0x0102_0304,
    }
}

#[test]
fn page_codec_v1_roundtrip_preserves_full_page_image() {
    let payload = vec![0x5A; 128];
    let header = sample_header(PageSize::KiB16, payload.len());
    let trailer = integrity_trailer_for_payload(&header, &payload);

    let encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();
    let decoded = PageCodecV1::decode_page(&encoded).unwrap();

    assert_eq!(
        encoded.len(),
        PAGE_CODEC_V1_HEADER_LEN + payload.len() + PAGE_CODEC_V1_TRAILER_LEN
    );
    assert_eq!(decoded.header, header);
    assert_eq!(decoded.payload, payload);
    assert_eq!(decoded.trailer, trailer);
}

#[test]
fn page_codec_v1_rejects_corrupt_payload_integrity() {
    let payload = vec![0x11; 64];
    let header = sample_header(PageSize::KiB16, payload.len());
    let trailer = integrity_trailer_for_payload(&header, &payload);
    let mut encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();

    encoded[PAGE_CODEC_V1_HEADER_LEN] ^= 0xFF;

    let error = PageCodecV1::decode_page(&encoded).unwrap_err();
    assert!(error.message().contains("CRC"));
}

#[test]
fn page_codec_v1_rejects_non_fixed_header_length_bytes() {
    let header = sample_header(PageSize::KiB16, 8);
    let mut encoded_header = PageCodecV1::encode_header(&header).unwrap().to_vec();
    encoded_header.push(0);

    let error = PageCodecV1::decode_header(&encoded_header).unwrap_err();
    assert!(error.message().contains("header length"));
}

#[test]
fn page_codec_v1_rejects_truncated_page_image() {
    let payload = vec![0x7C; 48];
    let header = sample_header(PageSize::KiB16, payload.len());
    let trailer = integrity_trailer_for_payload(&header, &payload);
    let encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();

    let truncated = &encoded[..encoded.len() - 1];
    let error = PageCodecV1::decode_page(truncated).unwrap_err();

    assert!(
        error.message().contains("trailer")
            || error.message().contains("length")
            || error.message().contains("truncated")
    );
}
