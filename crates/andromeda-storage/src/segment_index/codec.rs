use crate::{AllocationId, ExtentId, Lsn, ObjectId, PageId, PageSize, SegmentId, SegmentState};

use super::{
    digest::{crc32_iso_hdlc, crc64_ecma, sha256},
    error::{SegmentIndexError, SegmentIndexResult},
    types::{
        SegmentIndexBuildContextV0, SegmentIndexEntryV0, SegmentIndexHeaderV0,
        SegmentIndexTrailerV0, SegmentIndexV0,
    },
};

pub const SEGMENT_INDEX_V0_MAGIC: [u8; 8] = *b"ANDSGIX0";
pub const SEGMENT_INDEX_V0_FORMAT_MAJOR: u16 = 1;
pub const SEGMENT_INDEX_V0_FORMAT_MINOR: u16 = 0;
pub const SEGMENT_INDEX_V0_BYTE_ORDER: u16 = 0x0102;
pub const SEGMENT_INDEX_V0_HEADER_LEN: usize = 256;
pub const SEGMENT_INDEX_V0_ENTRY_LEN: usize = 160;
pub const SEGMENT_INDEX_V0_TRAILER_LEN: usize = 96;
pub const SEGMENT_INDEX_V0_MAX_ENTRIES: u64 = 16_777_216;
pub const SEGMENT_INDEX_V0_MAX_EXTENSION_BYTES: u64 = 1024 * 1024;

pub const SEGMENT_INDEX_V0_FLAG_PUBLISHED_COLD_ONLY: u32 = 0x0000_0001;
pub const SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES: u32 = 0x0000_0002;
pub const SEGMENT_INDEX_V0_FLAG_FORENSIC_HOLD: u32 = 0x0000_0008;

const KNOWN_FLAGS: u32 = SEGMENT_INDEX_V0_FLAG_PUBLISHED_COLD_ONLY
    | SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES
    | SEGMENT_INDEX_V0_FLAG_FORENSIC_HOLD;

const PAGE_SIZE_POLICY_MIXED: u32 = 0;
const PAGE_SIZE_POLICY_16K: u32 = 1;
const PAGE_SIZE_POLICY_32K: u32 = 2;

const PAGE_SIZE_TAG_16K: u16 = 1;
const PAGE_SIZE_TAG_32K: u16 = 2;
const SEGMENT_STATE_TAG_BUILDING_HOT_SNAPSHOT: u16 = 1;
const SEGMENT_STATE_TAG_SEALED: u16 = 2;
const SEGMENT_STATE_TAG_PUBLISHED_COLD: u16 = 3;
const EXTENSION_REQUIRED_FLAG: u16 = 0x0001;

const HEADER_TOTAL_LEN_OFFSET: usize = 16;
const HEADER_CRC_OFFSET: usize = 24;
const HEADER_FLAGS_OFFSET: usize = 28;
const HEADER_DATABASE_ID_OFFSET: usize = 32;
const HEADER_SNAPSHOT_ID_OFFSET: usize = 40;
const HEADER_SEGMENT_INDEX_ID_OFFSET: usize = 48;
const HEADER_MANIFEST_VERSION_OFFSET: usize = 56;
const HEADER_BASE_CHECKPOINT_LSN_OFFSET: usize = 64;
const HEADER_REQUIRED_WAL_START_LSN_OFFSET: usize = 72;
const HEADER_ENTRY_OFFSET_OFFSET: usize = 80;
const HEADER_ENTRY_COUNT_OFFSET: usize = 88;
const HEADER_ENTRY_LEN_OFFSET: usize = 96;
const HEADER_PAGE_SIZE_POLICY_TAG_OFFSET: usize = 100;
const HEADER_EXTENSION_OFFSET_OFFSET: usize = 104;
const HEADER_EXTENSION_LEN_OFFSET: usize = 112;
const HEADER_FIRST_SEGMENT_ID_OFFSET: usize = 120;
const HEADER_LAST_SEGMENT_ID_OFFSET: usize = 128;
const HEADER_FIRST_PAGE_ID_OFFSET: usize = 136;
const HEADER_LAST_PAGE_ID_OFFSET: usize = 144;
const HEADER_MIN_PAGE_LSN_OFFSET: usize = 152;
const HEADER_MAX_PAGE_LSN_OFFSET: usize = 160;
const HEADER_TOTAL_PAGE_COUNT_OFFSET: usize = 168;
const HEADER_PARENT_MANIFEST_HASH_OFFSET: usize = 176;
const HEADER_ENTRY_TABLE_SHA256_OFFSET: usize = 208;
const HEADER_ENTRY_TABLE_CRC64_OFFSET: usize = 240;
const HEADER_RESERVED_248_OFFSET: usize = 248;

