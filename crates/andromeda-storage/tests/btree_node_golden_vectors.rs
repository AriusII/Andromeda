use andromeda_storage::{
    BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN, BTREE_NODE_V1_MAGIC, BTreeNodeKindV1,
    BTreeNodeV1, Lsn, PageId,
};

#[test]
fn empty_leaf_golden_header_vector_is_stable() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(42),
        Lsn::new(7),
        Vec::new(),
        Some(PageId::new(41)),
        Some(PageId::new(43)),
        4096,
    )
    .expect("empty leaf encodes");

    let encoded = node.encode().expect("leaf image encodes");
    const EMPTY_LEAF_HEADER: [u8; BTREE_NODE_V1_HEADER_LEN] = [
        0x41, 0x4E, 0x42, 0x54, 0x01, 0x00, 0x01, 0x00, 0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00,
        0x00, 0x10, 0x29, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2B, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0F, 0xDA, 0x64, 0x1E, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(encoded.len(), 4096);
    assert_eq!(&encoded[..BTREE_NODE_V1_HEADER_LEN], &EMPTY_LEAF_HEADER);
    assert_eq!(&encoded[0..4], &BTREE_NODE_V1_MAGIC.to_le_bytes());
    assert_eq!(&encoded[4..6], &BTREE_NODE_V1_FORMAT_VERSION.to_le_bytes());
    assert_eq!(encoded[6], 1);
    assert_eq!(&encoded[8..16], &42u64.to_le_bytes());
    assert_eq!(&encoded[16..24], &7u64.to_le_bytes());
    assert_eq!(&encoded[24..26], &0u16.to_le_bytes());
    assert_eq!(&encoded[26..28], &0u16.to_le_bytes());
    assert_eq!(
        &encoded[28..30],
        &(BTREE_NODE_V1_HEADER_LEN as u16).to_le_bytes()
    );
    assert_eq!(&encoded[30..32], &4096u16.to_le_bytes());
    assert_eq!(&encoded[32..40], &41u64.to_le_bytes());
    assert_eq!(&encoded[40..48], &43u64.to_le_bytes());
    assert_ne!(&encoded[52..56], &0u32.to_le_bytes());

    let decoded = BTreeNodeV1::decode(&encoded).expect("golden leaf decodes");
    assert_eq!(decoded, node);
    assert_eq!(decoded.header.node_kind, BTreeNodeKindV1::Leaf);
    assert_eq!(
        BTreeNodeV1::decode_for_page_id(&encoded, PageId::new(42)).expect("page id matches"),
        node
    );
}

#[test]
fn leaf_with_multiple_keys_roundtrips_and_high_key_is_inside_body() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(50),
        Lsn::new(8),
        vec![
            (b"a".to_vec(), 10u64.to_le_bytes().to_vec()),
            (b"b".to_vec(), 11u64.to_le_bytes().to_vec()),
            (b"c".to_vec(), 12u64.to_le_bytes().to_vec()),
        ],
        None,
        Some(PageId::new(51)),
        4096,
    )
    .expect("leaf encodes");

    let encoded = node.encode().expect("leaf image encodes");
    let decoded = BTreeNodeV1::decode(&encoded).expect("leaf decodes");
    assert_eq!(
        decoded.keys,
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
    );
    assert_eq!(decoded.leaf_values.len(), 3);
    let high_key_offset = decoded
        .header
        .high_key_offset
        .expect("multi-key leaf carries high-key offset");
    assert!(high_key_offset >= BTREE_NODE_V1_HEADER_LEN as u16);
    assert!(high_key_offset < decoded.header.free_start);
    assert_eq!(decoded.key_count(), 3);
    assert_eq!(decoded.encoded_len(), decoded.header.free_start as usize);
}

#[test]
fn internal_node_roundtrips_children_before_keys() {
    let node = BTreeNodeV1::new_internal(
        PageId::new(70),
        Lsn::new(9),
        vec![b"k10".to_vec(), b"k20".to_vec()],
        vec![PageId::new(100), PageId::new(101), PageId::new(102)],
        4096,
    )
    .expect("internal node encodes");

    let encoded = node.encode().expect("internal image encodes");
    assert_eq!(encoded[6], 2);
    assert_eq!(&encoded[26..28], &3u16.to_le_bytes());
    assert_eq!(
        &encoded[BTREE_NODE_V1_HEADER_LEN..BTREE_NODE_V1_HEADER_LEN + 8],
        &100u64.to_le_bytes()
    );

    let decoded = BTreeNodeV1::decode(&encoded).expect("internal image decodes");
    assert_eq!(decoded.header.node_kind, BTreeNodeKindV1::Internal);
    assert_eq!(
        decoded.child_page_ids,
        vec![PageId::new(100), PageId::new(101), PageId::new(102)]
    );
    assert_eq!(decoded.keys, vec![b"k10".to_vec(), b"k20".to_vec()]);
}

