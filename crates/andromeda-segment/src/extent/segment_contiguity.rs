use andromeda_core::AndromedaResult;

use crate::{SegmentDescriptor, SegmentState};

use super::{ExtentDescriptor, ExtentState, error::storage_error};

pub fn validate_segment_extent_contiguity(
    segment: &SegmentDescriptor,
    extents: &[ExtentDescriptor],
) -> AndromedaResult<()> {
    segment.validate()?;
    if extents.len()
        != usize::try_from(segment.extent_count)
            .map_err(|_| storage_error("segment extent count does not fit usize"))?
    {
        return Err(storage_error(
            "segment extent list must match segment extent count",
        ));
    }

    let mut expected_extent_id = segment.first_extent_id.get();
    let mut expected_first_page = segment.first_page_id.get();
    let segment_last_page = segment
        .first_page_id
        .get()
        .checked_add(u64::from(segment.page_count - 1))
        .ok_or_else(|| storage_error("segment page range overflows u64"))?;

    for extent in extents {
        extent.validate()?;
        if extent.extent_id.get() != expected_extent_id {
            return Err(storage_error(
                "segment extents must use contiguous extent identifiers",
            ));
        }
        if extent.object_id != segment.object_id
            || extent.allocation_id != segment.allocation_id
            || extent.page_size != segment.page_size
        {
            return Err(storage_error(
                "segment extent identity and page size must match segment descriptor",
            ));
        }
        if extent.first_page_id.get() != expected_first_page {
            return Err(storage_error(
                "segment extents must cover a physically contiguous page range",
            ));
        }
        if extent.last_page_id()?.get() > segment_last_page {
            return Err(storage_error("segment extent page range exceeds segment"));
        }
        if segment.state == SegmentState::PublishedCold
            && (extent.state != ExtentState::PublishedCold
                || extent.segment_id != Some(segment.segment_id))
        {
            return Err(storage_error(
                "published cold segment requires all extents to be published and segment-bound",
            ));
        }
        expected_extent_id = expected_extent_id
            .checked_add(1)
            .ok_or_else(|| storage_error("segment extent id range overflows u64"))?;
        expected_first_page = extent
            .last_page_id()?
            .get()
            .checked_add(1)
            .ok_or_else(|| storage_error("segment page range overflows u64"))?;
    }

    let expected_after_segment = segment_last_page
        .checked_add(1)
        .ok_or_else(|| storage_error("segment page range overflows u64"))?;
    if expected_first_page != expected_after_segment {
        return Err(storage_error(
            "segment extents must exactly cover the segment page range",
        ));
    }
    Ok(())
}
