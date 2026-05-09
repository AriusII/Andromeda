use crate::{AllocationId, ExtentId, Lsn, ObjectId, PageId, PageSize, SegmentId, SegmentState};

use super::{
    codec::{build_entry_with_crc, build_header, build_trailer, validate_index_contract},
    error::{SegmentIndexError, SegmentIndexResult},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentIndexV0 {
    pub header: SegmentIndexHeaderV0,
    pub entries: Vec<SegmentIndexEntryV0>,
    pub extension_bytes: Vec<u8>,
    pub trailer: SegmentIndexTrailerV0,
}

impl SegmentIndexV0 {
    pub fn new(
        context: SegmentIndexBuildContextV0,
        entries: Vec<SegmentIndexEntryV0>,
        extension_bytes: Vec<u8>,
    ) -> SegmentIndexResult<Self> {
        context.validate()?;
        let header = build_header(&context, &entries, &extension_bytes)?;
        let mut index = Self {
            header,
            entries,
            extension_bytes,
            trailer: SegmentIndexTrailerV0::empty(),
        };
        index.trailer = build_trailer(&index)?;
        index.validate()?;
        Ok(index)
    }

    pub fn validate(&self) -> SegmentIndexResult<()> {
        validate_index_contract(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentIndexBuildContextV0 {
    pub database_id: u64,
    pub snapshot_id: u64,
    pub segment_index_id: u64,
    pub manifest_version: u64,
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
    pub parent_manifest_hash: [u8; 32],
}

impl SegmentIndexBuildContextV0 {
    pub fn validate(&self) -> SegmentIndexResult<()> {
        if self.database_id == 0
            || self.snapshot_id == 0
            || self.segment_index_id == 0
            || self.manifest_version == 0
        {
            return Err(SegmentIndexError::InvalidHeader {
                reason: "manifest identity fields must not be zero",
            });
        }
        if self.base_checkpoint_lsn.is_zero() || self.required_wal_start_lsn.is_zero() {
            return Err(SegmentIndexError::InvalidHeader {
                reason: "manifest LSN fields must not be zero",
            });
        }
        if self.parent_manifest_hash == [0; 32] {
            return Err(SegmentIndexError::InvalidHeader {
                reason: "parent manifest hash must not be zero",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentIndexHeaderV0 {
    pub magic: [u8; 8],
    pub format_major: u16,
    pub format_minor: u16,
    pub byte_order: u16,
    pub header_len: u16,
    pub total_len: u64,
    pub header_crc32: u32,
    pub flags: u32,
    pub database_id: u64,
    pub snapshot_id: u64,
    pub segment_index_id: u64,
    pub manifest_version: u64,
    pub base_checkpoint_lsn: Lsn,
    pub required_wal_start_lsn: Lsn,
    pub entry_offset: u64,
    pub entry_count: u64,
    pub entry_len: u32,
    pub page_size_policy_tag: u32,
    pub extension_offset: u64,
    pub extension_len: u64,
    pub first_segment_id: SegmentId,
    pub last_segment_id: SegmentId,
    pub first_page_id: PageId,
    pub last_page_id: PageId,
    pub min_page_lsn: Lsn,
    pub max_page_lsn: Lsn,
    pub total_page_count: u64,
    pub parent_manifest_hash: [u8; 32],
    pub entry_table_sha256: [u8; 32],
    pub entry_table_crc64: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentIndexEntryV0 {
    pub segment_id: SegmentId,
    pub object_id: ObjectId,
    pub allocation_id: AllocationId,
    pub first_extent_id: ExtentId,
    pub extent_count: u32,
    pub page_size: PageSize,
    pub segment_state: SegmentState,
    pub first_page_id: PageId,
    pub page_count: u32,
    pub min_page_lsn: Lsn,
    pub max_page_lsn: Lsn,
    pub snapshot_id: u64,
    pub segment_file_id: u64,
    pub segment_file_offset: u64,
    pub segment_byte_len: u64,
    pub segment_payload_crc64: u64,
    pub segment_sha256: [u8; 32],
    pub segment_header_crc32: u32,
    pub segment_trailer_crc32: u32,
    pub entry_flags: u32,
    pub entry_crc32: u32,
}

impl SegmentIndexEntryV0 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        segment_id: SegmentId,
        object_id: ObjectId,
        allocation_id: AllocationId,
        first_extent_id: ExtentId,
        extent_count: u32,
        page_size: PageSize,
        first_page_id: PageId,
        page_count: u32,
        min_page_lsn: Lsn,
        max_page_lsn: Lsn,
        snapshot_id: u64,
        segment_file_id: u64,
        segment_file_offset: u64,
        segment_byte_len: u64,
        segment_payload_crc64: u64,
        segment_sha256: [u8; 32],
        segment_header_crc32: u32,
        segment_trailer_crc32: u32,
    ) -> SegmentIndexResult<Self> {
        let entry = Self {
            segment_id,
            object_id,
            allocation_id,
            first_extent_id,
            extent_count,
            page_size,
            segment_state: SegmentState::PublishedCold,
            first_page_id,
            page_count,
            min_page_lsn,
            max_page_lsn,
            snapshot_id,
            segment_file_id,
            segment_file_offset,
            segment_byte_len,
            segment_payload_crc64,
            segment_sha256,
            segment_header_crc32,
            segment_trailer_crc32,
            entry_flags: 0,
            entry_crc32: 0,
        };
        build_entry_with_crc(entry, 0)
    }

    pub fn last_page_id(&self) -> SegmentIndexResult<PageId> {
        if self.page_count == 0 {
            return Err(SegmentIndexError::InvalidEntry {
                index: 0,
                reason: "page count must not be zero",
            });
        }
        let last_page_id = self
            .first_page_id
            .get()
            .checked_add(u64::from(self.page_count - 1))
            .ok_or(SegmentIndexError::InvalidEntry {
                index: 0,
                reason: "page range overflows u64",
            })?;
        Ok(PageId::new(last_page_id))
    }

    pub fn last_extent_id(&self) -> SegmentIndexResult<ExtentId> {
        if self.extent_count == 0 {
            return Err(SegmentIndexError::InvalidEntry {
                index: 0,
                reason: "extent count must not be zero",
            });
        }
        let last_extent_id = self
            .first_extent_id
            .get()
            .checked_add(u64::from(self.extent_count - 1))
            .ok_or(SegmentIndexError::InvalidEntry {
                index: 0,
                reason: "extent range overflows u64",
            })?;
        Ok(ExtentId::new(last_extent_id))
    }

    pub fn segment_end_offset(&self) -> SegmentIndexResult<u64> {
        self.segment_file_offset
            .checked_add(self.segment_byte_len)
            .ok_or(SegmentIndexError::InvalidEntry {
                index: 0,
                reason: "segment byte range overflows u64",
            })
    }

    pub fn contains_page(&self, page_id: PageId) -> SegmentIndexResult<bool> {
        let value = page_id.get();
        Ok(value >= self.first_page_id.get() && value <= self.last_page_id()?.get())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentIndexTrailerV0 {
    pub entry_table_crc64_mirror: u64,
    pub segment_index_file_sha256: [u8; 32],
    pub segment_index_root_hash: [u8; 32],
    pub trailer_crc32: u32,
    pub trailer_flags: u32,
}

impl SegmentIndexTrailerV0 {
    pub(super) const fn empty() -> Self {
        Self {
            entry_table_crc64_mirror: 0,
            segment_index_file_sha256: [0; 32],
            segment_index_root_hash: [0; 32],
            trailer_crc32: 0,
            trailer_flags: 0,
        }
    }
}
