use crate::support::{PAGE_SIZE, leaf_entry, leaf_node};
use andromeda_storage::{BTREE_NODE_V1_HEADER_LEN, BTreeNodeV1, Lsn, PageId};

#[test]
fn invalid_key_order_is_rejected_before_persistence() {
    let err = BTreeNodeV1::new_leaf(
        PageId::new(80),
        Lsn::new(10),
        vec![leaf_entry(b"b", 10), leaf_entry(b"a", 11)],
        None,
        None,
        PAGE_SIZE,
    )
    .expect_err("descending keys must be rejected");
    assert!(err.message().contains("strictly ordered"));
}

#[test]
fn invalid_internal_child_count_is_rejected_before_persistence() {
    let err = BTreeNodeV1::new_internal(
        PageId::new(90),
        Lsn::new(11),
        vec![b"k10".to_vec(), b"k20".to_vec()],
        vec![PageId::new(100), PageId::new(101)],
        PAGE_SIZE,
    )
    .expect_err("internal node requires keys + 1 children");
    assert!(err.message().contains("keys + 1"));
}

#[test]
fn leaf_self_sibling_links_are_rejected_before_persistence() {
    let err = BTreeNodeV1::new_leaf(
        PageId::new(93),
        Lsn::new(14),
        Vec::new(),
        Some(PageId::new(93)),
        None,
        PAGE_SIZE,
    )
    .expect_err("leaf must not link to itself");
    assert!(err.message().contains("point to self"));

    let err = BTreeNodeV1::new_leaf(
        PageId::new(94),
        Lsn::new(15),
        Vec::new(),
        None,
        Some(PageId::new(94)),
        PAGE_SIZE,
    )
    .expect_err("leaf must not link to itself");
    assert!(err.message().contains("point to self"));
}

#[test]
fn high_key_offset_must_match_encoded_keys() {
    let mut node = leaf_node(
        95,
        16,
        vec![
            leaf_entry(b"a", 10),
            leaf_entry(b"b", 11),
            leaf_entry(b"c", 12),
        ],
        None,
        None,
    );
    node.header = node
        .header
        .clone()
        .with_high_key_offset(Some(BTREE_NODE_V1_HEADER_LEN as u16))
        .with_computed_crc()
        .expect("syntactically valid high-key offset recomputes CRC");

    let err = node
        .validate()
        .expect_err("wrong high-key offset must be rejected");
    assert!(err.message().contains("high_key_offset"));
}
