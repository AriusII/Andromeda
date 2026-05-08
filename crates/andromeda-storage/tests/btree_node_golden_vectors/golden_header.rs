use crate::support::assert_same_type;
use andromeda_storage as storage;
use andromeda_storage_page as page;

#[test]
fn storage_facade_reexports_btree_node_v1_owner_types() {
    assert_same_type::<storage::BTreeNodeHeaderV1, page::BTreeNodeHeaderV1>();
    assert_same_type::<storage::BTreeNodeKindV1, page::BTreeNodeKindV1>();
    assert_same_type::<storage::BTreeNodeV1, page::BTreeNodeV1>();
    assert_same_type::<storage::PageId, page::PageId>();
    assert_same_type::<storage::Lsn, page::Lsn>();

    assert_eq!(
        storage::BTREE_NODE_V1_FORMAT_VERSION,
        page::BTREE_NODE_V1_FORMAT_VERSION
    );
    assert_eq!(
        storage::BTREE_NODE_V1_HEADER_LEN,
        page::BTREE_NODE_V1_HEADER_LEN
    );
    assert_eq!(storage::BTREE_NODE_V1_MAGIC, page::BTREE_NODE_V1_MAGIC);
}
