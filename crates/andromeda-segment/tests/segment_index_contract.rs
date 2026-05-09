use andromeda_segment::segment_index::{
    SEGMENT_INDEX_V0_BYTE_ORDER, SEGMENT_INDEX_V0_ENTRY_LEN, SEGMENT_INDEX_V0_FORMAT_MAJOR,
    SEGMENT_INDEX_V0_FORMAT_MINOR, SEGMENT_INDEX_V0_HEADER_LEN, SEGMENT_INDEX_V0_MAGIC,
    SEGMENT_INDEX_V0_TRAILER_LEN, SegmentIndexBuildContextV0, SegmentIndexEntryV0,
    SegmentIndexError, SegmentIndexV0,
};
use andromeda_segment::{AllocationId, ExtentId, Lsn, ObjectId, PageId, PageSize, SegmentId};

fn context(snapshot_id: u64) -> SegmentIndexBuildContextV0 {
    SegmentIndexBuildContextV0 {
        database_id: 10,
        snapshot_id,
        segment_index_id: 20,
        manifest_version: 30,
        base_checkpoint_lsn: Lsn::new(40),
        required_wal_start_lsn: Lsn::new(41),
        parent_manifest_hash: [0xD0; 32],
    }
}

fn entry(segment_id: u64, first_page_id: u64, digest_byte: u8) -> SegmentIndexEntryV0 {
    SegmentIndexEntryV0::new(
        SegmentId::new(segment_id),
        ObjectId::new(1000 + segment_id),
        AllocationId::new(2000 + segment_id),
        ExtentId::new(3000 + segment_id),
        1,
        PageSize::KiB16,
        PageId::new(first_page_id),
        2,
        Lsn::new(5000 + segment_id),
        Lsn::new(6000 + segment_id),
        44,
        7000 + segment_id,
        0,
        16 * 1024,
        8000 + segment_id,
        [digest_byte; 32],
        9000 + segment_id as u32,
        10_000 + segment_id as u32,
    )
    .expect("entry is valid")
}

fn index() -> SegmentIndexV0 {
    SegmentIndexV0::new(
        context(44),
        vec![entry(7, 100, 0xA7), entry(8, 102, 0xA8)],
        Vec::new(),
    )
    .expect("segment index is valid")
}

#[test]
fn encode_decode_round_trip_is_canonical_little_endian() {
    let index = index();

    let encoded = index.encode().expect("segment index encodes");
    assert_eq!(
        encoded.len(),
        SEGMENT_INDEX_V0_HEADER_LEN + 2 * SEGMENT_INDEX_V0_ENTRY_LEN + SEGMENT_INDEX_V0_TRAILER_LEN
    );
    assert_eq!(&encoded[0..8], &SEGMENT_INDEX_V0_MAGIC);
    assert_eq!(
        &encoded[8..10],
        &SEGMENT_INDEX_V0_FORMAT_MAJOR.to_le_bytes()
    );
    assert_eq!(
        &encoded[10..12],
        &SEGMENT_INDEX_V0_FORMAT_MINOR.to_le_bytes()
    );
    assert_eq!(&encoded[12..14], &SEGMENT_INDEX_V0_BYTE_ORDER.to_le_bytes());
    assert_eq!(
        &encoded[14..16],
        &(SEGMENT_INDEX_V0_HEADER_LEN as u16).to_le_bytes()
    );
    assert_eq!(&encoded[16..24], &(encoded.len() as u64).to_le_bytes());
    assert_eq!(
        &encoded[80..88],
        &(SEGMENT_INDEX_V0_HEADER_LEN as u64).to_le_bytes()
    );
    assert_eq!(&encoded[88..96], &2u64.to_le_bytes());
    assert_eq!(
        &encoded[96..100],
        &(SEGMENT_INDEX_V0_ENTRY_LEN as u32).to_le_bytes()
    );

    let first_entry = SEGMENT_INDEX_V0_HEADER_LEN;
    assert_eq!(&encoded[first_entry..first_entry + 8], &7u64.to_le_bytes());
    assert_eq!(
        &encoded[first_entry + 8..first_entry + 16],
        &1007u64.to_le_bytes()
    );
    assert_eq!(
        &encoded[first_entry + 36..first_entry + 38],
        &1u16.to_le_bytes()
    );
    assert_eq!(
        &encoded[first_entry + 38..first_entry + 40],
        &3u16.to_le_bytes()
    );
    assert_eq!(
        &encoded[first_entry + 40..first_entry + 48],
        &100u64.to_le_bytes()
    );
    assert_eq!(
        &encoded[first_entry + 48..first_entry + 52],
        &2u32.to_le_bytes()
    );
    assert_eq!(
        &encoded[first_entry + 52..first_entry + 56],
        &0u32.to_le_bytes()
    );

    let decoded = SegmentIndexV0::decode(&encoded).expect("segment index decodes");
    assert_eq!(decoded, index);
    assert_eq!(decoded.encode().expect("decoded index re-encodes"), encoded);
}