const ENTRY_RESERVED_52_OFFSET: usize = 52;
const ENTRY_CRC_OFFSET: usize = 156;

const TRAILER_FILE_SHA_OFFSET: usize = 8;
const TRAILER_ROOT_HASH_OFFSET: usize = 40;
const TRAILER_CRC_OFFSET: usize = 72;
const TRAILER_FLAGS_OFFSET: usize = 76;
const TRAILER_RESERVED_80_OFFSET: usize = 80;

impl SegmentIndexV0 {
    pub fn encode(&self) -> SegmentIndexResult<Vec<u8>> {
        self.validate()?;
        Ok(encode_file(self, TrailerHashMode::Stored))
    }

    pub fn decode(bytes: &[u8]) -> SegmentIndexResult<Self> {
        if bytes.len() < SEGMENT_INDEX_V0_HEADER_LEN + SEGMENT_INDEX_V0_TRAILER_LEN {
            return Err(SegmentIndexError::Truncated {
                field: "segment index file",
            });
        }

        let header = decode_header(bytes)?;
        validate_header_before_allocate(&header, bytes.len())?;

        let entry_start = usize::try_from(header.entry_offset).map_err(|_| {
            SegmentIndexError::LengthOverflow {
                field: "entry_offset",
            }
        })?;
        let entry_count =
            usize::try_from(header.entry_count).map_err(|_| SegmentIndexError::LengthOverflow {
                field: "entry_count",
            })?;
        let entry_table_len = entry_count.checked_mul(SEGMENT_INDEX_V0_ENTRY_LEN).ok_or(
            SegmentIndexError::LengthOverflow {
                field: "entry table length",
            },
        )?;
        let entry_end =
            entry_start
                .checked_add(entry_table_len)
                .ok_or(SegmentIndexError::LengthOverflow {
                    field: "entry table end",
                })?;

        let mut entries = Vec::with_capacity(entry_count);
        for index in 0..entry_count {
            let offset = entry_start
                .checked_add(index * SEGMENT_INDEX_V0_ENTRY_LEN)
                .ok_or(SegmentIndexError::LengthOverflow {
                    field: "entry offset",
                })?;
            entries.push(decode_entry(
                index,
                bytes
                    .get(offset..offset + SEGMENT_INDEX_V0_ENTRY_LEN)
                    .ok_or(SegmentIndexError::Truncated {
                        field: "segment index entry",
                    })?,
            )?);
        }

        let extension_start = entry_end;
        let extension_len = usize::try_from(header.extension_len).map_err(|_| {
            SegmentIndexError::LengthOverflow {
                field: "extension_len",
            }
        })?;
        let extension_end = extension_start.checked_add(extension_len).ok_or(
            SegmentIndexError::LengthOverflow {
                field: "extension end",
            },
        )?;
        let extension_bytes = bytes
            .get(extension_start..extension_end)
            .ok_or(SegmentIndexError::Truncated {
                field: "segment index extension",
            })?
            .to_vec();

        let trailer = decode_trailer(
            bytes
                .get(extension_end..extension_end + SEGMENT_INDEX_V0_TRAILER_LEN)
                .ok_or(SegmentIndexError::Truncated {
                    field: "segment index trailer",
                })?,
        )?;

        let index = Self {
            header,
            entries,
            extension_bytes,
            trailer,
        };
        index.validate()?;
        Ok(index)
    }
}

pub(super) fn build_entry_with_crc(
    mut entry: SegmentIndexEntryV0,
    index: usize,
) -> SegmentIndexResult<SegmentIndexEntryV0> {
    validate_entry_semantics(index, &entry)?;
    entry.entry_crc32 = 0;
    entry.entry_crc32 = entry_crc32(&entry);
    Ok(entry)
}

