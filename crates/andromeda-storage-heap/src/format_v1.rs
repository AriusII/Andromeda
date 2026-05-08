use andromeda_core::AndromedaResult;
use andromeda_storage_page::{PageHeader, PageSize, PageTrailer};

use super::SlotEntry;
use super::heap_error;

const PAGE_CODEC_V1_HEADER_LEN: usize = 112;
const PAGE_CODEC_V1_TRAILER_LEN: usize = 48;

pub(crate) const HEAP_PAGE_V1_HEADER_SIZE: usize = PageHeader::MIN_HEADER_LEN_V0 as usize;
pub const HEAP_PAGE_V1_PAYLOAD_OFFSET: usize = PAGE_CODEC_V1_HEADER_LEN;
pub(crate) const HEAP_PAGE_V1_TRAILER_SIZE: usize = 48;
pub(crate) const HEAP_PAGE_V1_SLOT_METADATA_SIZE: usize = 4;
const HEAP_PAGE_V1_LEGACY_HEADER_SLOT_COUNT_OFFSET: usize = 40;
const DISK_PAGE_STORE_HEADER_LEN_OFFSET: usize = 64;
const DISK_PAGE_STORE_PAYLOAD_OFFSET_OFFSET: usize = 66;
const DISK_PAGE_STORE_SLOT_COUNT_OFFSET: usize = 86;
const PAGE_CODEC_V1_HEADER_LEN_OFFSET: usize = 68;
const PAGE_CODEC_V1_PAYLOAD_OFFSET_OFFSET: usize = 72;
const PAGE_CODEC_V1_SLOT_COUNT_OFFSET: usize = 92;
pub(crate) const HEAP_PAGE_V1_SLOT_FLAGS_KNOWN_MASK: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeapPageV1SlotMetadata {
    pub(crate) slot_count: usize,
    pub(crate) free_offset: u16,
    pub(crate) metadata_offset: usize,
    pub(crate) slot_base: usize,
}

pub(crate) fn heap_page_v1_max_slots(page_size: PageSize) -> usize {
    match page_size {
        PageSize::KiB16 => 256,
        PageSize::KiB32 => 512,
    }
}

pub(crate) fn heap_page_v1_metadata_offset(page_size: PageSize) -> usize {
    page_size.bytes_usize() - HEAP_PAGE_V1_TRAILER_SIZE - HEAP_PAGE_V1_SLOT_METADATA_SIZE
}

pub(crate) fn heap_page_v1_validate_format_guard() -> AndromedaResult<()> {
    if HEAP_PAGE_V1_HEADER_SIZE != PageHeader::MIN_HEADER_LEN_V0 as usize {
        return Err(heap_error(
            "heap page v1 header size must match PageHeader V0 minimum",
        ));
    }
    if HEAP_PAGE_V1_PAYLOAD_OFFSET < HEAP_PAGE_V1_HEADER_SIZE {
        return Err(heap_error(
            "heap page v1 payload offset must not overlap PageHeader",
        ));
    }
    if HEAP_PAGE_V1_PAYLOAD_OFFSET < PAGE_CODEC_V1_HEADER_LEN {
        return Err(heap_error(
            "heap page v1 payload offset must reserve PageCodecV1 header bytes",
        ));
    }
    if HEAP_PAGE_V1_TRAILER_SIZE != PageTrailer::V0_LEN as usize
        || HEAP_PAGE_V1_TRAILER_SIZE != PAGE_CODEC_V1_TRAILER_LEN
    {
        return Err(heap_error(
            "heap page v1 trailer size must match DEC-032 page trailer size",
        ));
    }
    for page_size in [PageSize::KiB16, PageSize::KiB32] {
        if heap_page_v1_metadata_offset(page_size) <= HEAP_PAGE_V1_PAYLOAD_OFFSET {
            return Err(heap_error("heap page v1 metadata overlaps payload offset"));
        }
    }
    Ok(())
}

pub(crate) fn heap_page_v1_read_slot_metadata(
    page_size: PageSize,
    page_data: &[u8],
) -> AndromedaResult<HeapPageV1SlotMetadata> {
    heap_page_v1_validate_format_guard()?;

    if page_data.len() != page_size.bytes_usize() {
        return Err(heap_error(format!(
            "page size mismatch: expected {} bytes, got {}",
            page_size.bytes_usize(),
            page_data.len()
        )));
    }

    let metadata_offset = heap_page_v1_metadata_offset(page_size);
    if metadata_offset < HEAP_PAGE_V1_HEADER_SIZE {
        return Err(heap_error("page too small for heap page v1 metadata"));
    }

    let footer_slot_count = read_u16(page_data, metadata_offset)? as usize;
    let free_offset = read_u16(page_data, metadata_offset + 2)?;
    let max_slots = heap_page_v1_max_slots(page_size);
    if footer_slot_count > max_slots {
        return Err(heap_error(format!(
            "heap page v1 slot count {} exceeds limit {}",
            footer_slot_count, max_slots
        )));
    }

    validate_redundant_header_slot_count(page_data, footer_slot_count)?;

    let slot_directory_size = footer_slot_count
        .checked_mul(SlotEntry::SIZE)
        .ok_or_else(|| heap_error("heap page v1 slot directory size overflow"))?;
    if slot_directory_size > metadata_offset.saturating_sub(HEAP_PAGE_V1_PAYLOAD_OFFSET) {
        return Err(heap_error("slot directory overlaps heap payload offset"));
    }

    let slot_base = metadata_offset - slot_directory_size;
    let free_offset_usize = free_offset as usize;
    if footer_slot_count > 0 && free_offset == 0 {
        return Err(heap_error(
            "heap page v1 free offset missing for non-empty slot directory",
        ));
    }
    if free_offset != 0
        && (free_offset_usize < HEAP_PAGE_V1_PAYLOAD_OFFSET || free_offset_usize > slot_base)
    {
        return Err(heap_error(format!(
            "heap page v1 free offset {} outside payload/free-space bounds {}..={}",
            free_offset, HEAP_PAGE_V1_PAYLOAD_OFFSET, slot_base
        )));
    }

    Ok(HeapPageV1SlotMetadata {
        slot_count: footer_slot_count,
        free_offset,
        metadata_offset,
        slot_base,
    })
}

