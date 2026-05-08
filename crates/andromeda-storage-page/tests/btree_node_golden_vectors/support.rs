use andromeda_storage_page::{BTREE_NODE_V1_HEADER_LEN, BTreeNodeV1, Lsn, PageId};

pub(crate) const PAGE_SIZE: u16 = 4096;

pub(crate) fn leaf_entry(key: &[u8], value: u64) -> (Vec<u8>, Vec<u8>) {
    (key.to_vec(), value.to_le_bytes().to_vec())
}

pub(crate) fn leaf_node(
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

pub(crate) fn empty_leaf(page_id: u64, page_lsn: u64) -> BTreeNodeV1 {
    leaf_node(page_id, page_lsn, Vec::new(), None, None)
}

pub(crate) fn internal_node(
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

pub(crate) fn refresh_btree_header_crc(encoded: &mut [u8]) {
    encoded[52..56].copy_from_slice(&0u32.to_le_bytes());
    let mut state = 0x811C_9DC5u32;
    for (idx, byte) in encoded[..BTREE_NODE_V1_HEADER_LEN].iter().enumerate() {
        let byte = if (52..56).contains(&idx) { 0 } else { *byte };
        state ^= u32::from(byte);
        state = state.wrapping_mul(0x0100_0193);
    }
    let crc = if state == 0 { 1 } else { state };
    encoded[52..56].copy_from_slice(&crc.to_le_bytes());
}