#[test]
fn invalid_key_order_is_rejected_before_persistence() {
    let err = BTreeNodeV1::new_leaf(
        PageId::new(80),
        Lsn::new(10),
        vec![
            (b"b".to_vec(), 10u64.to_le_bytes().to_vec()),
            (b"a".to_vec(), 11u64.to_le_bytes().to_vec()),
        ],
        None,
        None,
        4096,
    )
    .expect_err("descending keys must be rejected");
    assert!(err.message().contains("strictly ordered"));
}

#[test]
fn invalid_magic_is_rejected_from_decoded_header() {
    let node = BTreeNodeV1::new_leaf(PageId::new(82), Lsn::new(10), Vec::new(), None, None, 4096)
        .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[0..4].copy_from_slice(&0u32.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("invalid magic must be rejected");
    assert!(err.message().contains("magic"));
}

#[test]
fn unsupported_version_is_rejected_from_decoded_header() {
    let node = BTreeNodeV1::new_leaf(PageId::new(83), Lsn::new(10), Vec::new(), None, None, 4096)
        .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[4..6].copy_from_slice(&2u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("unsupported version must be rejected");
    assert!(err.message().contains("format version"));
}

#[test]
fn magic_is_checked_before_secondary_header_fields() {
    let node = BTreeNodeV1::new_leaf(PageId::new(830), Lsn::new(10), Vec::new(), None, None, 4096)
        .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[0..4].copy_from_slice(&BTREE_NODE_V1_MAGIC.to_be_bytes());
    encoded[6] = 0xFF;

    let err = BTreeNodeV1::decode(&encoded).expect_err("wrong-endian magic must be rejected");
    assert!(err.message().contains("magic"));
}

#[test]
fn reserved_header_bytes_are_rejected_from_decoded_header() {
    let node = BTreeNodeV1::new_leaf(PageId::new(831), Lsn::new(10), Vec::new(), None, None, 4096)
        .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[7] = 1;
    let err = BTreeNodeV1::decode(&encoded).expect_err("reserved byte must be rejected");
    assert!(err.message().contains("reserved header byte"));

    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[50..52].copy_from_slice(&1u16.to_le_bytes());
    let err = BTreeNodeV1::decode(&encoded).expect_err("reserved field must be rejected");
    assert!(err.message().contains("reserved header field"));
}

#[test]
fn impossible_leaf_key_count_is_rejected_before_body_decode() {
    let node = BTreeNodeV1::new_leaf(PageId::new(832), Lsn::new(10), Vec::new(), None, None, 4096)
        .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[24..26].copy_from_slice(&1u16.to_le_bytes());
    refresh_btree_header_crc(&mut encoded);

    let err = BTreeNodeV1::decode(&encoded).expect_err("body length gate must reject key count");
    assert!(err.message().contains("key_count"));
    assert!(err.message().contains("encoded body length"));
}

#[test]
fn page_id_mismatch_is_rejected_against_physical_page_id() {
    let node = BTreeNodeV1::new_leaf(PageId::new(84), Lsn::new(10), Vec::new(), None, None, 4096)
        .expect("leaf encodes");
    let encoded = node.encode().expect("leaf image encodes");

    let err = BTreeNodeV1::decode_for_page_id(&encoded, PageId::new(85))
        .expect_err("physical page id mismatch must be rejected");
    assert!(err.message().contains("expected page id"));

    let err = BTreeNodeV1::decode_for_page_id(&encoded, PageId::new(0))
        .expect_err("zero expected page id must be rejected");
    assert!(err.message().contains("must not be zero"));
}

#[test]
fn duplicate_keys_are_rejected_from_decoded_body() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(81),
        Lsn::new(10),
        vec![
            (b"a".to_vec(), 10u64.to_le_bytes().to_vec()),
            (b"b".to_vec(), 11u64.to_le_bytes().to_vec()),
        ],
        None,
        None,
        4096,
    )
    .expect("ordered leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    let second_key_byte_offset = BTREE_NODE_V1_HEADER_LEN + 2 + 1 + 2 + 8 + 2;
    encoded[second_key_byte_offset] = b'a';

    let err = BTreeNodeV1::decode(&encoded).expect_err("duplicate decoded keys must be rejected");
    assert!(err.message().contains("strictly ordered"));
}

#[test]
fn invalid_leaf_child_count_is_rejected_from_decoded_header() {
    let node = BTreeNodeV1::new_leaf(PageId::new(86), Lsn::new(10), Vec::new(), None, None, 4096)
        .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[26..28].copy_from_slice(&1u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("leaf child count must be rejected");
    assert!(err.message().contains("child pointers"));
}

#[test]
fn invalid_internal_child_count_is_rejected_before_persistence() {
    let err = BTreeNodeV1::new_internal(
        PageId::new(90),
        Lsn::new(11),
        vec![b"k10".to_vec(), b"k20".to_vec()],
        vec![PageId::new(100), PageId::new(101)],
        4096,
    )
    .expect_err("internal node requires keys + 1 children");
    assert!(err.message().contains("keys + 1"));
}

#[test]
fn internal_zero_child_pointer_is_rejected_from_decoded_body() {
    let node = BTreeNodeV1::new_internal(
        PageId::new(92),
        Lsn::new(13),
        vec![b"k10".to_vec()],
        vec![PageId::new(100), PageId::new(101)],
        4096,
    )
    .expect("internal node encodes");
    let mut encoded = node.encode().expect("internal image encodes");
    encoded[BTREE_NODE_V1_HEADER_LEN..BTREE_NODE_V1_HEADER_LEN + 8]
        .copy_from_slice(&0u64.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("zero child pointer must be rejected");
    assert!(err.message().contains("child page ids must not be zero"));
}

#[test]
fn invalid_internal_child_count_is_rejected_from_decoded_header() {
    let node = BTreeNodeV1::new_internal(
        PageId::new(87),
        Lsn::new(13),
        vec![b"k10".to_vec()],
        vec![PageId::new(100), PageId::new(101)],
        4096,
    )
    .expect("internal node encodes");
    let mut encoded = node.encode().expect("internal image encodes");
    encoded[26..28].copy_from_slice(&1u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("bad child count must be rejected");
    assert!(err.message().contains("key_count + 1"));
}

#[test]
fn leaf_self_sibling_links_are_rejected_before_persistence() {
    let err = BTreeNodeV1::new_leaf(
        PageId::new(93),
        Lsn::new(14),
        Vec::new(),
        Some(PageId::new(93)),
        None,
        4096,
    )
    .expect_err("leaf must not link to itself");
    assert!(err.message().contains("point to self"));

    let err = BTreeNodeV1::new_leaf(
        PageId::new(94),
        Lsn::new(15),
        Vec::new(),
        None,
        Some(PageId::new(94)),
        4096,
    )
    .expect_err("leaf must not link to itself");
    assert!(err.message().contains("point to self"));
}

#[test]
fn leaf_sibling_pointer_violations_are_rejected_from_decoded_header() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(88),
        Lsn::new(14),
        Vec::new(),
        Some(PageId::new(87)),
        Some(PageId::new(89)),
        4096,
    )
    .expect("leaf encodes");

    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[32..40].copy_from_slice(&88u64.to_le_bytes());
    let err = BTreeNodeV1::decode(&encoded).expect_err("self sibling link must be rejected");
    assert!(err.message().contains("point to self"));

    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[32..40].copy_from_slice(&90u64.to_le_bytes());
    encoded[40..48].copy_from_slice(&90u64.to_le_bytes());
    let err = BTreeNodeV1::decode(&encoded).expect_err("duplicate sibling links must be rejected");
    assert!(err.message().contains("must differ"));
}

#[test]
fn internal_sibling_pointers_are_rejected_from_decoded_header() {
    let node = BTreeNodeV1::new_internal(
        PageId::new(89),
        Lsn::new(15),
        vec![b"k10".to_vec()],
        vec![PageId::new(100), PageId::new(101)],
        4096,
    )
    .expect("internal node encodes");
    let mut encoded = node.encode().expect("internal image encodes");
    encoded[32..40].copy_from_slice(&88u64.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("internal sibling link must be rejected");
    assert!(err.message().contains("internal nodes"));
}

#[test]
fn high_key_offset_must_match_encoded_keys() {
    let mut node = BTreeNodeV1::new_leaf(
        PageId::new(95),
        Lsn::new(16),
        vec![
            (b"a".to_vec(), 10u64.to_le_bytes().to_vec()),
            (b"b".to_vec(), 11u64.to_le_bytes().to_vec()),
            (b"c".to_vec(), 12u64.to_le_bytes().to_vec()),
        ],
        None,
        None,
        4096,
    )
    .expect("leaf encodes");
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

#[test]
fn truncated_length_prefixed_key_is_rejected() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(96),
        Lsn::new(17),
        vec![(b"a".to_vec(), 10u64.to_le_bytes().to_vec())],
        None,
        None,
        4096,
    )
    .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[BTREE_NODE_V1_HEADER_LEN..BTREE_NODE_V1_HEADER_LEN + 2]
        .copy_from_slice(&4096u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("truncated key must be rejected");
    assert!(err.message().contains("free_start"));
}

#[test]
fn truncated_length_prefixed_value_is_rejected() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(97),
        Lsn::new(18),
        vec![(b"a".to_vec(), 10u64.to_le_bytes().to_vec())],
        None,
        None,
        4096,
    )
    .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    let value_len_offset = BTREE_NODE_V1_HEADER_LEN + 2 + 1;
    encoded[value_len_offset..value_len_offset + 2].copy_from_slice(&4096u16.to_le_bytes());

    let err = BTreeNodeV1::decode(&encoded).expect_err("truncated value must be rejected");
    assert!(err.message().contains("free_start"));
}

#[test]
fn header_crc_detects_corrupted_node_header() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(91),
        Lsn::new(12),
        vec![(b"a".to_vec(), 10u64.to_le_bytes().to_vec())],
        None,
        None,
        4096,
    )
    .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[8] ^= 0xFF;

    let err = BTreeNodeV1::decode(&encoded).expect_err("corrupted header must be rejected");
    assert!(err.message().contains("CRC") || err.message().contains("page_id"));
}

