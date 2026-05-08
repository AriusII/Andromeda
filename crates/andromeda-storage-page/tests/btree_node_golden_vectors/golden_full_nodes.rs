use crate::support::{PAGE_SIZE, encode_node, internal_node, leaf_entry, leaf_node};
use andromeda_storage_page::{BTREE_NODE_V1_HEADER_LEN, BTreeNodeKindV1, BTreeNodeV1, PageId};

const FNV64_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01B3;

fn full_image_digest(encoded: &[u8]) -> u64 {
    encoded.iter().fold(FNV64_OFFSET, |state, byte| {
        (state ^ u64::from(*byte)).wrapping_mul(FNV64_PRIME)
    })
}

#[test]
fn non_empty_leaf_full_node_golden_vector_is_stable() {
    let node = leaf_node(
        150,
        22,
        vec![leaf_entry(b"aa", 100), leaf_entry(b"bb", 101)],
        Some(149),
        Some(151),
    );
    let encoded = encode_node(&node);

    assert_eq!(encoded.len(), usize::from(PAGE_SIZE));
    assert_eq!(full_image_digest(&encoded), 0x465F_4251_7D74_E029);
    assert_eq!(&encoded[0..4], &[0x41, 0x4E, 0x42, 0x54]);
    assert_eq!(encoded[6], 1);
    assert_eq!(&encoded[8..16], &150u64.to_le_bytes());
    assert_eq!(&encoded[16..24], &22u64.to_le_bytes());
    assert_eq!(&encoded[24..26], &2u16.to_le_bytes());
    assert_eq!(&encoded[26..28], &0u16.to_le_bytes());
    assert_eq!(&encoded[28..30], &92u16.to_le_bytes());
    assert_eq!(&encoded[30..32], &4096u16.to_le_bytes());
    assert_eq!(&encoded[32..40], &149u64.to_le_bytes());
    assert_eq!(&encoded[40..48], &151u64.to_le_bytes());
    assert_eq!(&encoded[48..50], &68u16.to_le_bytes());
    assert_eq!(&encoded[52..56], &0x745F_53B8u32.to_le_bytes());

    assert_eq!(
        &encoded[BTREE_NODE_V1_HEADER_LEN..68],
        &[0x02, 0x00, b'a', b'a']
    );
    assert_eq!(&encoded[68..78], &[0x08, 0x00, 100, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&encoded[78..82], &[0x02, 0x00, b'b', b'b']);
    assert_eq!(&encoded[82..92], &[0x08, 0x00, 101, 0, 0, 0, 0, 0, 0, 0]);
    assert!(encoded[92..].iter().all(|byte| *byte == 0));

    let decoded = BTreeNodeV1::decode(&encoded).expect("non-empty leaf golden image decodes");
    assert_eq!(decoded, node);
    assert_eq!(decoded.header.node_kind, BTreeNodeKindV1::Leaf);
    assert_eq!(decoded.header.high_key_offset, Some(68));
    assert_eq!(
        BTreeNodeV1::decode_for_page_id(&encoded, PageId::new(150)).expect("leaf page id matches"),
        node
    );
}

#[test]
fn non_empty_internal_full_node_golden_vector_is_stable() {
    let node = internal_node(
        160,
        23,
        vec![b"k10".to_vec(), b"k20".to_vec()],
        vec![200, 201, 202],
    );
    let encoded = encode_node(&node);

    assert_eq!(encoded.len(), usize::from(PAGE_SIZE));
    assert_eq!(full_image_digest(&encoded), 0x9F68_2E5C_FB34_99FF);
    assert_eq!(&encoded[0..4], &[0x41, 0x4E, 0x42, 0x54]);
    assert_eq!(encoded[6], 2);
    assert_eq!(&encoded[8..16], &160u64.to_le_bytes());
    assert_eq!(&encoded[16..24], &23u64.to_le_bytes());
    assert_eq!(&encoded[24..26], &2u16.to_le_bytes());
    assert_eq!(&encoded[26..28], &3u16.to_le_bytes());
    assert_eq!(&encoded[28..30], &98u16.to_le_bytes());
    assert_eq!(&encoded[30..32], &4096u16.to_le_bytes());
    assert_eq!(&encoded[32..40], &0u64.to_le_bytes());
    assert_eq!(&encoded[40..48], &0u64.to_le_bytes());
    assert_eq!(&encoded[48..50], &93u16.to_le_bytes());
    assert_eq!(&encoded[52..56], &0xC169_6DAAu32.to_le_bytes());

    assert_eq!(
        &encoded[BTREE_NODE_V1_HEADER_LEN..72],
        &200u64.to_le_bytes()
    );
    assert_eq!(&encoded[72..80], &201u64.to_le_bytes());
    assert_eq!(&encoded[80..88], &202u64.to_le_bytes());
    assert_eq!(&encoded[88..93], &[0x03, 0x00, b'k', b'1', b'0']);
    assert_eq!(&encoded[93..98], &[0x03, 0x00, b'k', b'2', b'0']);
    assert!(encoded[98..].iter().all(|byte| *byte == 0));

    let decoded = BTreeNodeV1::decode(&encoded).expect("non-empty internal golden image decodes");
    assert_eq!(decoded, node);
    assert_eq!(decoded.header.node_kind, BTreeNodeKindV1::Internal);
    assert_eq!(decoded.header.high_key_offset, Some(93));
    assert_eq!(
        decoded.child_page_ids,
        vec![PageId::new(200), PageId::new(201), PageId::new(202)]
    );
    assert_eq!(
        BTreeNodeV1::decode_for_page_id(&encoded, PageId::new(160))
            .expect("internal page id matches"),
        node
    );
}
