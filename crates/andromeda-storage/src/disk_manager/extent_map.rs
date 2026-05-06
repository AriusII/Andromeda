use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::{ExtentDescriptor, PageId};

use super::FileDiskManager;

impl FileDiskManager {
    pub(crate) fn extent_for_page_impl(
        &self,
        page_id: PageId,
    ) -> AndromedaResult<Option<ExtentDescriptor>> {
        let page_id_val = page_id.get();

        for (first_page, extent) in self.extents_by_page_range.iter().rev() {
            let last_page = extent
                .last_page_id()
                .map_err(|e| {
                    AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!("Failed to compute extent bounds: {}", e.message()),
                    )
                })?
                .get();

            if page_id_val >= *first_page && page_id_val <= last_page {
                return Ok(Some(*extent));
            }

            if page_id_val > last_page {
                break;
            }
        }

        Ok(None)
    }

    pub(crate) fn page_to_file_offset_impl(&self, page_id: PageId) -> AndromedaResult<u64> {
        let extent = self.extent_for_page_impl(page_id)?.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Page {} not allocated", page_id.get()),
            )
        })?;

        let offset_within_extent = page_id
            .get()
            .checked_sub(extent.first_page_id.get())
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!(
                        "Page ID {} before extent start {}",
                        page_id.get(),
                        extent.first_page_id.get()
                    ),
                )
            })?;

        let page_offset = offset_within_extent
            .checked_mul(u64::from(extent.page_size.bytes()))
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "Page offset computation overflowed",
                )
            })?;

        extent.file_offset.checked_add(page_offset).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!(
                    "File offset computation overflowed for page {}",
                    page_id.get()
                ),
            )
        })
    }

    pub(crate) fn verify_extent_contiguity_impl(
        &self,
        descriptor: &ExtentDescriptor,
    ) -> AndromedaResult<()> {
        descriptor.validate().map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("Invalid extent descriptor: {}", e.message()),
            )
        })?;

        let new_start = descriptor.first_page_id.get();
        let new_end = descriptor
            .last_page_id()
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("Failed to compute extent bounds: {}", e.message()),
                )
            })?
            .get();

        for (existing_start, existing_end) in &self.allocated_page_ranges {
            if !(new_end < *existing_start || new_start > *existing_end) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!(
                        "Extent [{}, {}] overlaps existing range [{}, {}]",
                        new_start, new_end, existing_start, existing_end
                    ),
                ));
            }
        }

        Ok(())
    }
}