#[test]
fn new_rejects_non_monotonic_segment_ids() {
    let err = SegmentIndexV0::new(
        context(44),
        vec![entry(9, 100, 0xA9), entry(8, 102, 0xA8)],
        Vec::new(),
    )
    .expect_err("segment ids must be monotonic");

    assert_eq!(
        err,
        SegmentIndexError::InvalidOrdering {
            index: 1,
            reason: "segment ids must be strictly increasing",
        }
    );
}

#[test]
fn new_rejects_overlapping_page_ranges() {
    let err = SegmentIndexV0::new(
        context(44),
        vec![entry(7, 100, 0xA7), entry(8, 101, 0xA8)],
        Vec::new(),
    )
    .expect_err("page ranges must not overlap");

    assert_eq!(
        err,
        SegmentIndexError::InvalidOrdering {
            index: 1,
            reason: "page ranges must be strictly increasing and non-overlapping",
        }
    );
}

#[test]
fn new_rejects_snapshot_mismatch() {
    let err = SegmentIndexV0::new(context(45), vec![entry(7, 100, 0xA7)], Vec::new())
        .expect_err("entry snapshot id must match header snapshot id");

    assert_eq!(
        err,
        SegmentIndexError::InvalidEntry {
            index: 0,
            reason: "entry snapshot id must match header snapshot id",
        }
    );
}

#[test]
fn entry_rejects_page_range_overflow() {
    let err = SegmentIndexEntryV0::new(
        SegmentId::new(7),
        ObjectId::new(1007),
        AllocationId::new(2007),
        ExtentId::new(3007),
        1,
        PageSize::KiB16,
        PageId::new(u64::MAX),
        2,
        Lsn::new(5007),
        Lsn::new(6007),
        44,
        7007,
        0,
        16 * 1024,
        8007,
        [0xA7; 32],
        9007,
        10_007,
    )
    .expect_err("overflowing page ranges are invalid");

    assert_eq!(
        err,
        SegmentIndexError::InvalidEntry {
            index: 0,
            reason: "page range overflows u64",
        }
    );
}

#[test]
fn decode_rejects_truncated_header() {
    let encoded = index().encode().expect("segment index encodes");

    let err = SegmentIndexV0::decode(&encoded[..SEGMENT_INDEX_V0_HEADER_LEN - 1])
        .expect_err("truncated header must fail");

    assert_eq!(
        err,
        SegmentIndexError::Truncated {
            field: "segment index file",
        }
    );
}

#[test]
fn decode_rejects_truncated_trailer() {
    let encoded = index().encode().expect("segment index encodes");

    let err = SegmentIndexV0::decode(&encoded[..encoded.len() - 1])
        .expect_err("truncated trailer must fail");

    assert_eq!(
        err,
        SegmentIndexError::InvalidHeader {
            reason: "total length does not match input length",
        }
    );
}

#[test]
fn decode_rejects_unsupported_version() {
    let mut encoded = index().encode().expect("segment index encodes");
    encoded[8..10].copy_from_slice(&2u16.to_le_bytes());

    let err = SegmentIndexV0::decode(&encoded).expect_err("unknown version must fail");

    assert_eq!(
        err,
        SegmentIndexError::UnsupportedVersion { major: 2, minor: 0 }
    );
}

#[test]
fn decode_rejects_corrupted_header_checksum() {
    let mut encoded = index().encode().expect("segment index encodes");
    encoded[24] ^= 0xFF;

    let err = SegmentIndexV0::decode(&encoded).expect_err("header checksum corruption must fail");

    assert_eq!(err, SegmentIndexError::HeaderChecksumMismatch);
}

#[test]
fn decode_rejects_corrupted_entry_checksum() {
    let mut encoded = index().encode().expect("segment index encodes");
    encoded[SEGMENT_INDEX_V0_HEADER_LEN + 112] ^= 0xFF;

    let err = SegmentIndexV0::decode(&encoded).expect_err("entry corruption must fail");

    assert_eq!(err, SegmentIndexError::EntryChecksumMismatch { index: 0 });
}

#[test]
fn decode_rejects_reserved_entry_bytes() {
    let mut encoded = index().encode().expect("segment index encodes");
    let reserved_offset = SEGMENT_INDEX_V0_HEADER_LEN + 52;
    encoded[reserved_offset..reserved_offset + 4].copy_from_slice(&1u32.to_le_bytes());

    let err = SegmentIndexV0::decode(&encoded).expect_err("reserved entry bytes must fail");

    assert_eq!(
        err,
        SegmentIndexError::InvalidEntry {
            index: 0,
            reason: "reserved entry field must be zero",
        }
    );
}
