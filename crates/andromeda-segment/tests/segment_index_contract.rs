use andromeda_segment::segment_index::{
    COLUMN_CHUNK_DIRECTORY_ENTRY_LEN, COLUMN_CHUNK_DIRECTORY_EXT_FLAGS,
    COLUMN_CHUNK_DIRECTORY_EXT_TYPE, ColumnChunkDirectory, ColumnChunkDirectoryEntry,
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

// ── ColumnChunkDirectory codec tests ─────────────────────────────────────────

/// Returns a canonical two-entry `ColumnChunkDirectory` used in codec tests.
fn chunk_directory() -> ColumnChunkDirectory {
    ColumnChunkDirectory {
        entries: vec![
            ColumnChunkDirectoryEntry {
                ordinal: 1,
                column_id: 100,
                offset: 4096,
                length: 512,
                min_value_inline_or_offset: 0xAA00,
                max_value_inline_or_offset: 0xAA01,
                bloom_bytes_offset: 0,
                bloom_bytes_len: 0,
                source_snapshot_lsn: 65536,
            },
            ColumnChunkDirectoryEntry {
                ordinal: 2,
                column_id: 101,
                offset: 8192,
                length: 1024,
                min_value_inline_or_offset: 0xBB00,
                max_value_inline_or_offset: 0xBB01,
                bloom_bytes_offset: 12288,
                bloom_bytes_len: 256,
                source_snapshot_lsn: 65536,
            },
        ],
    }
}

/// Frozen little-endian byte vector for `chunk_directory()`.
///
/// Layout:
/// ```text
/// [0..2]   0x10 0x00       ext_type  = COLUMN_CHUNK_DIRECTORY_EXT_TYPE (0x0010)
/// [2..4]   0x00 0x00       ext_flags = 0x0000 (optional)
/// [4..8]   0x84 0x00 0x00 0x00  payload_len = 132 (4 + 2×64)
/// [8..12]  0x02 0x00 0x00 0x00  chunk_count = 2
///
/// Entry 0 — ordinal=1, column_id=100, offset=4096, length=512,
///            min=0xAA00, max=0xAA01, bloom_off=0, bloom_len=0, lsn=65536
/// [12..76]  64 bytes (little-endian)
///
/// Entry 1 — ordinal=2, column_id=101, offset=8192, length=1024,
///            min=0xBB00, max=0xBB01, bloom_off=12288, bloom_len=256, lsn=65536
/// [76..140] 64 bytes (little-endian)
/// ```
const COLUMN_CHUNK_DIRECTORY_GOLDEN: &[u8] = &[
    // ── Extension header ──────────────────────────────────────────────────
    0x10, 0x00, // ext_type  = 0x0010
    0x00, 0x00, // ext_flags = 0x0000
    0x84, 0x00, 0x00, 0x00, // payload_len = 132
    // ── chunk_count ───────────────────────────────────────────────────────
    0x02, 0x00, 0x00, 0x00, // chunk_count = 2
    // ── Entry 0 ───────────────────────────────────────────────────────────
    0x01, 0x00, 0x00, 0x00, // ordinal = 1
    0x64, 0x00, 0x00, 0x00, // column_id = 100
    0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // offset = 4096
    0x00, 0x02, 0x00, 0x00, // length = 512
    0x00, 0xAA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // min = 0xAA00
    0x01, 0xAA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // max = 0xAA01
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // bloom_bytes_offset = 0
    0x00, 0x00, 0x00, 0x00, // bloom_bytes_len = 0
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, // source_snapshot_lsn = 65536
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // reserved = 0
    // ── Entry 1 ───────────────────────────────────────────────────────────
    0x02, 0x00, 0x00, 0x00, // ordinal = 2
    0x65, 0x00, 0x00, 0x00, // column_id = 101
    0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // offset = 8192
    0x00, 0x04, 0x00, 0x00, // length = 1024
    0x00, 0xBB, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // min = 0xBB00
    0x01, 0xBB, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // max = 0xBB01
    0x00, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // bloom_bytes_offset = 12288
    0x00, 0x01, 0x00, 0x00, // bloom_bytes_len = 256
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, // source_snapshot_lsn = 65536
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // reserved = 0
];

/// Golden encode/decode round-trip test for `ColumnChunkDirectory`.
///
/// Verifies:
/// - The codec emits exactly the frozen byte vector above (explicit
///   little-endian, no native struct dump).
/// - Decoding the golden bytes reproduces the original struct.
/// - Re-encoding the decoded struct yields byte-identical output.
#[test]
fn column_chunk_directory_encode_decode_roundtrip_is_byte_stable() {
    let dir = chunk_directory();

    // ── Encode ──────────────────────────────────────────────────────────────
    let encoded = dir
        .encode_as_extension()
        .expect("directory must encode without error");

    // Extension header: total = 8-byte header + 132-byte payload = 140 bytes.
    assert_eq!(
        encoded.len(),
        8 + 4 + 2 * COLUMN_CHUNK_DIRECTORY_ENTRY_LEN,
        "encoded length must be 140 bytes"
    );

    // Extension type and flags (wire header).
    assert_eq!(
        &encoded[0..2],
        &COLUMN_CHUNK_DIRECTORY_EXT_TYPE.to_le_bytes(),
        "ext_type must be COLUMN_CHUNK_DIRECTORY_EXT_TYPE in LE"
    );
    assert_eq!(
        &encoded[2..4],
        &COLUMN_CHUNK_DIRECTORY_EXT_FLAGS.to_le_bytes(),
        "ext_flags must be 0x0000 (optional)"
    );
    // payload_len = 132
    assert_eq!(
        &encoded[4..8],
        &132u32.to_le_bytes(),
        "payload_len must be 132"
    );

    // Frozen byte-exact comparison — this is the acceptance gate for the codec.
    assert_eq!(
        encoded.as_slice(),
        COLUMN_CHUNK_DIRECTORY_GOLDEN,
        "encoded bytes must match the frozen golden vector"
    );

    // ── Decode ──────────────────────────────────────────────────────────────
    let decoded = ColumnChunkDirectory::decode_from_extension_bytes(&encoded)
        .expect("decode must not error")
        .expect("directory record must be present");

    assert_eq!(decoded, dir, "decoded directory must equal original");

    // source_snapshot_lsn values must survive the round-trip.
    for entry in &decoded.entries {
        assert_eq!(
            entry.source_snapshot_lsn, 65536,
            "source_snapshot_lsn must survive encode→decode"
        );
    }

    // ── Re-encode stability ─────────────────────────────────────────────────
    let re_encoded = decoded
        .encode_as_extension()
        .expect("re-encode must not error");
    assert_eq!(
        re_encoded, encoded,
        "re-encoded bytes must be byte-identical (no drifting codec)"
    );
}

/// Crash-recovery test: `ColumnChunkDirectory` snapshot binding survives crash replay.
///
/// Simulates the scenario where:
/// 1. A writer builds a `SegmentIndexV0` that embeds a `ColumnChunkDirectory`
///    (with known `source_snapshot_lsn` values) and flushes it to disk.
/// 2. A crash occurs at various points during the write (modelled by truncating
///    the encoded bytes).
/// 3. Recovery reads back the bytes.
///
/// Invariants verified:
/// - Truncated inputs return typed errors — no panics, no silent data loss.
/// - A complete (non-truncated) file survives encode→decode with all
///   `source_snapshot_lsn` values intact.
/// - Absent extension (no `ColumnChunkDirectory` record) returns `Ok(None)`.
#[test]
fn columnar_segment_snapshot_binding_survives_crash_replay() {
    let chunk_dir = chunk_directory();
    let ext_bytes = chunk_dir
        .encode_as_extension()
        .expect("directory must encode without error");

    // ── Part 1: Simulate crash during extension write (truncation scenarios) ─
    //
    // Each truncation point represents a crash at a different stage of the
    // disk write.  The decoder must reject all of them cleanly.

    // Crash mid-header (no payload_len yet)
    let err = ColumnChunkDirectory::decode_from_extension_bytes(&ext_bytes[..3])
        .expect_err("truncated extension header must be rejected");
    assert!(
        matches!(err, SegmentIndexError::Truncated { .. }),
        "truncated header must yield Truncated error, got: {err:?}"
    );

    // Crash after header, before chunk_count
    let err = ColumnChunkDirectory::decode_from_extension_bytes(&ext_bytes[..9])
        .expect_err("truncated payload (no chunk_count) must be rejected");
    assert!(
        matches!(err, SegmentIndexError::Truncated { .. }),
        "truncated payload must yield Truncated error, got: {err:?}"
    );

    // Crash mid first entry (header + chunk_count + partial entry)
    let err = ColumnChunkDirectory::decode_from_extension_bytes(&ext_bytes[..30])
        .expect_err("truncated first entry must be rejected");
    assert!(
        matches!(err, SegmentIndexError::Truncated { .. }),
        "truncated entry must yield Truncated error, got: {err:?}"
    );

    // Crash between entry 0 and entry 1 (header + 4 + 64 bytes = 76 bytes,
    // but payload_len in the header says 132 so the decoder expects more)
    let err = ColumnChunkDirectory::decode_from_extension_bytes(&ext_bytes[..76])
        .expect_err("truncated between entries must be rejected");
    assert!(
        matches!(err, SegmentIndexError::Truncated { .. }),
        "truncated between entries must yield Truncated error, got: {err:?}"
    );

    // ── Part 2: Absent extension returns Ok(None) ────────────────────────────
    let absent = ColumnChunkDirectory::decode_from_extension_bytes(&[])
        .expect("empty extension must not error");
    assert!(absent.is_none(), "absent extension must return Ok(None)");

    // ── Part 3: Full file round-trip — snapshot binding survives ─────────────
    let recovered = ColumnChunkDirectory::decode_from_extension_bytes(&ext_bytes)
        .expect("full extension bytes must decode without error")
        .expect("directory record must be present after recovery");

    assert_eq!(
        recovered.entries.len(),
        2,
        "both chunk entries must survive recovery"
    );
    // source_snapshot_lsn must be intact for both entries
    for (i, entry) in recovered.entries.iter().enumerate() {
        assert_eq!(
            entry.source_snapshot_lsn, 65536,
            "entry {i} source_snapshot_lsn must be intact after crash-recovery replay"
        );
    }
    // Structural fields must also match — no silent mutation
    assert_eq!(
        recovered.entries[0].ordinal, 1,
        "entry 0 ordinal must survive recovery"
    );
    assert_eq!(
        recovered.entries[0].column_id, 100,
        "entry 0 column_id must survive recovery"
    );
    assert_eq!(
        recovered.entries[1].ordinal, 2,
        "entry 1 ordinal must survive recovery"
    );
    assert_eq!(
        recovered.entries[1].bloom_bytes_len, 256,
        "entry 1 bloom_bytes_len must survive recovery"
    );

    // ── Part 4: Full SegmentIndexV0 with directory extension survives ─────────
    //
    // This verifies the full durable path: index → encode → decode (recovery)
    // → extract directory → source_snapshot_lsn intact.
    let full_index = SegmentIndexV0::new(
        context(44),
        vec![entry(7, 100, 0xA7), entry(8, 102, 0xA8)],
        ext_bytes.clone(),
    )
    .expect("segment index with directory extension must build");

    let full_encoded = full_index.encode().expect("full index must encode");
    let full_decoded = SegmentIndexV0::decode(&full_encoded)
        .expect("full encoded index must decode (crash-recovery simulation)");

    // The extension bytes must be preserved verbatim through the SegmentIndex codec.
    assert_eq!(
        full_decoded.extension_bytes, ext_bytes,
        "extension bytes must survive SegmentIndexV0 encode→decode"
    );

    // Re-extract the directory from the recovered extension.
    let recovered_dir =
        ColumnChunkDirectory::decode_from_extension_bytes(&full_decoded.extension_bytes)
            .expect("decode after SegmentIndexV0 recovery must not error")
            .expect("directory must still be present after full-index recovery");

    assert_eq!(
        recovered_dir, chunk_dir,
        "ColumnChunkDirectory must be structurally identical after crash-recovery"
    );
    for (i, recovered_entry) in recovered_dir.entries.iter().enumerate() {
        assert_eq!(
            recovered_entry.source_snapshot_lsn, 65536,
            "entry {i} source_snapshot_lsn must survive full SegmentIndexV0 crash-recovery"
        );
    }
}

/// Verify that the ColumnChunkDirectory extension is skipped (not rejected) by
/// older readers that do not know about type 0x0010.
///
/// Because `ext_flags = 0x0000` (not required), `validate_extension_bytes`
/// inside the existing SegmentIndexV0 codec must accept the bytes.
#[test]
fn column_chunk_directory_extension_is_tolerated_by_old_segment_index_reader() {
    let ext_bytes = chunk_directory()
        .encode_as_extension()
        .expect("directory must encode");

    // Build a SegmentIndexV0 with the directory extension.
    // If the existing validation logic rejects the unknown type, this panics.
    let idx = SegmentIndexV0::new(
        context(44),
        vec![entry(7, 100, 0xA7), entry(8, 102, 0xA8)],
        ext_bytes,
    )
    .expect("SegmentIndexV0 must accept optional extension record with unknown type");

    // Encode and re-decode must succeed end-to-end.
    let encoded = idx
        .encode()
        .expect("index with directory extension encodes");
    let decoded = SegmentIndexV0::decode(&encoded).expect("index with directory extension decodes");
    assert_eq!(
        decoded.header.extension_len, idx.header.extension_len,
        "extension length must round-trip through SegmentIndexV0"
    );
}

/// Verify that `ColumnChunkDirectory` encode rejects an empty entries list.
#[test]
fn column_chunk_directory_rejects_empty_entries() {
    let empty = ColumnChunkDirectory { entries: vec![] };
    let err = empty
        .encode_as_extension()
        .expect_err("empty directory must be rejected");
    assert!(
        matches!(err, SegmentIndexError::InvalidExtension { .. }),
        "empty directory must yield InvalidExtension, got: {err:?}"
    );
}

/// Verify that `ColumnChunkDirectory` rejects a zero `source_snapshot_lsn`.
#[test]
fn column_chunk_directory_rejects_zero_source_snapshot_lsn() {
    let dir = ColumnChunkDirectory {
        entries: vec![ColumnChunkDirectoryEntry {
            ordinal: 0,
            column_id: 1,
            offset: 0,
            length: 64,
            min_value_inline_or_offset: 0,
            max_value_inline_or_offset: 0,
            bloom_bytes_offset: 0,
            bloom_bytes_len: 0,
            source_snapshot_lsn: 0, // invalid
        }],
    };
    let err = dir
        .encode_as_extension()
        .expect_err("zero snapshot LSN must be rejected");
    assert!(
        matches!(err, SegmentIndexError::InvalidExtension { .. }),
        "zero snapshot LSN must yield InvalidExtension, got: {err:?}"
    );
}

/// Verify that decoding an entry with non-zero reserved bytes is rejected.
#[test]
fn column_chunk_directory_decode_rejects_nonzero_reserved_bytes() {
    let mut ext_bytes = chunk_directory()
        .encode_as_extension()
        .expect("directory must encode");

    // Offset of reserved field in entry 0:
    // 8-byte ext header + 4-byte chunk_count + entry-0-start + 56 = 68
    let reserved_offset = 8 + 4 + 56;
    ext_bytes[reserved_offset] = 0x01; // corrupt reserved field

    let err = ColumnChunkDirectory::decode_from_extension_bytes(&ext_bytes)
        .expect_err("non-zero reserved bytes must be rejected");
    assert!(
        matches!(err, SegmentIndexError::InvalidExtension { .. }),
        "non-zero reserved bytes must yield InvalidExtension, got: {err:?}"
    );
}