pub(super) fn build_header(
    context: &SegmentIndexBuildContextV0,
    entries: &[SegmentIndexEntryV0],
    extension_bytes: &[u8],
) -> SegmentIndexResult<SegmentIndexHeaderV0> {
    validate_extension_bytes(extension_bytes)?;
    validate_entries_for_header(context.snapshot_id, entries, None)?;

    let entry_table = encode_entries(entries);
    let entry_table_len =
        u64::try_from(entry_table.len()).map_err(|_| SegmentIndexError::LengthOverflow {
            field: "entry table length",
        })?;
    let extension_len =
        u64::try_from(extension_bytes.len()).map_err(|_| SegmentIndexError::LengthOverflow {
            field: "extension_len",
        })?;
    let extension_offset = u64::try_from(SEGMENT_INDEX_V0_HEADER_LEN)
        .map_err(|_| SegmentIndexError::LengthOverflow {
            field: "header length",
        })?
        .checked_add(entry_table_len)
        .ok_or(SegmentIndexError::LengthOverflow {
            field: "extension_offset",
        })?;
    let total_len = extension_offset
        .checked_add(extension_len)
        .and_then(|value| value.checked_add(SEGMENT_INDEX_V0_TRAILER_LEN as u64))
        .ok_or(SegmentIndexError::LengthOverflow { field: "total_len" })?;

    let summary = summarize_entries(entries)?;
    let mut header = SegmentIndexHeaderV0 {
        magic: SEGMENT_INDEX_V0_MAGIC,
        format_major: SEGMENT_INDEX_V0_FORMAT_MAJOR,
        format_minor: SEGMENT_INDEX_V0_FORMAT_MINOR,
        byte_order: SEGMENT_INDEX_V0_BYTE_ORDER,
        header_len: SEGMENT_INDEX_V0_HEADER_LEN as u16,
        total_len,
        header_crc32: 0,
        flags: computed_flags(entries),
        database_id: context.database_id,
        snapshot_id: context.snapshot_id,
        segment_index_id: context.segment_index_id,
        manifest_version: context.manifest_version,
        base_checkpoint_lsn: context.base_checkpoint_lsn,
        required_wal_start_lsn: context.required_wal_start_lsn,
        entry_offset: SEGMENT_INDEX_V0_HEADER_LEN as u64,
        entry_count: entries.len() as u64,
        entry_len: SEGMENT_INDEX_V0_ENTRY_LEN as u32,
        page_size_policy_tag: page_size_policy(entries)?,
        extension_offset,
        extension_len,
        first_segment_id: summary.first_segment_id,
        last_segment_id: summary.last_segment_id,
        first_page_id: summary.first_page_id,
        last_page_id: summary.last_page_id,
        min_page_lsn: summary.min_page_lsn,
        max_page_lsn: summary.max_page_lsn,
        total_page_count: summary.total_page_count,
        parent_manifest_hash: context.parent_manifest_hash,
        entry_table_sha256: sha256(&entry_table),
        entry_table_crc64: crc64_ecma(&entry_table),
    };
    header.header_crc32 = header_crc32(&header);
    Ok(header)
}

pub(super) fn build_trailer(index: &SegmentIndexV0) -> SegmentIndexResult<SegmentIndexTrailerV0> {
    let mut trailer = SegmentIndexTrailerV0 {
        entry_table_crc64_mirror: index.header.entry_table_crc64,
        segment_index_file_sha256: [0; 32],
        segment_index_root_hash: [0; 32],
        trailer_crc32: 0,
        trailer_flags: 0,
    };
    trailer.segment_index_file_sha256 = file_sha256(index, &trailer);
    trailer.segment_index_root_hash = root_hash(&index.header, trailer.segment_index_file_sha256);
    trailer.trailer_crc32 = trailer_crc32(&trailer);
    Ok(trailer)
}

