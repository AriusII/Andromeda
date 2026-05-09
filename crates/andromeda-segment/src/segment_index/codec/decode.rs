use crate::{AllocationId, ExtentId, Lsn, ObjectId, PageId, SegmentId};

use super::super::{
    error::{SegmentIndexError, SegmentIndexResult},
    types::{SegmentIndexEntryV0, SegmentIndexHeaderV0, SegmentIndexTrailerV0, SegmentIndexV0},
};
use super::{
    ENTRY_CRC_OFFSET, ENTRY_RESERVED_52_OFFSET, HEADER_BASE_CHECKPOINT_LSN_OFFSET,
    HEADER_CRC_OFFSET, HEADER_DATABASE_ID_OFFSET, HEADER_ENTRY_COUNT_OFFSET,
    HEADER_ENTRY_LEN_OFFSET, HEADER_ENTRY_OFFSET_OFFSET, HEADER_ENTRY_TABLE_CRC64_OFFSET,
    HEADER_ENTRY_TABLE_SHA256_OFFSET, HEADER_EXTENSION_LEN_OFFSET, HEADER_EXTENSION_OFFSET_OFFSET,
    HEADER_FIRST_PAGE_ID_OFFSET, HEADER_FIRST_SEGMENT_ID_OFFSET, HEADER_FLAGS_OFFSET,
    HEADER_LAST_PAGE_ID_OFFSET, HEADER_LAST_SEGMENT_ID_OFFSET, HEADER_MANIFEST_VERSION_OFFSET,
    HEADER_MAX_PAGE_LSN_OFFSET, HEADER_MIN_PAGE_LSN_OFFSET, HEADER_PAGE_SIZE_POLICY_TAG_OFFSET,
    HEADER_PARENT_MANIFEST_HASH_OFFSET, HEADER_REQUIRED_WAL_START_LSN_OFFSET,
    HEADER_RESERVED_248_OFFSET, HEADER_SEGMENT_INDEX_ID_OFFSET, HEADER_SNAPSHOT_ID_OFFSET,
    HEADER_TOTAL_LEN_OFFSET, HEADER_TOTAL_PAGE_COUNT_OFFSET, SEGMENT_INDEX_V0_ENTRY_LEN,
    SEGMENT_INDEX_V0_HEADER_LEN, SEGMENT_INDEX_V0_TRAILER_LEN, TRAILER_CRC_OFFSET,
    TRAILER_FILE_SHA_OFFSET, TRAILER_FLAGS_OFFSET, TRAILER_RESERVED_80_OFFSET,
    TRAILER_ROOT_HASH_OFFSET,
    binary::{read_hash, read_u16, read_u32, read_u64},
    integrity::entry_crc32,
    policy::{decode_page_size_tag, decode_segment_state_tag},
    validation::{validate_entry_semantics, validate_header_before_allocate},
};

impl SegmentIndexV0 {
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

pub(super) fn decode_header(bytes: &[u8]) -> SegmentIndexResult<SegmentIndexHeaderV0> {
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

pub(super) fn decode_entry(index: usize, bytes: &[u8]) -> SegmentIndexResult<SegmentIndexEntryV0> {
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

pub(super) fn decode_trailer(bytes: &[u8]) -> SegmentIndexResult<SegmentIndexTrailerV0> {
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
