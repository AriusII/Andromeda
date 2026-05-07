use andromeda_core::AndromedaErrorKind;
use andromeda_storage::{
    AllocationId, Lsn, ObjectId, PageCodecV1, PageFlags, PageHeader, PageId, PageSize, PageType,
    integrity_trailer_for_payload,
};
use proptest::prelude::*;

fn header_for(size: PageSize, payload_len: usize) -> PageHeader {
    PageHeader {
        magic: PageHeader::MAGIC,
        format_version: PageHeader::FORMAT_VERSION_V0,
        page_size: size,
        page_type: PageType::FixedRow,
        page_id: PageId::new(1),
        object_id: ObjectId::new(2),
        allocation_id: AllocationId::new(3),
        page_lsn: Lsn::new(4),
        page_epoch: 1,
        previous_page_id: None,
        next_page_id: None,
        header_len: 112,
        payload_offset: 112,
        payload_len: payload_len as u32,
        free_start: 112,
        free_end: 112,
        free_bytes: 0,
        slot_count: 1,
        row_count: 1,
        flags: PageFlags::NONE,
        header_crc: 77,
    }
}

proptest! {
    #[test]
    fn page_codec_v1_roundtrip_is_stable(payload in proptest::collection::vec(any::<u8>(), 1..512), use_32k in any::<bool>()) {
        let size = if use_32k { PageSize::KiB32 } else { PageSize::KiB16 };
        let header = header_for(size, payload.len());
        let trailer = integrity_trailer_for_payload(&header, &payload);
        let encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();
        let decoded = PageCodecV1::decode_page(&encoded).unwrap();
        prop_assert_eq!(decoded.header, header);
        prop_assert_eq!(decoded.payload, payload);
        prop_assert_eq!(decoded.trailer, trailer);

        let mut oversized = encoded.clone();
        oversized.push(0);
        prop_assert_eq!(
            PageCodecV1::decode_page(&oversized).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );

        let mut bad_magic = encoded;
        bad_magic[0] ^= 0xff;
        prop_assert_eq!(
            PageCodecV1::decode_page(&bad_magic).unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
