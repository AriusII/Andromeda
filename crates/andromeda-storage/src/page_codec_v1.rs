use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use sha2::{Digest, Sha256};

use crate::{
    AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageLayoutContract, PageSize,
    PageTrailer, PageType,
};

pub const PAGE_CODEC_V1_HEADER_LEN: usize = 112;
pub const PAGE_CODEC_V1_TRAILER_LEN: usize = 48;
const PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET: usize = 104;
const PAGE_CODEC_V1_HEADER_INTEGRITY_LEN: usize = 4;
const PAGE_CODEC_V1_HEADER_LEN_U16: u16 = PAGE_CODEC_V1_HEADER_LEN as u16;
const PAGE_CODEC_V1_HEADER_LEN_U32: u32 = PAGE_CODEC_V1_HEADER_LEN as u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPageV1 {
    pub header: PageHeader,
    pub payload: Vec<u8>,
    pub trailer: PageTrailer,
}

pub struct PageCodecV1;

impl PageCodecV1 {
    pub fn encode_header(header: &PageHeader) -> AndromedaResult<[u8; PAGE_CODEC_V1_HEADER_LEN]> {
        header.validate()?;
        validate_v1_fixed_header_layout(header)?;
        let mut bytes = [0u8; PAGE_CODEC_V1_HEADER_LEN];
        write_u32(&mut bytes, 0, header.magic);
        write_u16(&mut bytes, 4, header.format_version);
        write_u16(&mut bytes, 6, page_size_tag(header.page_size));
        write_u16(&mut bytes, 8, page_type_tag(header.page_type));
        write_u16(&mut bytes, 10, header.flags.bits());
        write_u64(&mut bytes, 12, header.page_id.get());
        write_u64(&mut bytes, 20, header.object_id.get());
        write_u64(&mut bytes, 28, header.allocation_id.get());
        write_u64(&mut bytes, 36, header.page_lsn.get());
        write_u64(&mut bytes, 44, header.page_epoch);
        write_u64(
            &mut bytes,
            52,
            header.previous_page_id.map_or(0, PageId::get),
        );
        write_u64(&mut bytes, 60, header.next_page_id.map_or(0, PageId::get));
        write_u16(&mut bytes, 68, header.header_len);
        write_u32(&mut bytes, 72, header.payload_offset);
        write_u32(&mut bytes, 76, header.payload_len);
        write_u32(&mut bytes, 80, header.free_start);
        write_u32(&mut bytes, 84, header.free_end);
        write_u32(&mut bytes, 88, header.free_bytes);
        write_u16(&mut bytes, 92, header.slot_count);
        write_u32(&mut bytes, 96, header.row_count);
        write_u32(&mut bytes, 100, header.header_crc);
        let integrity_crc = header_integrity_crc32(&bytes);
        write_u32(
            &mut bytes,
            PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET,
            integrity_crc,
        );
        Ok(bytes)
    }

