use crate::{PageSize, SegmentState};

use super::super::{
    digest::{crc64_ecma, sha256},
    error::{SegmentIndexError, SegmentIndexResult},
    types::{SegmentIndexEntryV0, SegmentIndexHeaderV0, SegmentIndexV0},
};
use super::{
    EXTENSION_REQUIRED_FLAG, KNOWN_FLAGS, PAGE_SIZE_POLICY_16K, PAGE_SIZE_POLICY_32K,
    PAGE_SIZE_POLICY_MIXED, SEGMENT_INDEX_V0_BYTE_ORDER, SEGMENT_INDEX_V0_ENTRY_LEN,
    SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES, SEGMENT_INDEX_V0_FLAG_PUBLISHED_COLD_ONLY,
    SEGMENT_INDEX_V0_FORMAT_MAJOR, SEGMENT_INDEX_V0_FORMAT_MINOR, SEGMENT_INDEX_V0_HEADER_LEN,
    SEGMENT_INDEX_V0_MAGIC, SEGMENT_INDEX_V0_MAX_ENTRIES, SEGMENT_INDEX_V0_MAX_EXTENSION_BYTES,
    SEGMENT_INDEX_V0_TRAILER_LEN,
    binary::{read_u16, read_u32},
    encode::encode_entries,
    integrity::{entry_crc32, file_sha256, header_crc32, root_hash, trailer_crc32},
    policy::{computed_flags, summarize_entries, validate_page_size_policy},
};

pub(in crate::segment_index) fn validate_index_contract(
    index: &SegmentIndexV0,
) -> SegmentIndexResult<()> {
    validate_header_before_allocate(&index.header, index.header.total_len as usize)?;
    validate_extension_bytes(&index.extension_bytes)?;
    validate_entries_for_header(
        index.header.snapshot_id,
        &index.entries,
        Some(&index.header),
    )?;
    validate_header_summary(index)?;
    validate_trailer(index)?;
    Ok(())
}

pub(super) fn validate_header_before_allocate(
    header: &SegmentIndexHeaderV0,
    input_len: usize,
) -> SegmentIndexResult<()> {
    if header.magic != SEGMENT_INDEX_V0_MAGIC {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "magic mismatch",
        });
    }
    if header.format_major != SEGMENT_INDEX_V0_FORMAT_MAJOR
        || header.format_minor != SEGMENT_INDEX_V0_FORMAT_MINOR
    {
        return Err(SegmentIndexError::UnsupportedVersion {
            major: header.format_major,
            minor: header.format_minor,
        });
    }
    if header.byte_order != SEGMENT_INDEX_V0_BYTE_ORDER {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "byte order marker mismatch",
        });
    }
    if header.header_len != SEGMENT_INDEX_V0_HEADER_LEN as u16 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "header length mismatch",
        });
    }
    if header.header_crc32 == 0 || header_crc32(header) != header.header_crc32 {
        return Err(SegmentIndexError::HeaderChecksumMismatch);
    }
    if header.flags & !KNOWN_FLAGS != 0 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "unknown flag bits are set",
        });
    }
    if header.flags & SEGMENT_INDEX_V0_FLAG_PUBLISHED_COLD_ONLY == 0 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "published-cold-only flag must be set",
        });
    }
    if header.database_id == 0
        || header.snapshot_id == 0
        || header.segment_index_id == 0
        || header.manifest_version == 0
    {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "identity fields must not be zero",
        });
    }
    if header.base_checkpoint_lsn.is_zero() || header.required_wal_start_lsn.is_zero() {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "LSN fields must not be zero",
        });
    }
    if header.entry_offset != SEGMENT_INDEX_V0_HEADER_LEN as u64 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "entry offset mismatch",
        });
    }
    if header.entry_count == 0 || header.entry_count > SEGMENT_INDEX_V0_MAX_ENTRIES {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "entry count is empty or exceeds the v0 bound",
        });
    }
    if header.entry_len != SEGMENT_INDEX_V0_ENTRY_LEN as u32 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "entry length mismatch",
        });
    }
    validate_page_size_policy(header.page_size_policy_tag)?;
    if header.extension_len > SEGMENT_INDEX_V0_MAX_EXTENSION_BYTES {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "extension length exceeds the v0 bound",
        });
    }
    if header.parent_manifest_hash == [0; 32] || header.entry_table_sha256 == [0; 32] {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "required SHA-256 fields must not be zero",
        });
    }
    if header.entry_table_crc64 == 0 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "entry table CRC64 must not be zero",
        });
    }
    let expected_extension_offset = header
        .entry_offset
        .checked_add(
            header
                .entry_count
                .checked_mul(u64::from(header.entry_len))
                .ok_or(SegmentIndexError::LengthOverflow {
                    field: "entry table length",
                })?,
        )
        .ok_or(SegmentIndexError::LengthOverflow {
            field: "extension_offset",
        })?;
    if header.extension_offset != expected_extension_offset {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "extension offset must follow the entry table",
        });
    }
    let expected_total_len = header
        .extension_offset
        .checked_add(header.extension_len)
        .and_then(|value| value.checked_add(SEGMENT_INDEX_V0_TRAILER_LEN as u64))
        .ok_or(SegmentIndexError::LengthOverflow { field: "total_len" })?;
    if header.total_len != expected_total_len {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "total length does not match fixed ranges",
        });
    }
    if usize::try_from(header.total_len).ok() != Some(input_len) {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "total length does not match input length",
        });
    }
    Ok(())
}