pub(super) fn validate_index_contract(index: &SegmentIndexV0) -> SegmentIndexResult<()> {
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

fn validate_header_before_allocate(
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

fn validate_entries_for_header(
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

fn validate_entry_semantics(index: usize, entry: &SegmentIndexEntryV0) -> SegmentIndexResult<()> {
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

fn validate_extension_bytes(extension_bytes: &[u8]) -> SegmentIndexResult<()> {
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

fn decode_header(bytes: &[u8]) -> SegmentIndexResult<SegmentIndexHeaderV0> {
    let header_bytes =
        bytes
            .get(..SEGMENT_INDEX_V0_HEADER_LEN)
            .ok_or(SegmentIndexError::Truncated {
                field: "segment index header",
            })?;
    let mut magic = [0u8; 8];
    magic.copy_from_slice(&header_bytes[0..8]);
    if read_u64(header_bytes, HEADER_RESERVED_248_OFFSET, "reserved_248")? != 0 {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "reserved header bytes must be zero",
        });
    }
    Ok(SegmentIndexHeaderV0 {
        magic,
        format_major: read_u16(header_bytes, 8, "format_major")?,
        format_minor: read_u16(header_bytes, 10, "format_minor")?,
        byte_order: read_u16(header_bytes, 12, "byte_order")?,
        header_len: read_u16(header_bytes, 14, "header_len")?,
        total_len: read_u64(header_bytes, HEADER_TOTAL_LEN_OFFSET, "total_len")?,
        header_crc32: read_u32(header_bytes, HEADER_CRC_OFFSET, "header_crc32")?,
        flags: read_u32(header_bytes, HEADER_FLAGS_OFFSET, "flags")?,
        database_id: read_u64(header_bytes, HEADER_DATABASE_ID_OFFSET, "database_id")?,
        snapshot_id: read_u64(header_bytes, HEADER_SNAPSHOT_ID_OFFSET, "snapshot_id")?,
        segment_index_id: read_u64(
            header_bytes,
            HEADER_SEGMENT_INDEX_ID_OFFSET,
            "segment_index_id",
        )?,
        manifest_version: read_u64(
            header_bytes,
            HEADER_MANIFEST_VERSION_OFFSET,
            "manifest_version",
        )?,
        base_checkpoint_lsn: Lsn::new(read_u64(
            header_bytes,
            HEADER_BASE_CHECKPOINT_LSN_OFFSET,
            "base_checkpoint_lsn",
        )?),
        required_wal_start_lsn: Lsn::new(read_u64(
            header_bytes,
            HEADER_REQUIRED_WAL_START_LSN_OFFSET,
            "required_wal_start_lsn",
        )?),
        entry_offset: read_u64(header_bytes, HEADER_ENTRY_OFFSET_OFFSET, "entry_offset")?,
        entry_count: read_u64(header_bytes, HEADER_ENTRY_COUNT_OFFSET, "entry_count")?,
        entry_len: read_u32(header_bytes, HEADER_ENTRY_LEN_OFFSET, "entry_len")?,
        page_size_policy_tag: read_u32(
            header_bytes,
            HEADER_PAGE_SIZE_POLICY_TAG_OFFSET,
            "page_size_policy_tag",
        )?,
        extension_offset: read_u64(
            header_bytes,
            HEADER_EXTENSION_OFFSET_OFFSET,
            "extension_offset",
        )?,
        extension_len: read_u64(header_bytes, HEADER_EXTENSION_LEN_OFFSET, "extension_len")?,
        first_segment_id: SegmentId::new(read_u64(
            header_bytes,
            HEADER_FIRST_SEGMENT_ID_OFFSET,
            "first_segment_id",
        )?),
        last_segment_id: SegmentId::new(read_u64(
            header_bytes,
            HEADER_LAST_SEGMENT_ID_OFFSET,
            "last_segment_id",
        )?),
        first_page_id: PageId::new(read_u64(
            header_bytes,
            HEADER_FIRST_PAGE_ID_OFFSET,
            "first_page_id",
        )?),
        last_page_id: PageId::new(read_u64(
            header_bytes,
            HEADER_LAST_PAGE_ID_OFFSET,
            "last_page_id",
        )?),
        min_page_lsn: Lsn::new(read_u64(
            header_bytes,
            HEADER_MIN_PAGE_LSN_OFFSET,
            "min_page_lsn",
        )?),
        max_page_lsn: Lsn::new(read_u64(
            header_bytes,
            HEADER_MAX_PAGE_LSN_OFFSET,
            "max_page_lsn",
        )?),
        total_page_count: read_u64(
            header_bytes,
            HEADER_TOTAL_PAGE_COUNT_OFFSET,
            "total_page_count",
        )?,
        parent_manifest_hash: read_hash(
            header_bytes,
            HEADER_PARENT_MANIFEST_HASH_OFFSET,
            "parent_manifest_hash",
        )?,
        entry_table_sha256: read_hash(
            header_bytes,
            HEADER_ENTRY_TABLE_SHA256_OFFSET,
            "entry_table_sha256",
        )?,
        entry_table_crc64: read_u64(
            header_bytes,
            HEADER_ENTRY_TABLE_CRC64_OFFSET,
            "entry_table_crc64",
        )?,
    })
}

fn decode_entry(index: usize, bytes: &[u8]) -> SegmentIndexResult<SegmentIndexEntryV0> {
    if read_u32(bytes, ENTRY_RESERVED_52_OFFSET, "reserved_52")? != 0 {
        return Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "reserved entry field must be zero",
        });
    }
    let page_size = decode_page_size_tag(read_u16(bytes, 36, "page_size_tag")?, index)?;
    let segment_state = decode_segment_state_tag(read_u16(bytes, 38, "segment_state_tag")?, index)?;
    let entry = SegmentIndexEntryV0 {
        segment_id: SegmentId::new(read_u64(bytes, 0, "segment_id")?),
        object_id: ObjectId::new(read_u64(bytes, 8, "object_id")?),
        allocation_id: AllocationId::new(read_u64(bytes, 16, "allocation_id")?),
        first_extent_id: ExtentId::new(read_u64(bytes, 24, "first_extent_id")?),
        extent_count: read_u32(bytes, 32, "extent_count")?,
        page_size,
        segment_state,
        first_page_id: PageId::new(read_u64(bytes, 40, "first_page_id")?),
        page_count: read_u32(bytes, 48, "page_count")?,
        min_page_lsn: Lsn::new(read_u64(bytes, 56, "min_page_lsn")?),
        max_page_lsn: Lsn::new(read_u64(bytes, 64, "max_page_lsn")?),
        snapshot_id: read_u64(bytes, 72, "snapshot_id")?,
        segment_file_id: read_u64(bytes, 80, "segment_file_id")?,
        segment_file_offset: read_u64(bytes, 88, "segment_file_offset")?,
        segment_byte_len: read_u64(bytes, 96, "segment_byte_len")?,
        segment_payload_crc64: read_u64(bytes, 104, "segment_payload_crc64")?,
        segment_sha256: read_hash(bytes, 112, "segment_sha256")?,
        segment_header_crc32: read_u32(bytes, 144, "segment_header_crc32")?,
        segment_trailer_crc32: read_u32(bytes, 148, "segment_trailer_crc32")?,
        entry_flags: read_u32(bytes, 152, "entry_flags")?,
        entry_crc32: read_u32(bytes, ENTRY_CRC_OFFSET, "entry_crc32")?,
    };
    validate_entry_semantics(index, &entry)?;
    if entry_crc32(&entry) != entry.entry_crc32 {
        return Err(SegmentIndexError::EntryChecksumMismatch { index });
    }
    Ok(entry)
}

fn decode_trailer(bytes: &[u8]) -> SegmentIndexResult<SegmentIndexTrailerV0> {
    if bytes.len() != SEGMENT_INDEX_V0_TRAILER_LEN {
        return Err(SegmentIndexError::Truncated {
            field: "segment index trailer",
        });
    }
    if bytes[TRAILER_RESERVED_80_OFFSET..TRAILER_RESERVED_80_OFFSET + 16]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(SegmentIndexError::InvalidHeader {
            reason: "trailer reserved bytes must be zero",
        });
    }
    Ok(SegmentIndexTrailerV0 {
        entry_table_crc64_mirror: read_u64(bytes, 0, "entry_table_crc64_mirror")?,
        segment_index_file_sha256: read_hash(
            bytes,
            TRAILER_FILE_SHA_OFFSET,
            "segment_index_file_sha256",
        )?,
        segment_index_root_hash: read_hash(
            bytes,
            TRAILER_ROOT_HASH_OFFSET,
            "segment_index_root_hash",
        )?,
        trailer_crc32: read_u32(bytes, TRAILER_CRC_OFFSET, "trailer_crc32")?,
        trailer_flags: read_u32(bytes, TRAILER_FLAGS_OFFSET, "trailer_flags")?,
    })
}

