use andromeda_core::AndromedaResult;

use crate::{Lsn, PageId, PageSize};

use super::{ExtentDescriptor, error::storage_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtentFreeRange {
    pub first_page_id: PageId,
    pub page_count: u32,
    pub page_size: PageSize,
    pub recyclable_after_lsn: Option<Lsn>,
}

impl ExtentFreeRange {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.first_page_id.is_zero() || self.page_count == 0 {
            return Err(storage_error("free extent range must not be empty"));
        }
        if self
            .first_page_id
            .get()
            .checked_add(u64::from(self.page_count - 1))
            .is_none()
        {
            return Err(storage_error("free extent range overflows u64"));
        }
        if matches!(self.recyclable_after_lsn, Some(lsn) if lsn.is_zero()) {
            return Err(storage_error(
                "free extent recyclable-after LSN must not be zero",
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

    pub(crate) fn contains_descriptor(
        &self,
        descriptor: &ExtentDescriptor,
    ) -> AndromedaResult<bool> {
        self.validate()?;
        descriptor.validate()?;
        let range_end = self.last_page_id()?.get();
        let descriptor_end = descriptor.last_page_id()?.get();
        Ok(self.page_size == descriptor.page_size
            && self.first_page_id.get() <= descriptor.first_page_id.get()
            && descriptor_end <= range_end)
    }

    pub(crate) fn overlaps_descriptor(
        &self,
        descriptor: &ExtentDescriptor,
    ) -> AndromedaResult<bool> {
        self.validate()?;
        descriptor.validate()?;
        let start = self.first_page_id.get();
        let end = self.last_page_id()?.get();
        let other_start = descriptor.first_page_id.get();
        let other_end = descriptor.last_page_id()?.get();
        Ok(start <= other_end && other_start <= end)
    }
}
