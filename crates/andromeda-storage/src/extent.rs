use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{AllocationId, ObjectId, PageId, PageSize, SegmentId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtentId(u64);

impl ExtentId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

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
        if self
            .first_page_id
            .get()
            .checked_add(u64::from(self.page_count - 1))
            .is_none()
        {
            return Err(storage_error("extent page range overflows u64"));
        }
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
        Ok(PageId::new(
            self.first_page_id.get() + u64::from(self.page_count - 1),
        ))
    }
}

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extent() -> ExtentDescriptor {
        ExtentDescriptor {
            extent_id: ExtentId::new(1),
            object_id: ObjectId::new(2),
            allocation_id: AllocationId::new(3),
            first_page_id: PageId::new(100),
            page_count: 16,
            page_size: PageSize::KiB16,
            state: ExtentState::PublishedCold,
            segment_id: Some(SegmentId::new(4)),
        }
    }

    #[test]
    fn extent_contract_validates_page_range() {
        let descriptor = extent();
        assert!(descriptor.validate().is_ok());
        assert_eq!(descriptor.last_page_id().unwrap(), PageId::new(115));
    }

    #[test]
    fn extent_contract_rejects_published_extent_without_segment() {
        let mut descriptor = extent();
        descriptor.segment_id = None;

        assert_eq!(
            descriptor.validate().unwrap_err().kind(),
            AndromedaErrorKind::Storage
        );
    }
}