fn encode_file(index: &SegmentIndexV0, trailer_mode: TrailerHashMode) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(index.header.total_len as usize);
    bytes.extend_from_slice(&encode_header(&index.header));
    bytes.extend_from_slice(&encode_entries(&index.entries));
    bytes.extend_from_slice(&index.extension_bytes);
    bytes.extend_from_slice(&encode_trailer(&index.trailer, trailer_mode));
    bytes
}

fn encode_header(header: &SegmentIndexHeaderV0) -> [u8; SEGMENT_INDEX_V0_HEADER_LEN] {
    let mut bytes = [0u8; SEGMENT_INDEX_V0_HEADER_LEN];
    bytes[0..8].copy_from_slice(&header.magic);
    write_u16(&mut bytes, 8, header.format_major);
    write_u16(&mut bytes, 10, header.format_minor);
    write_u16(&mut bytes, 12, header.byte_order);
    write_u16(&mut bytes, 14, header.header_len);
    write_u64(&mut bytes, HEADER_TOTAL_LEN_OFFSET, header.total_len);
    write_u32(&mut bytes, HEADER_CRC_OFFSET, header.header_crc32);
    write_u32(&mut bytes, HEADER_FLAGS_OFFSET, header.flags);
    write_u64(&mut bytes, HEADER_DATABASE_ID_OFFSET, header.database_id);
    write_u64(&mut bytes, HEADER_SNAPSHOT_ID_OFFSET, header.snapshot_id);
    write_u64(
        &mut bytes,
        HEADER_SEGMENT_INDEX_ID_OFFSET,
        header.segment_index_id,
    );
    write_u64(
        &mut bytes,
        HEADER_MANIFEST_VERSION_OFFSET,
        header.manifest_version,
    );
    write_u64(
        &mut bytes,
        HEADER_BASE_CHECKPOINT_LSN_OFFSET,
        header.base_checkpoint_lsn.get(),
    );
    write_u64(
        &mut bytes,
        HEADER_REQUIRED_WAL_START_LSN_OFFSET,
        header.required_wal_start_lsn.get(),
    );
    write_u64(&mut bytes, HEADER_ENTRY_OFFSET_OFFSET, header.entry_offset);
    write_u64(&mut bytes, HEADER_ENTRY_COUNT_OFFSET, header.entry_count);
    write_u32(&mut bytes, HEADER_ENTRY_LEN_OFFSET, header.entry_len);
    write_u32(
        &mut bytes,
        HEADER_PAGE_SIZE_POLICY_TAG_OFFSET,
        header.page_size_policy_tag,
    );
    write_u64(
        &mut bytes,
        HEADER_EXTENSION_OFFSET_OFFSET,
        header.extension_offset,
    );
    write_u64(
        &mut bytes,
        HEADER_EXTENSION_LEN_OFFSET,
        header.extension_len,
    );
    write_u64(
        &mut bytes,
        HEADER_FIRST_SEGMENT_ID_OFFSET,
        header.first_segment_id.get(),
    );
    write_u64(
        &mut bytes,
        HEADER_LAST_SEGMENT_ID_OFFSET,
        header.last_segment_id.get(),
    );
    write_u64(
        &mut bytes,
        HEADER_FIRST_PAGE_ID_OFFSET,
        header.first_page_id.get(),
    );
    write_u64(
        &mut bytes,
        HEADER_LAST_PAGE_ID_OFFSET,
        header.last_page_id.get(),
    );
    write_u64(
        &mut bytes,
        HEADER_MIN_PAGE_LSN_OFFSET,
        header.min_page_lsn.get(),
    );
    write_u64(
        &mut bytes,
        HEADER_MAX_PAGE_LSN_OFFSET,
        header.max_page_lsn.get(),
    );
    write_u64(
        &mut bytes,
        HEADER_TOTAL_PAGE_COUNT_OFFSET,
        header.total_page_count,
    );
    bytes[HEADER_PARENT_MANIFEST_HASH_OFFSET..HEADER_PARENT_MANIFEST_HASH_OFFSET + 32]
        .copy_from_slice(&header.parent_manifest_hash);
    bytes[HEADER_ENTRY_TABLE_SHA256_OFFSET..HEADER_ENTRY_TABLE_SHA256_OFFSET + 32]
        .copy_from_slice(&header.entry_table_sha256);
    write_u64(
        &mut bytes,
        HEADER_ENTRY_TABLE_CRC64_OFFSET,
        header.entry_table_crc64,
    );
    write_u64(&mut bytes, HEADER_RESERVED_248_OFFSET, 0);
    bytes
}

