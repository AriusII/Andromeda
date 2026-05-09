use andromeda_error::AndromedaResult;

use crate::PageId;

use super::error::storage_error;

pub(super) fn checked_last_page_id(
    first_page_id: PageId,
    page_count: u32,
    overflow_msg: &'static str,
) -> AndromedaResult<PageId> {
    let last_page_id = first_page_id
        .get()
        .checked_add(u64::from(page_count - 1))
        .ok_or_else(|| storage_error(overflow_msg))?;
    Ok(PageId::new(last_page_id))
}
