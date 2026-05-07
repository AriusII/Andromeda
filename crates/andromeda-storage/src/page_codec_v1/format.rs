use andromeda_core::AndromedaResult;

use crate::{PageHeader, PageSize, PageType};

use super::error::storage_error;

pub const PAGE_CODEC_V1_HEADER_LEN: usize = 112;
pub const PAGE_CODEC_V1_TRAILER_LEN: usize = 48;

pub(crate) const PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET: usize = 104;
const PAGE_CODEC_V1_HEADER_INTEGRITY_LEN: usize = 4;
pub(crate) const PAGE_CODEC_V1_HEADER_LEN_U16: u16 = PAGE_CODEC_V1_HEADER_LEN as u16;
pub(crate) const PAGE_CODEC_V1_HEADER_LEN_U32: u32 = PAGE_CODEC_V1_HEADER_LEN as u32;

pub(crate) fn page_size_tag(size: PageSize) -> u16 {
    match size {
        PageSize::KiB16 => 1,
        PageSize::KiB32 => 2,
    }
}

pub(crate) fn page_size_from_tag(tag: u16) -> AndromedaResult<PageSize> {
    match tag {
        1 => Ok(PageSize::KiB16),
        2 => Ok(PageSize::KiB32),
        _ => Err(storage_error("unknown page size tag")),
    }
}

pub(crate) fn page_type_tag(page_type: PageType) -> u16 {
    match page_type {
        PageType::FixedRow => 1,
        PageType::HybridRow => 2,
        PageType::Manifest => 3,
        PageType::Free => 4,
    }
}

pub(crate) fn page_type_from_tag(tag: u16) -> AndromedaResult<PageType> {
    match tag {
        1 => Ok(PageType::FixedRow),
        2 => Ok(PageType::HybridRow),
        3 => Ok(PageType::Manifest),
        4 => Ok(PageType::Free),
        _ => Err(storage_error("unknown page type tag")),
    }
}

pub(crate) fn validate_v1_fixed_header_layout(header: &PageHeader) -> AndromedaResult<()> {
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

pub(crate) fn header_integrity_crc32(header_bytes: &[u8]) -> u32 {
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