fn encode_entries(entries: &[SegmentIndexEntryV0]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(entries.len() * SEGMENT_INDEX_V0_ENTRY_LEN);
    for entry in entries {
        bytes.extend_from_slice(&encode_entry(entry));
    }
    bytes
}

fn encode_entry(entry: &SegmentIndexEntryV0) -> [u8; SEGMENT_INDEX_V0_ENTRY_LEN] {
    let mut bytes = [0u8; SEGMENT_INDEX_V0_ENTRY_LEN];
    write_u64(&mut bytes, 0, entry.segment_id.get());
    write_u64(&mut bytes, 8, entry.object_id.get());
    write_u64(&mut bytes, 16, entry.allocation_id.get());
    write_u64(&mut bytes, 24, entry.first_extent_id.get());
    write_u32(&mut bytes, 32, entry.extent_count);
    write_u16(&mut bytes, 36, page_size_tag(entry.page_size));
    write_u16(&mut bytes, 38, segment_state_tag(entry.segment_state));
    write_u64(&mut bytes, 40, entry.first_page_id.get());
    write_u32(&mut bytes, 48, entry.page_count);
    write_u32(&mut bytes, ENTRY_RESERVED_52_OFFSET, 0);
    write_u64(&mut bytes, 56, entry.min_page_lsn.get());
    write_u64(&mut bytes, 64, entry.max_page_lsn.get());
    write_u64(&mut bytes, 72, entry.snapshot_id);
    write_u64(&mut bytes, 80, entry.segment_file_id);
    write_u64(&mut bytes, 88, entry.segment_file_offset);
    write_u64(&mut bytes, 96, entry.segment_byte_len);
    write_u64(&mut bytes, 104, entry.segment_payload_crc64);
    bytes[112..144].copy_from_slice(&entry.segment_sha256);
    write_u32(&mut bytes, 144, entry.segment_header_crc32);
    write_u32(&mut bytes, 148, entry.segment_trailer_crc32);
    write_u32(&mut bytes, 152, entry.entry_flags);
    write_u32(&mut bytes, ENTRY_CRC_OFFSET, entry.entry_crc32);
    bytes
}

