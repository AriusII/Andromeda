use andromeda_storage_index::{BTreeLatchLevel, BTreeLatchTarget, PageId};

pub(crate) fn latch_target(
    page_id: u64,
    level: BTreeLatchLevel,
    tree_depth: u16,
) -> BTreeLatchTarget {
    BTreeLatchTarget::new(PageId::new(page_id), level, tree_depth)
}