    pub fn decode_header(bytes: &[u8]) -> AndromedaResult<PageHeader> {
        if bytes.len() < PAGE_CODEC_V1_HEADER_LEN {
            return Err(storage_error("page codec v1 header is truncated"));
        }
        let magic = read_u32(bytes, 0)?;
        if magic != PageHeader::MAGIC {
            return Err(storage_error("page codec v1 little-endian magic mismatch"));
        }
        let format_version = read_u16(bytes, 4)?;
        if format_version != PageHeader::FORMAT_VERSION_V0 {
            return Err(storage_error("unsupported page codec v1 format version"));
        }
        let header_len = read_u16(bytes, 68)?;
        if header_len != PAGE_CODEC_V1_HEADER_LEN_U16 {
            return Err(storage_error(
                "page codec v1 header_len must match fixed header length",
            ));
        }
        let payload_offset = read_u32(bytes, 72)?;
        if payload_offset != PAGE_CODEC_V1_HEADER_LEN_U32 {
            return Err(storage_error(
                "page codec v1 payload_offset must match fixed header length",
            ));
        }
        let page_size = page_size_from_tag(read_u16(bytes, 6)?)?;
        let page_type = page_type_from_tag(read_u16(bytes, 8)?)?;
        let header = PageHeader {
            magic,
            format_version,
            page_size,
            page_type,
            page_id: PageId::new(read_u64(bytes, 12)?),
            object_id: ObjectId::new(read_u64(bytes, 20)?),
            allocation_id: AllocationId::new(read_u64(bytes, 28)?),
            page_lsn: Lsn::new(read_u64(bytes, 36)?),
            page_epoch: read_u64(bytes, 44)?,
            previous_page_id: match read_u64(bytes, 52)? {
                0 => None,
                value => Some(PageId::new(value)),
            },
            next_page_id: match read_u64(bytes, 60)? {
                0 => None,
                value => Some(PageId::new(value)),
            },
            header_len,
            payload_offset,
            payload_len: read_u32(bytes, 76)?,
            free_start: read_u32(bytes, 80)?,
            free_end: read_u32(bytes, 84)?,
            free_bytes: read_u32(bytes, 88)?,
            slot_count: read_u16(bytes, 92)?,
            row_count: read_u32(bytes, 96)?,
            flags: PageFlags::new(read_u16(bytes, 10)?),
            header_crc: read_u32(bytes, 100)?,
        };
        header.validate()?;
        let expected = read_u32(bytes, PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET)?;
        if expected == 0 {
            return Err(storage_error("page codec v1 header integrity CRC is zero"));
        }
        let actual = header_integrity_crc32(&bytes[..PAGE_CODEC_V1_HEADER_LEN]);
        if expected != actual {
            return Err(storage_error("page codec v1 header integrity CRC mismatch"));
        }
        Ok(header)
    }

    pub fn encode_trailer(
        trailer: &PageTrailer,
    ) -> AndromedaResult<[u8; PAGE_CODEC_V1_TRAILER_LEN]> {
        trailer.validate()?;
        let mut bytes = [0u8; PAGE_CODEC_V1_TRAILER_LEN];
        write_u64(&mut bytes, 0, trailer.payload_crc64);
        bytes[8..40].copy_from_slice(&trailer.page_hash);
        write_u64(&mut bytes, 40, trailer.torn_write_guard);
        Ok(bytes)
    }

    pub fn decode_trailer(bytes: &[u8]) -> AndromedaResult<PageTrailer> {
        if bytes.len() < PAGE_CODEC_V1_TRAILER_LEN {
            return Err(storage_error("page codec v1 trailer is truncated"));
        }
        let mut page_hash = [0u8; 32];
        page_hash.copy_from_slice(&bytes[8..40]);
        let trailer = PageTrailer {
            payload_crc64: read_u64(bytes, 0)?,
            page_hash,
            torn_write_guard: read_u64(bytes, 40)?,
        };
        trailer.validate()?;
        Ok(trailer)
    }

    pub fn encode_page(
        header: &PageHeader,
        payload: &[u8],
        trailer: &PageTrailer,
    ) -> AndromedaResult<Vec<u8>> {
        let contract = PageLayoutContract {
            header: *header,
            trailer: *trailer,
        };
        contract.validate()?;
        if payload.len() != header.payload_len as usize {
            return Err(storage_error(
                "page payload length does not match header payload_len",
            ));
        }
        if payload_crc64(payload) != trailer.payload_crc64 {
            return Err(storage_error("page payload CRC mismatch"));
        }
        let mut bytes = Vec::with_capacity(
            PAGE_CODEC_V1_HEADER_LEN + payload.len() + PAGE_CODEC_V1_TRAILER_LEN,
        );
        bytes.extend_from_slice(&Self::encode_header(header)?);
        bytes.extend_from_slice(payload);
        bytes.extend_from_slice(&Self::encode_trailer(trailer)?);
        Ok(bytes)
    }