fn encode_trailer(
    trailer: &SegmentIndexTrailerV0,
    mode: TrailerHashMode,
) -> [u8; SEGMENT_INDEX_V0_TRAILER_LEN] {
    let mut bytes = [0u8; SEGMENT_INDEX_V0_TRAILER_LEN];
    write_u64(&mut bytes, 0, trailer.entry_table_crc64_mirror);
    match mode {
        TrailerHashMode::Stored => {
            bytes[TRAILER_FILE_SHA_OFFSET..TRAILER_FILE_SHA_OFFSET + 32]
                .copy_from_slice(&trailer.segment_index_file_sha256);
            bytes[TRAILER_ROOT_HASH_OFFSET..TRAILER_ROOT_HASH_OFFSET + 32]
                .copy_from_slice(&trailer.segment_index_root_hash);
            write_u32(&mut bytes, TRAILER_CRC_OFFSET, trailer.trailer_crc32);
        }
        TrailerHashMode::AcyclicFileHashInput => {
            write_u32(&mut bytes, TRAILER_CRC_OFFSET, 0);
        }
    }
    write_u32(&mut bytes, TRAILER_FLAGS_OFFSET, trailer.trailer_flags);
    bytes
}

fn header_crc32(header: &SegmentIndexHeaderV0) -> u32 {
    crc32_iso_hdlc(
        &encode_header(header),
        HEADER_CRC_OFFSET..HEADER_CRC_OFFSET + 4,
    )
}

fn entry_crc32(entry: &SegmentIndexEntryV0) -> u32 {
    crc32_iso_hdlc(&encode_entry(entry), ENTRY_CRC_OFFSET..ENTRY_CRC_OFFSET + 4)
}

fn trailer_crc32(trailer: &SegmentIndexTrailerV0) -> u32 {
    crc32_iso_hdlc(
        &encode_trailer(trailer, TrailerHashMode::Stored),
        TRAILER_CRC_OFFSET..TRAILER_CRC_OFFSET + 4,
    )
}

fn file_sha256(index: &SegmentIndexV0, trailer: &SegmentIndexTrailerV0) -> [u8; 32] {
    let mut input = index.clone();
    input.trailer = trailer.clone();
    sha256(&encode_file(&input, TrailerHashMode::AcyclicFileHashInput))
}

fn root_hash(header: &SegmentIndexHeaderV0, file_sha256: [u8; 32]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(32 + 8 + 8 + 32 + 32);
    bytes.extend_from_slice(&header.parent_manifest_hash);
    bytes.extend_from_slice(&header.segment_index_id.to_le_bytes());
    bytes.extend_from_slice(&header.snapshot_id.to_le_bytes());
    bytes.extend_from_slice(&header.entry_table_sha256);
    bytes.extend_from_slice(&file_sha256);
    sha256(&bytes)
}

fn summarize_entries(entries: &[SegmentIndexEntryV0]) -> SegmentIndexResult<EntrySummary> {
    let first = entries.first().ok_or(SegmentIndexError::InvalidHeader {
        reason: "entry count must not be zero",
    })?;
    let last = entries.last().ok_or(SegmentIndexError::InvalidHeader {
        reason: "entry count must not be zero",
    })?;

    let mut min_page_lsn = first.min_page_lsn;
    let mut max_page_lsn = first.max_page_lsn;
    let mut total_page_count = 0_u64;
    for entry in entries {
        min_page_lsn = min_page_lsn.min(entry.min_page_lsn);
        max_page_lsn = max_page_lsn.max(entry.max_page_lsn);
        total_page_count = total_page_count
            .checked_add(u64::from(entry.page_count))
            .ok_or(SegmentIndexError::LengthOverflow {
                field: "total_page_count",
            })?;
    }

    Ok(EntrySummary {
        first_segment_id: first.segment_id,
        last_segment_id: last.segment_id,
        first_page_id: first.first_page_id,
        last_page_id: last.last_page_id()?,
        min_page_lsn,
        max_page_lsn,
        total_page_count,
    })
}

fn computed_flags(entries: &[SegmentIndexEntryV0]) -> u32 {
    let mut flags = SEGMENT_INDEX_V0_FLAG_PUBLISHED_COLD_ONLY;
    if entries.windows(2).all(|window| {
        window[0]
            .last_page_id()
            .map(|last| window[1].first_page_id.get() == last.get() + 1)
            .unwrap_or(false)
    }) {
        flags |= SEGMENT_INDEX_V0_FLAG_CONTIGUOUS_PAGE_RANGES;
    }
    flags
}

