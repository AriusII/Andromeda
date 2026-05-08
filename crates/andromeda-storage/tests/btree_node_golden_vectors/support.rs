use std::any::TypeId;

use andromeda_storage_page::{BTreeNodeV1, Lsn, PageId};

pub(crate) const PAGE_SIZE: u16 = 4096;

pub(crate) fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
}

pub(crate) fn leaf_entry(key: &[u8], value: u64) -> (Vec<u8>, Vec<u8>) {
    (key.to_vec(), value.to_le_bytes().to_vec())
}

pub(crate) fn owner_leaf_node(
    page_id: u64,
    page_lsn: u64,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    prev_leaf: Option<u64>,
    next_leaf: Option<u64>,
) -> BTreeNodeV1 {
    BTreeNodeV1::new_leaf(
        PageId::new(page_id),
        Lsn::new(page_lsn),
        entries,
        prev_leaf.map(PageId::new),
        next_leaf.map(PageId::new),
        PAGE_SIZE,
    )
    .expect("leaf encodes")
}

pub(crate) fn owner_internal_node(
    page_id: u64,
    page_lsn: u64,
    keys: Vec<Vec<u8>>,
    children: Vec<u64>,
) -> BTreeNodeV1 {
    BTreeNodeV1::new_internal(
        PageId::new(page_id),
        Lsn::new(page_lsn),
        keys,
        children.into_iter().map(PageId::new).collect(),
        PAGE_SIZE,
    )
    .expect("internal node encodes")
}

pub(crate) fn encode_node(node: &BTreeNodeV1) -> Vec<u8> {
    node.encode().expect("node image encodes")
}