    pub fn decode_page(bytes: &[u8]) -> AndromedaResult<DecodedPageV1> {
        if bytes.len() < PAGE_CODEC_V1_HEADER_LEN + PAGE_CODEC_V1_TRAILER_LEN {
            return Err(storage_error(
                "page image is smaller than v1 header+trailer",
            ));
        }
        let header = Self::decode_header(&bytes[..PAGE_CODEC_V1_HEADER_LEN])?;
        let payload_len = usize::try_from(header.payload_len)
            .map_err(|_| storage_error("page payload_len does not fit usize"))?;
        let payload_end = PAGE_CODEC_V1_HEADER_LEN
            .checked_add(payload_len)
            .ok_or_else(|| storage_error("page payload length overflows image offset"))?;
        let expected_len = payload_end
            .checked_add(PAGE_CODEC_V1_TRAILER_LEN)
            .ok_or_else(|| storage_error("page image length overflows usize"))?;
        if bytes.len() != expected_len {
            return Err(storage_error(
                "page image length does not match header payload_len",
            ));
        }
        let payload = bytes[PAGE_CODEC_V1_HEADER_LEN..payload_end].to_vec();
        let trailer = Self::decode_trailer(&bytes[payload_end..])?;
        if payload_crc64(&payload) != trailer.payload_crc64 {
            return Err(storage_error("page payload CRC mismatch"));
        }
        Ok(DecodedPageV1 {
            header,
            payload,
            trailer,
        })
    }
}

fn page_size_tag(size: PageSize) -> u16 {
    match size {
        PageSize::KiB16 => 1,
        PageSize::KiB32 => 2,
    }
}

fn page_size_from_tag(tag: u16) -> AndromedaResult<PageSize> {
    match tag {
        1 => Ok(PageSize::KiB16),
        2 => Ok(PageSize::KiB32),
        _ => Err(storage_error("unknown page size tag")),
    }
}

fn page_type_tag(page_type: PageType) -> u16 {
    match page_type {
        PageType::FixedRow => 1,
        PageType::HybridRow => 2,
        PageType::Manifest => 3,
        PageType::Free => 4,
    }
}

fn page_type_from_tag(tag: u16) -> AndromedaResult<PageType> {
    match tag {
        1 => Ok(PageType::FixedRow),
        2 => Ok(PageType::HybridRow),
        3 => Ok(PageType::Manifest),
        4 => Ok(PageType::Free),
        _ => Err(storage_error("unknown page type tag")),
    }
}