fn page_size_policy(entries: &[SegmentIndexEntryV0]) -> SegmentIndexResult<u32> {
    let first = entries.first().ok_or(SegmentIndexError::InvalidHeader {
        reason: "entry count must not be zero",
    })?;
    if entries
        .iter()
        .all(|entry| entry.page_size == first.page_size)
    {
        return Ok(match first.page_size {
            PageSize::KiB16 => PAGE_SIZE_POLICY_16K,
            PageSize::KiB32 => PAGE_SIZE_POLICY_32K,
        });
    }
    Ok(PAGE_SIZE_POLICY_MIXED)
}

fn validate_page_size_policy(tag: u32) -> SegmentIndexResult<()> {
    match tag {
        PAGE_SIZE_POLICY_MIXED | PAGE_SIZE_POLICY_16K | PAGE_SIZE_POLICY_32K => Ok(()),
        _ => Err(SegmentIndexError::InvalidHeader {
            reason: "unknown page size policy tag",
        }),
    }
}

fn page_size_tag(page_size: PageSize) -> u16 {
    match page_size {
        PageSize::KiB16 => PAGE_SIZE_TAG_16K,
        PageSize::KiB32 => PAGE_SIZE_TAG_32K,
    }
}

fn decode_page_size_tag(tag: u16, index: usize) -> SegmentIndexResult<PageSize> {
    match tag {
        PAGE_SIZE_TAG_16K => Ok(PageSize::KiB16),
        PAGE_SIZE_TAG_32K => Ok(PageSize::KiB32),
        _ => Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "unknown page size tag",
        }),
    }
}

fn segment_state_tag(state: SegmentState) -> u16 {
    match state {
        SegmentState::BuildingHotSnapshot => SEGMENT_STATE_TAG_BUILDING_HOT_SNAPSHOT,
        SegmentState::Sealed => SEGMENT_STATE_TAG_SEALED,
        SegmentState::PublishedCold => SEGMENT_STATE_TAG_PUBLISHED_COLD,
    }
}

fn decode_segment_state_tag(tag: u16, index: usize) -> SegmentIndexResult<SegmentState> {
    match tag {
        SEGMENT_STATE_TAG_BUILDING_HOT_SNAPSHOT => Ok(SegmentState::BuildingHotSnapshot),
        SEGMENT_STATE_TAG_SEALED => Ok(SegmentState::Sealed),
        SEGMENT_STATE_TAG_PUBLISHED_COLD => Ok(SegmentState::PublishedCold),
        _ => Err(SegmentIndexError::InvalidEntry {
            index,
            reason: "unknown segment state tag",
        }),
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

fn read_u16(source: &[u8], offset: usize, field: &'static str) -> SegmentIndexResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or(SegmentIndexError::LengthOverflow { field })?;
    let mut bytes = [0u8; 2];
    bytes.copy_from_slice(
        source
            .get(offset..end)
            .ok_or(SegmentIndexError::Truncated { field })?,
    );
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(source: &[u8], offset: usize, field: &'static str) -> SegmentIndexResult<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(SegmentIndexError::LengthOverflow { field })?;
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(
        source
            .get(offset..end)
            .ok_or(SegmentIndexError::Truncated { field })?,
    );
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(source: &[u8], offset: usize, field: &'static str) -> SegmentIndexResult<u64> {
    let end = offset
        .checked_add(8)
        .ok_or(SegmentIndexError::LengthOverflow { field })?;
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(
        source
            .get(offset..end)
            .ok_or(SegmentIndexError::Truncated { field })?,
    );
    Ok(u64::from_le_bytes(bytes))
}

fn read_hash(source: &[u8], offset: usize, field: &'static str) -> SegmentIndexResult<[u8; 32]> {
    let end = offset
        .checked_add(32)
        .ok_or(SegmentIndexError::LengthOverflow { field })?;
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(
        source
            .get(offset..end)
            .ok_or(SegmentIndexError::Truncated { field })?,
    );
    Ok(bytes)
}

#[derive(Debug, Clone, Copy)]
struct EntrySummary {
    first_segment_id: SegmentId,
    last_segment_id: SegmentId,
    first_page_id: PageId,
    last_page_id: PageId,
    min_page_lsn: Lsn,
    max_page_lsn: Lsn,
    total_page_count: u64,
}

#[derive(Debug, Clone, Copy)]
enum TrailerHashMode {
    Stored,
    AcyclicFileHashInput,
}
