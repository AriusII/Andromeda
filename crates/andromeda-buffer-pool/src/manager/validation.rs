use andromeda_core::AndromedaResult;
use andromeda_storage_page::{PageId, PageImage, PageSize};

use crate::BufferPoolError;

pub(super) fn validate_page_id(page_id: PageId) -> AndromedaResult<()> {
    if page_id.is_zero() {
        return Err(BufferPoolError::InvalidPageId.into_andromeda_error());
    }
    Ok(())
}

pub(super) fn validate_image_for_pool(
    image: &PageImage,
    page_size: PageSize,
) -> AndromedaResult<PageId> {
    if image.page_size() != page_size {
        return Err(BufferPoolError::PageSizeMismatch.into_andromeda_error());
    }
    let layout_contract = image
        .layout_contract()
        .ok_or_else(|| BufferPoolError::InvalidPageImage.into_andromeda_error())?;
    layout_contract
        .validate()
        .map_err(|_| BufferPoolError::InvalidPageLayout.into_andromeda_error())?;
    let page_id = layout_contract.header.page_id;
    validate_page_id(page_id)?;
    Ok(page_id)
}
