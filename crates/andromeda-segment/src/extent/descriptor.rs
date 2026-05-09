use andromeda_error::AndromedaResult;

use crate::{AllocationId, ObjectId, PageId, PageSize, SegmentId};

use super::{ExtentId, error::storage_error, page_range::checked_last_page_id};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtentState {
    AllocatingHot,
    Sealed,
    PublishedCold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtentDescriptor {
    pub extent_id: ExtentId,
    pub object_id: ObjectId,
    pub allocation_id: AllocationId,
    pub first_page_id: PageId,
    pub page_count: u32,
    pub page_size: PageSize,
    pub state: ExtentState,
    pub segment_id: Option<SegmentId>,
    /// File offset where this extent begins on disk (hot-store).
    /// Set during allocation; read from manifest during recovery.
    pub file_offset: u64,
    /// Whether pages in this extent have been actually written to disk.
    /// Used for contiguity checks and allocation tracking.
    pub allocated_on_disk: bool,
}

impl ExtentDescriptor {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.extent_id.is_zero() {
            return Err(storage_error("extent id must not be zero"));
        }
        if self.object_id.is_zero() || self.allocation_id.is_zero() {
            return Err(storage_error(
                "extent object/allocation identity must not be zero",
            ));
        }
        if self.first_page_id.is_zero() {
            return Err(storage_error("extent first page id must not be zero"));
        }
        if self.page_count == 0 {
            return Err(storage_error("extent page count must not be zero"));
        }
        checked_last_page_id(
            self.first_page_id,
            self.page_count,
            "extent page range overflows u64",
        )?;
        if matches!(self.segment_id, Some(segment_id) if segment_id.is_zero()) {
            return Err(storage_error("extent segment id must not be zero"));
        }
        if self.state == ExtentState::PublishedCold && self.segment_id.is_none() {
            return Err(storage_error(
                "published cold extent must reference a segment",
            ));
        }
        Ok(())
    }

    pub fn last_page_id(&self) -> AndromedaResult<PageId> {
        self.validate()?;
        checked_last_page_id(
            self.first_page_id,
            self.page_count,
            "extent page range overflows u64",
        )
    }
}