pub(super) fn validate_entries_for_header(
    snapshot_id: u64,
    entries: &[SegmentIndexEntryV0],
    header: Option<&SegmentIndexHeaderV0>,
) -> SegmentIndexResult<()> {
    if entries.is_empty() {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "entry count must not be zero",
        });
    }

    let mut previous: Option<&SegmentIndexEntryV0> = None;
    for (index, entry) in entries.iter().enumerate() {
        validate_entry_semantics(index, entry)?;
        if entry.snapshot_id != snapshot_id {
            return Err(SegmentIndexError::InvalidEntry {
                index,
                reason: "entry snapshot id must match header snapshot id",
            });
        }
        if entry_crc32(entry) != entry.entry_crc32 {
            return Err(SegmentIndexError::EntryChecksumMismatch { index });
        }
        if let Some(header) = header {
            validate_entry_against_header_policy(index, entry, header)?;
        }
        if let Some(previous) = previous {
            validate_monotonic_order(index, previous, entry, header)?;
        }
        previous = Some(entry);
    }
    Ok(())
}

pub(super) fn validate_entry_semantics(
    index: usize,
    entry: &SegmentIndexEntryV0,
) -> SegmentIndexResult<()> {
    if entry.segment_id.is_zero()
        || entry.object_id.is_zero()
        || entry.allocation_id.is_zero()
        || entry.first_extent_id.is_zero()
        || entry.first_page_id.is_zero()
    {
        return Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "identity fields must not be zero",
        });
    }
    if entry.extent_count == 0 || entry.page_count == 0 {
        return Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "extent and page counts must not be zero",
        });
    }
    entry
        .first_extent_id
        .get()
        .checked_add(u64::from(entry.extent_count - 1))
        .ok_or(SegmentIndexError::InvalidEntry {
            index,
            reason: "extent range overflows u64",
        })?;
    entry
        .first_page_id
        .get()
        .checked_add(u64::from(entry.page_count - 1))
        .ok_or(SegmentIndexError::InvalidEntry {
            index,
            reason: "page range overflows u64",
        })?;
    if entry.segment_state != SegmentState::PublishedCold {
        return Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "segment state must be PublishedCold",
        });
    }
    if entry.min_page_lsn.is_zero()
        || entry.max_page_lsn.is_zero()
        || entry.max_page_lsn < entry.min_page_lsn
    {
        return Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "page LSN bounds are invalid",
        });
    }
    if entry.snapshot_id == 0 || entry.segment_file_id == 0 {
        return Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "snapshot and segment file ids must not be zero",
        });
    }
    if entry.segment_byte_len == 0
        || entry
            .segment_file_offset
            .checked_add(entry.segment_byte_len)
            .is_none()
    {
        return Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "segment byte range is invalid",
        });
    }
    if entry.segment_payload_crc64 == 0
        || entry.segment_sha256 == [0; 32]
        || entry.segment_header_crc32 == 0
        || entry.segment_trailer_crc32 == 0
    {
        return Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "segment checksum and hash evidence must not be zero",
        });
    }
    if entry.entry_flags != 0 {
        return Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "entry flags must be zero in v0",
        });
    }
    Ok(())
}

fn validate_entry_against_header_policy(
    index: usize,
    entry: &SegmentIndexEntryV0,
    header: &SegmentIndexHeaderV0,
) -> SegmentIndexResult<()> {
    match (header.page_size_policy_tag, entry.page_size) {
        (PAGE_SIZE_POLICY_MIXED, _)
        | (PAGE_SIZE_POLICY_16K, PageSize::KiB16)
        | (PAGE_SIZE_POLICY_32K, PageSize::KiB32) => Ok(()),
        _ => Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "entry page size conflicts with header page size policy",
        }),
    }
}

