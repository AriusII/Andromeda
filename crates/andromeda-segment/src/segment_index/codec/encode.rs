use super::super::{
    digest::{crc64_ecma, sha256},
    error::{SegmentIndexError, SegmentIndexResult},
    types::{
        SegmentIndexBuildContextV0, SegmentIndexEntryV0, SegmentIndexHeaderV0,
        SegmentIndexTrailerV0, SegmentIndexV0,
    },
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
    HEADER_TOTAL_LEN_OFFSET, HEADER_TOTAL_PAGE_COUNT_OFFSET, SEGMENT_INDEX_V0_BYTE_ORDER,
    SEGMENT_INDEX_V0_ENTRY_LEN, SEGMENT_INDEX_V0_FORMAT_MAJOR, SEGMENT_INDEX_V0_FORMAT_MINOR,
    SEGMENT_INDEX_V0_HEADER_LEN, SEGMENT_INDEX_V0_MAGIC, SEGMENT_INDEX_V0_TRAILER_LEN,
    TRAILER_CRC_OFFSET, TRAILER_FILE_SHA_OFFSET, TRAILER_FLAGS_OFFSET, TRAILER_ROOT_HASH_OFFSET,
    binary::{write_u16, write_u32, write_u64},
    integrity::{entry_crc32, file_sha256, header_crc32, root_hash, trailer_crc32},
    policy::{
        TrailerHashMode, computed_flags, page_size_policy, page_size_tag, segment_state_tag,
        summarize_entries,
    },
    validation::{validate_entries_for_header, validate_entry_semantics, validate_extension_bytes},
};

impl SegmentIndexV0 {
    pub fn encode(&self) -> SegmentIndexResult<Vec<u8>> {
        self.validate()?;
        Ok(encode_file(self, TrailerHashMode::Stored))
    }
}

pub(in crate::segment_index) fn build_entry_with_crc(
    mut entry: SegmentIndexEntryV0,
    index: usize,
) -> SegmentIndexResult<SegmentIndexEntryV0> {
    validate_entry_semantics(index, &entry)?;
    entry.entry_crc32 = 0;
    entry.entry_crc32 = entry_crc32(&entry);
    Ok(entry)
}

pub(in crate::segment_index) fn build_header(
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

pub(in crate::segment_index) fn build_trailer(
    index: &SegmentIndexV0,
) -> SegmentIndexResult<SegmentIndexTrailerV0> {
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

pub(super) fn encode_file(index: &SegmentIndexV0, trailer_mode: TrailerHashMode) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(index.header.total_len as usize);
    bytes.extend_from_slice(&encode_header(&index.header));
    bytes.extend_from_slice(&encode_entries(&index.entries));
    bytes.extend_from_slice(&index.extension_bytes);
    bytes.extend_from_slice(&encode_trailer(&index.trailer, trailer_mode));
    bytes
}

pub(super) fn encode_header(header: &SegmentIndexHeaderV0) -> [u8; SEGMENT_INDEX_V0_HEADER_LEN] {
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

pub(super) fn encode_entries(entries: &[SegmentIndexEntryV0]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(entries.len() * SEGMENT_INDEX_V0_ENTRY_LEN);
    for entry in entries {
        bytes.extend_from_slice(&encode_entry(entry));
    }
    bytes
}

pub(super) fn encode_entry(entry: &SegmentIndexEntryV0) -> [u8; SEGMENT_INDEX_V0_ENTRY_LEN] {
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

pub(super) fn encode_trailer(
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
        },
        TrailerHashMode::AcyclicFileHashInput => {
            write_u32(&mut bytes, TRAILER_CRC_OFFSET, 0);
        },
    }
    write_u32(&mut bytes, TRAILER_FLAGS_OFFSET, trailer.trailer_flags);
    bytes
}
