use andromeda_core::AndromedaResult;

use crate::{
    AllocationId, Lsn, ObjectId, PageFlags, PageHeader, PageId, PageLayoutContract, PageTrailer,
};

use super::binary::{read_u16, read_u32, read_u64, write_u16, write_u32, write_u64};
use super::error::storage_error;
use super::format::{
    PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET, PAGE_CODEC_V1_HEADER_LEN, PAGE_CODEC_V1_HEADER_LEN_U16,
    PAGE_CODEC_V1_HEADER_LEN_U32, PAGE_CODEC_V1_TRAILER_LEN, header_integrity_crc32,
    page_size_from_tag, page_size_tag, page_type_from_tag, page_type_tag,
    validate_v1_fixed_header_layout,
};
use super::integrity::{payload_crc64, validate_payload_integrity};

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
        if bytes.len() != PAGE_CODEC_V1_HEADER_LEN {
            return Err(storage_error(
                "page codec v1 header length must match fixed header length",
            ));
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
        if bytes.len() != PAGE_CODEC_V1_TRAILER_LEN {
            return Err(storage_error(
                "page codec v1 trailer length must match fixed trailer length",
            ));
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
        validate_payload_integrity(&header, &payload, &trailer)?;
        Ok(DecodedPageV1 {
            header,
            payload,
            trailer,
        })
    }
}