#[test]
fn header_crc_field_corruption_is_rejected() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(98),
        Lsn::new(19),
        vec![(b"a".to_vec(), 10u64.to_le_bytes().to_vec())],
        None,
        None,
        4096,
    )
    .expect("leaf encodes");
    let mut encoded = node.encode().expect("leaf image encodes");
    encoded[52] ^= 0xAA;

    let err = BTreeNodeV1::decode(&encoded).expect_err("corrupted CRC must be rejected");
    assert!(err.message().contains("CRC"));
}

#[test]
fn leaf_sibling_links_reencode_stably() {
    let node = BTreeNodeV1::new_leaf(
        PageId::new(99),
        Lsn::new(20),
        vec![
            (b"a".to_vec(), 10u64.to_le_bytes().to_vec()),
            (b"z".to_vec(), 11u64.to_le_bytes().to_vec()),
        ],
        Some(PageId::new(98)),
        Some(PageId::new(100)),
        4096,
    )
    .expect("leaf encodes");

    let encoded = node.encode().expect("leaf image encodes");
    let decoded = BTreeNodeV1::decode(&encoded).expect("leaf decodes");
    assert_eq!(decoded.header.prev_leaf, Some(PageId::new(98)));
    assert_eq!(decoded.header.next_leaf, Some(PageId::new(100)));
    assert_eq!(decoded.key_count(), 2);
    assert_eq!(decoded.encode().expect("leaf re-encodes"), encoded);
}

#[test]
fn internal_children_reencode_stably() {
    let node = BTreeNodeV1::new_internal(
        PageId::new(120),
        Lsn::new(21),
        vec![b"k10".to_vec(), b"k20".to_vec(), b"k30".to_vec()],
        vec![
            PageId::new(200),
            PageId::new(201),
            PageId::new(202),
            PageId::new(203),
        ],
        4096,
    )
    .expect("internal node encodes");

    let encoded = node.encode().expect("internal image encodes");
    let decoded = BTreeNodeV1::decode(&encoded).expect("internal image decodes");
    assert_eq!(
        decoded.child_page_ids,
        vec![
            PageId::new(200),
            PageId::new(201),
            PageId::new(202),
            PageId::new(203)
        ]
    );
    assert_eq!(decoded.key_count(), 3);
    assert_eq!(decoded.encode().expect("internal re-encodes"), encoded);
}

fn refresh_btree_header_crc(encoded: &mut [u8]) {
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