fn validate_redundant_header_slot_count(
    page_data: &[u8],
    footer_slot_count: usize,
) -> AndromedaResult<()> {
    match read_u32(page_data, 0)? {
        0 => validate_optional_legacy_header_slot_count(page_data, footer_slot_count),
        PageHeader::MAGIC => {
            validate_persisted_header_version(page_data)?;
            let header_slot_count =
                read_persisted_header_slot_count(page_data)?.ok_or_else(|| {
                    heap_error("heap page v1 persisted header layout is not recognized")
                })?;
            if header_slot_count != footer_slot_count {
                return Err(heap_error(format!(
                    "heap page v1 slot count ambiguity: persisted header slot count {} does not match footer slot count {}",
                    header_slot_count, footer_slot_count
                )));
            }
            Ok(())
        }
        _ => Err(heap_error("heap page v1 persisted header magic is invalid")),
    }
}

fn validate_persisted_header_version(page_data: &[u8]) -> AndromedaResult<()> {
    let format_version = read_u16(page_data, 4)?;
    if format_version != PageHeader::FORMAT_VERSION_V0 {
        return Err(heap_error(
            "heap page v1 persisted header format version is unsupported",
        ));
    }
    Ok(())
}

fn validate_optional_legacy_header_slot_count(
    page_data: &[u8],
    footer_slot_count: usize,
) -> AndromedaResult<()> {
    let legacy_header_slot_count =
        read_u16(page_data, HEAP_PAGE_V1_LEGACY_HEADER_SLOT_COUNT_OFFSET)? as usize;
    if legacy_header_slot_count != 0 && legacy_header_slot_count != footer_slot_count {
        return Err(heap_error(format!(
            "heap page v1 slot count ambiguity: legacy header slot count {} does not match footer slot count {}",
            legacy_header_slot_count, footer_slot_count
        )));
    }
    Ok(())
}

fn read_persisted_header_slot_count(page_data: &[u8]) -> AndromedaResult<Option<usize>> {
    let codec_header_len = read_u16(page_data, PAGE_CODEC_V1_HEADER_LEN_OFFSET)? as usize;
    let codec_payload_offset = read_u32(page_data, PAGE_CODEC_V1_PAYLOAD_OFFSET_OFFSET)? as usize;
    if codec_header_len == PAGE_CODEC_V1_HEADER_LEN {
        if codec_payload_offset != PAGE_CODEC_V1_HEADER_LEN {
            return Err(heap_error(
                "heap page v1 PageCodecV1 payload offset does not match fixed header length",
            ));
        }
        return Ok(Some(
            read_u16(page_data, PAGE_CODEC_V1_SLOT_COUNT_OFFSET)? as usize
        ));
    }
    if codec_payload_offset == PAGE_CODEC_V1_HEADER_LEN {
        return Err(heap_error(
            "heap page v1 PageCodecV1 header length does not match fixed header length",
        ));
    }

    let disk_header_len = read_u16(page_data, DISK_PAGE_STORE_HEADER_LEN_OFFSET)? as usize;
    let disk_payload_offset = read_u32(page_data, DISK_PAGE_STORE_PAYLOAD_OFFSET_OFFSET)? as usize;
    if disk_header_len == HEAP_PAGE_V1_PAYLOAD_OFFSET
        && disk_payload_offset == HEAP_PAGE_V1_PAYLOAD_OFFSET
    {
        return Ok(Some(
            read_u16(page_data, DISK_PAGE_STORE_SLOT_COUNT_OFFSET)? as usize,
        ));
    }
    if disk_header_len != 0 || disk_payload_offset != 0 {
        return Err(heap_error(
            "heap page v1 persisted disk header length fields are invalid",
        ));
    }

    Ok(None)
}

fn read_u16(page_data: &[u8], offset: usize) -> AndromedaResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| heap_error("heap page v1 u16 field offset overflow"))?;
    let mut bytes = [0; 2];
    bytes.copy_from_slice(
        page_data
            .get(offset..end)
            .ok_or_else(|| heap_error("heap page v1 u16 field is truncated"))?,
    );
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(page_data: &[u8], offset: usize) -> AndromedaResult<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| heap_error("heap page v1 u32 field offset overflow"))?;
    let mut bytes = [0; 4];
    bytes.copy_from_slice(
        page_data
            .get(offset..end)
            .ok_or_else(|| heap_error("heap page v1 u32 field is truncated"))?,
    );
    Ok(u32::from_le_bytes(bytes))
}