fn validate_monotonic_order(
    index: usize,
    previous: &SegmentIndexEntryV0,
    current: &SegmentIndexEntryV0,
    header: Option<&SegmentIndexHeaderV0>,
) -> SegmentIndexResult<()> {
    if current.segment_id <= previous.segment_id {
        return Err(SegmentIndexError::InvalidOrdering {
            index,
            reason: "segment ids must be strictly increasing",
        });
    }

    let previous_last_page_id = previous.last_page_id()?.get();
    let current_first_page_id = current.first_page_id.get();
    if current_first_page_id <= previous_last_page_id {
        return Err(SegmentIndexError::InvalidOrdering {
            index,
            reason: "page ranges must be strictly increasing and non-overlapping",
        });
    }
    if header
        .map(|header| header.flags & SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES != 0)
        .unwrap_or(false)
        && current_first_page_id != previous_last_page_id + 1
    {
        return Err(SegmentIndexError::InvalidOrdering {
            index,
            reason: "contiguous page range flag requires gap-free entries",
        });
    }
    Ok(())
}

fn validate_header_summary(index: &SegmentIndexV0) -> SegmentIndexResult<()> {
    let summary = summarize_entries(&index.entries)?;
    let header = &index.header;
    if header.first_segment_id != summary.first_segment_id
        || header.last_segment_id != summary.last_segment_id
        || header.first_page_id != summary.first_page_id
        || header.last_page_id != summary.last_page_id
        || header.min_page_lsn != summary.min_page_lsn
        || header.max_page_lsn != summary.max_page_lsn
        || header.total_page_count != summary.total_page_count
    {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "header range summary does not match entries",
        });
    }
    if header.entry_count != index.entries.len() as u64 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "entry count does not match entries",
        });
    }
    let expected_contiguous =
        computed_flags(&index.entries) & SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES;
    if header.flags & SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES != expected_contiguous {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "contiguous page range flag does not match entries",
        });
    }
    if header.extension_len != index.extension_bytes.len() as u64 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "extension length does not match extension bytes",
        });
    }
    let entry_table = encode_entries(&index.entries);
    if sha256(&entry_table) != header.entry_table_sha256 {
        return Err(SegmentIndexError::EntryTableDigestMismatch);
    }
    if crc64_ecma(&entry_table) != header.entry_table_crc64 {
        return Err(SegmentIndexError::EntryTableChecksumMismatch);
    }
    Ok(())
}

fn validate_trailer(index: &SegmentIndexV0) -> SegmentIndexResult<()> {
    let trailer = &index.trailer;
    if trailer.entry_table_crc64_mirror == 0
        || trailer.segment_index_file_sha256 == [0; 32]
        || trailer.segment_index_root_hash == [0; 32]
        || trailer.trailer_crc32 == 0
    {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "trailer integrity fields must not be zero",
        });
    }
    if trailer.entry_table_crc64_mirror != index.header.entry_table_crc64 {
        return Err(SegmentIndexError::EntryTableChecksumMismatch);
    }
    if trailer.trailer_flags != 0 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "trailer flags must be zero in v0",
        });
    }
    if trailer_crc32(trailer) != trailer.trailer_crc32 {
        return Err(SegmentIndexError::TrailerChecksumMismatch);
    }
    if file_sha256(index, trailer) != trailer.segment_index_file_sha256 {
        return Err(SegmentIndexError::FileDigestMismatch);
    }
    if root_hash(&index.header, trailer.segment_index_file_sha256)
        != trailer.segment_index_root_hash
    {
        return Err(SegmentIndexError::RootDigestMismatch);
    }
    Ok(())
}

pub(super) fn validate_extension_bytes(extension_bytes: &[u8]) -> SegmentIndexResult<()> {
    if extension_bytes.len() as u64 > SEGMENT_INDEX_V0_MAX_EXTENSION_BYTES {
        return Err(SegmentIndexError::InvalidExtension {
            reason: "extension length exceeds the v0 bound",
        });
    }

    let mut offset = 0;
    while offset < extension_bytes.len() {
        let header_end = offset
            .checked_add(8)
            .ok_or(SegmentIndexError::LengthOverflow {
                field: "extension record header",
            })?;
        if header_end > extension_bytes.len() {
            return Err(SegmentIndexError::InvalidExtension {
                reason: "extension record header is truncated",
            });
        }
        let flags = read_u16(extension_bytes, offset + 2, "extension_flags")?;
        if flags & EXTENSION_REQUIRED_FLAG != 0 {
            return Err(SegmentIndexError::InvalidExtension {
                reason: "unknown required extension record",
            });
        }
        let payload_len = usize::try_from(read_u32(
            extension_bytes,
            offset + 4,
            "extension_payload_len",
        )?)
        .map_err(|_| SegmentIndexError::LengthOverflow {
            field: "extension payload length",
        })?;
        offset = header_end
            .checked_add(payload_len)
            .ok_or(SegmentIndexError::LengthOverflow {
                field: "extension record end",
            })?;
        if offset > extension_bytes.len() {
            return Err(SegmentIndexError::InvalidExtension {
                reason: "extension record payload is truncated",
            });
        }
    }
    Ok(())
}