fn write_u16(target: &mut [u8], offset: usize, value: u16) {
    target[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn write_u32(target: &mut [u8], offset: usize, value: u32) {
    target[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn write_u64(target: &mut [u8], offset: usize, value: u64) {
    target[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u16(source: &[u8], offset: usize) -> AndromedaResult<u16> {
    let mut bytes = [0u8; 2];
    bytes.copy_from_slice(
        source
            .get(offset..offset + 2)
            .ok_or_else(|| storage_error("truncated u16 field"))?,
    );
    Ok(u16::from_le_bytes(bytes))
}
fn read_u32(source: &[u8], offset: usize) -> AndromedaResult<u32> {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(
        source
            .get(offset..offset + 4)
            .ok_or_else(|| storage_error("truncated u32 field"))?,
    );
    Ok(u32::from_le_bytes(bytes))
}
fn read_u64(source: &[u8], offset: usize) -> AndromedaResult<u64> {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(
        source
            .get(offset..offset + 8)
            .ok_or_else(|| storage_error("truncated u64 field"))?,
    );
    Ok(u64::from_le_bytes(bytes))
}

fn validate_v1_fixed_header_layout(header: &PageHeader) -> AndromedaResult<()> {
    if header.header_len != PAGE_CODEC_V1_HEADER_LEN_U16 {
        return Err(storage_error(
            "page codec v1 header_len must match fixed header length",
        ));
    }
    if header.payload_offset != PAGE_CODEC_V1_HEADER_LEN_U32 {
        return Err(storage_error(
            "page codec v1 payload_offset must match fixed header length",
        ));
    }
    Ok(())
}

fn header_integrity_crc32(header_bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for (offset, byte) in header_bytes.iter().copied().enumerate() {
        let byte = if (PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET
            ..PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET + PAGE_CODEC_V1_HEADER_INTEGRITY_LEN)
            .contains(&offset)
        {
            0
        } else {
            byte
        };
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    let crc = !crc;
    if crc == 0 { 1 } else { crc }
}

pub fn payload_crc64(payload: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = FNV_OFFSET;
    for byte in payload {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    if state == 0 { 1 } else { state }
}

pub fn payload_hash(payload: &[u8]) -> [u8; 32] {
    let hash: [u8; 32] = Sha256::digest(payload).into();
    if hash == [0; 32] { [1; 32] } else { hash }
}

pub fn torn_write_guard(header: &PageHeader, payload_crc64: u64) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut state = FNV_OFFSET;
    for value in [
        u64::from(header.magic),
        u64::from(header.format_version),
        header.page_id.get(),
        header.object_id.get(),
        header.allocation_id.get(),
        header.page_lsn.get(),
        header.page_epoch,
        payload_crc64,
        u64::from(header.payload_offset),
        u64::from(header.payload_len),
    ] {
        for byte in value.to_le_bytes() {
            state ^= u64::from(byte);
            state = state.wrapping_mul(FNV_PRIME);
        }
    }

    if state == 0 || state == header.page_id.get() {
        state ^ 0xA9D3_78B5_4C2F_6101
    } else {
        state
    }
}

pub fn integrity_trailer_for_payload(header: &PageHeader, payload: &[u8]) -> PageTrailer {
    let payload_crc64 = payload_crc64(payload);
    PageTrailer {
        payload_crc64,
        page_hash: payload_hash(payload),
        torn_write_guard: torn_write_guard(header, payload_crc64),
    }
}

pub fn validate_payload_integrity(
    header: &PageHeader,
    payload: &[u8],
    trailer: &PageTrailer,
) -> AndromedaResult<()> {
    let expected = integrity_trailer_for_payload(header, payload);
    if trailer.payload_crc64 != expected.payload_crc64 {
        return Err(storage_error("page payload CRC mismatch"));
    }
    if trailer.page_hash != expected.page_hash {
        return Err(storage_error("page payload hash mismatch"));
    }
    if trailer.torn_write_guard != expected.torn_write_guard {
        return Err(storage_error("page torn-write guard mismatch"));
    }
    Ok(())
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
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

    fn sample_trailer(payload: &[u8]) -> PageTrailer {
        PageTrailer {
            payload_crc64: payload_crc64(payload),
            page_hash: [3; 32],
            torn_write_guard: 99,
        }
    }

    #[test]
    fn header_and_trailer_roundtrip() {
        let header = sample_header(PageSize::KiB16, 32);
        let trailer = sample_trailer(&[1; 32]);
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
        let trailer = sample_trailer(&payload);
        let encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();
        let decoded = PageCodecV1::decode_page(&encoded).unwrap();
        assert_eq!(decoded.header, header);
        assert_eq!(decoded.payload, payload);
        assert_eq!(decoded.trailer, trailer);
    }

    #[test]
    fn malformed_rejections_unknown_tag_truncated_and_crc() {
        let mut header_bytes =
            PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
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

        let mut header_bytes =
            PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
        header_bytes[96] ^= 0x01;
        assert_eq!(
            PageCodecV1::decode_header(&header_bytes)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Storage
        );

        let payload = vec![1u8; 8];
        let header = sample_header(PageSize::KiB16, payload.len() as u32);
        let mut trailer = sample_trailer(&payload);
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
    }

    #[test]
    fn header_decode_checks_magic_version_and_fixed_lengths_first() {
        let mut header_bytes =
            PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
        header_bytes[0..4].copy_from_slice(&0u32.to_le_bytes());
        header_bytes[6] = 0xFF;
        let error = PageCodecV1::decode_header(&header_bytes).unwrap_err();
        assert!(error.message().contains("magic"));

        let mut header_bytes =
            PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
        header_bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        let error = PageCodecV1::decode_header(&header_bytes).unwrap_err();
        assert!(error.message().contains("format version"));

        let mut header_bytes =
            PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
        write_u16(&mut header_bytes, 68, PageHeader::MIN_HEADER_LEN_V0);
        let error = PageCodecV1::decode_header(&header_bytes).unwrap_err();
        assert!(error.message().contains("header_len"));

        let mut header_bytes =
            PageCodecV1::encode_header(&sample_header(PageSize::KiB16, 8)).unwrap();
        write_u32(&mut header_bytes, 72, PAGE_CODEC_V1_HEADER_LEN_U32 + 1);
        let error = PageCodecV1::decode_header(&header_bytes).unwrap_err();
        assert!(error.message().contains("payload_offset"));
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
            let trailer = sample_trailer(&payload);
            let encoded = PageCodecV1::encode_page(&header, &payload, &trailer).unwrap();
            let decoded = PageCodecV1::decode_page(&encoded).unwrap();
            assert_eq!(decoded.header.page_size, size);
            assert_eq!(decoded.payload.len(), payload_len);
        }
    }
}
